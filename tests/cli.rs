use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_claude-status")
}

fn run(state_dir: &std::path::Path, args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("XDG_RUNTIME_DIR", state_dir.parent().unwrap())
        .env_remove("CLAUDE_PROJECT_DIR")
        .env_remove("TMUX_PANE")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn binary");
    if let Some(s) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().expect("wait");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn end_to_end_set_status_clear() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("claude-status");

    let (stdout, _, code) = run(&state_dir, &["status"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");

    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":"sess-A"}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, code) = run(&state_dir, &["status"], None);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("#[fg=yellow,bold]🔔 "), "got: {stdout:?}");

    let (_, _, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"sess-A"}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, code) = run(&state_dir, &["status"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
}

#[test]
fn unknown_subcommand_exits_2() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("claude-status");
    let (_, stderr, code) = run(&state_dir, &["frobnicate"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown subcommand"));
}

#[test]
fn set_with_empty_session_id_is_noop() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("claude-status");
    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":""}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, _) = run(&state_dir, &["status"], None);
    assert_eq!(stdout, "");
}
