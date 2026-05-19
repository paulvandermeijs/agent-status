use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_agent-status")
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
    let state_dir = tmp.path().join("agent-status");

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
    assert!(stdout.starts_with("[!] "), "got: {stdout:?}");

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
    let state_dir = tmp.path().join("agent-status");
    let (_, stderr, code) = run(&state_dir, &["frobnicate"], None);
    assert_eq!(code, 2);
    assert!(!stderr.is_empty(), "expected non-empty stderr, got: {stderr:?}");
}

#[test]
fn set_with_empty_session_id_is_noop() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");
    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":""}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, _) = run(&state_dir, &["status"], None);
    assert_eq!(stdout, "");
}

#[test]
fn list_outputs_session_id_pane_display_columns() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":"sess-list","message":"Permission required"}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, code) = run(&state_dir, &["list"], None);
    assert_eq!(code, 0);
    let line = stdout.lines().next().expect("at least one line");
    let cols: Vec<&str> = line.split('\t').collect();
    assert_eq!(cols.len(), 3, "expected 3 columns, got: {cols:?}");
    assert_eq!(cols[0], "sess-list");
    // pane is empty in tests because TMUX_PANE is removed by `run`.
    assert_eq!(cols[1], "");
    // Display column starts with the [!]/[*] marker, not the raw event word.
    assert!(cols[2].starts_with("[!] "), "got: {:?}", cols[2]);
    assert!(!cols[2].contains("notify"), "event word leaked: {:?}", cols[2]);
    assert!(cols[2].contains("Permission required"));
}

#[test]
fn status_prunes_state_file_with_dead_pid() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");
    std::fs::create_dir_all(&state_dir).unwrap();

    let json = r#"{"agent":"claude-code","project":"ghost","cwd":"/x","event":"notify","tmux_pane":"","ts":1,"pid":1000000000}"#;
    std::fs::write(state_dir.join("ghost-session"), json).unwrap();
    assert!(state_dir.join("ghost-session").exists());

    let (stdout, _, code) = run(&state_dir, &["status"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout, "", "status should report no waiting sessions");
    assert!(
        !state_dir.join("ghost-session").exists(),
        "stale state file should have been pruned by the status read",
    );
}

#[test]
fn repeated_clear_is_idempotent_and_silent() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    // First clear of a never-set session: should be a clean no-op.
    let (stdout, stderr, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"ghost"}"#),
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "");

    // Second clear of the same session: also no-op.
    let (stdout, _, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"ghost"}"#),
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "", "second no-op clear must stay silent");

    // After a set, a clear should still work and a second clear is a no-op.
    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":"s"}"#),
    );
    assert_eq!(code, 0);
    let (stdout, _, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"s"}"#),
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "", "clear of a previously-set session must stay silent");
    let (stdout, _, code) = run(
        &state_dir,
        &["clear"],
        Some(r#"{"session_id":"s"}"#),
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "", "follow-up clear of cleared session must stay silent");
}

#[test]
fn status_keeps_state_file_with_live_pid() {
    // Companion to status_prunes_state_file_with_dead_pid: pins the inverse
    // invariant — entries owned by a live process survive the prune. Uses the
    // test runner's own pid, which is guaranteed alive for the duration of
    // the spawned subprocess.
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");
    std::fs::create_dir_all(&state_dir).unwrap();

    let live_pid = std::process::id();
    let json = format!(
        r#"{{"agent":"claude-code","project":"alive","cwd":"/x","event":"notify","tmux_pane":"","ts":1,"pid":{live_pid}}}"#
    );
    std::fs::write(state_dir.join("alive-session"), json).unwrap();

    let (stdout, _, code) = run(&state_dir, &["status"], None);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("[!] "), "live entry should appear in status, got: {stdout:?}");
    assert!(
        state_dir.join("alive-session").exists(),
        "live state file must not be pruned",
    );
}

