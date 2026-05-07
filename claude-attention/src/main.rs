mod commands;
mod state;

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use commands::{build_entry, extract_session_id, format_list, format_status};
use state::StateStore;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let store = StateStore::from_env();

    let result = match args.get(1).map(String::as_str) {
        Some("set") => {
            let event = args.get(2).map(String::as_str).unwrap_or("attention");
            run_set(&store, event)
        }
        Some("clear") => run_clear(&store),
        Some("status") => run_status(&store, &mut io::stdout().lock()),
        Some("list") => run_list(&store, &mut io::stdout().lock()),
        Some(other) => {
            eprintln!("claude-attention: unknown subcommand: {other}");
            return ExitCode::from(2);
        }
        None => {
            eprintln!("usage: claude-attention <set [event]|clear|status|list>");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("claude-attention: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_set(store: &StateStore, event: &str) -> io::Result<()> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;

    let Some(session_id) = extract_session_id(&buf) else {
        return Ok(());
    };

    let cwd = std::env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let pane = std::env::var("TMUX_PANE").unwrap_or_default();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let entry = build_entry(event, &cwd, &pane, ts);
    store.write(&session_id, &entry)?;
    refresh_tmux();
    Ok(())
}

fn run_clear(store: &StateStore) -> io::Result<()> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let Some(session_id) = extract_session_id(&buf) else {
        return Ok(());
    };
    store.remove(&session_id)?;
    refresh_tmux();
    Ok(())
}

fn run_status(store: &StateStore, out: &mut impl Write) -> io::Result<()> {
    let entries = store.list()?;
    if let Some(line) = format_status(&entries) {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

fn run_list(store: &StateStore, out: &mut impl Write) -> io::Result<()> {
    let entries = store.list()?;
    write!(out, "{}", format_list(&entries))?;
    Ok(())
}

fn refresh_tmux() {
    let _ = std::process::Command::new("tmux")
        .args(["refresh-client", "-S"])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
}