#[test]
fn agent_extension_writes_file_and_prints_path() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (stdout, stderr, code) = run(&state_dir, &["agent-extension"], None);
    assert_eq!(code, 0, "stderr: {stderr}");

    // The printed path should point at the settings file inside XDG_RUNTIME_DIR.
    let printed_path = stdout.trim_end_matches('\n');
    let expected = state_dir.join("extensions").join("claude-code.json");
    assert_eq!(printed_path, expected.to_string_lossy());

    // File must exist with parseable JSON containing the hooks block.
    let contents = std::fs::read_to_string(&expected).expect("settings file written");
    let parsed: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    let hooks = parsed.get("hooks").expect("hooks key present");
    for event in [
        "Notification",
        "Stop",
        "UserPromptSubmit",
        "PreToolUse",
        "SessionStart",
        "SessionEnd",
    ] {
        assert!(hooks.get(event).is_some(), "missing hook event {event}");
    }
}

#[test]
fn agent_extension_unknown_agent_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");
    let (_, stderr, code) = run(&state_dir, &["agent-extension", "--agent", "frobnicator"], None);
    assert_eq!(code, 2, "clap parse error should exit 2");
    assert!(
        stderr.contains("invalid value 'frobnicator'") || stderr.contains("possible values"),
        "stderr: {stderr:?}",
    );
}

#[test]
fn agent_extension_pi_coding_agent_writes_ts_file() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (stdout, stderr, code) = run(
        &state_dir,
        &["agent-extension", "--agent", "pi-coding-agent"],
        None,
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let printed_path = stdout.trim_end_matches('\n');
    let expected = state_dir.join("extensions").join("pi-coding-agent.ts");
    assert_eq!(printed_path, expected.to_string_lossy());

    let contents = std::fs::read_to_string(&expected).expect("extension file written");
    assert!(
        contents.contains(r#"const BIN = ""#),
        "expected substituted BIN, got:\n{contents}",
    );
    assert!(
        !contents.contains("process.env.AGENT_STATUS_BIN ??"),
        "env-fallback should have been replaced",
    );
    assert!(contents.contains("export default function"));
    assert!(contents.contains("pi.on(\"agent_end\""));
}

#[test]
fn agent_extension_opencode_writes_ts_file() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (stdout, stderr, code) = run(
        &state_dir,
        &["agent-extension", "--agent", "opencode"],
        None,
    );
    assert_eq!(code, 0, "stderr: {stderr}");

    let printed_path = stdout.trim_end_matches('\n');
    let expected = state_dir.join("extensions").join("opencode.ts");
    assert_eq!(printed_path, expected.to_string_lossy());

    let contents = std::fs::read_to_string(&expected).expect("extension file written");
    assert!(
        contents.contains(r#"const BIN = ""#),
        "expected substituted BIN, got:\n{contents}",
    );
    assert!(
        !contents.contains("process.env.AGENT_STATUS_BIN ??"),
        "env-fallback should have been replaced",
    );
    assert!(contents.contains("AgentStatusPlugin"));
}

#[test]
fn working_status_is_recorded_but_hidden_from_indicator_and_list() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    // Record a working session.
    let (_, _, code) = run(
        &state_dir,
        &["set", "working"],
        Some(r#"{"session_id":"sess-work"}"#),
    );
    assert_eq!(code, 0);

    // The state file should exist.
    assert!(state_dir.join("sess-work").exists());

    // `status` should still print nothing (working doesn't surface).
    let (stdout, _, code) = run(&state_dir, &["status"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");

    // `list` should be empty too — working entries are for the switcher only.
    let (stdout, _, code) = run(&state_dir, &["list"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");

    // A second session that's actually waiting *should* surface.
    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":"sess-wait"}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, _) = run(&state_dir, &["status"], None);
    assert!(stdout.starts_with("[!] "), "got: {stdout:?}");
    let (stdout, _, _) = run(&state_dir, &["list"], None);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "got: {lines:?}");
    assert!(lines[0].contains("sess-wait"));
}
