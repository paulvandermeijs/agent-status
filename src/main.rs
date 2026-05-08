mod agents;
mod commands;
mod state;

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

use agents::Agent;
use agents::claude_code::ClaudeCodeAgent;
use commands::{build_entry, format_list, format_status};
use state::StateStore;

/// Tmux-integrated indicator showing which Claude Code sessions are waiting on user input.
///
/// Claude Code hooks call `set` (`Notification`, `Stop`) and `clear` (`UserPromptSubmit`,
/// `SessionStart`, `SessionEnd`) with the hook event payload on stdin. tmux `status-right`
/// calls `status` periodically; the popup picker calls `list`.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Mark this Claude session as waiting on user attention.
    ///
    /// Reads the hook event JSON from stdin and stores an entry keyed by `session_id`.
    /// If `session_id` is missing or empty, exits 0 silently.
    Set {
        /// Event label stored with the entry (e.g. `notify`, `done`).
        #[arg(default_value = "attention")]
        event: String,
    },
    /// Clear this Claude session's attention state.
    ///
    /// Reads the hook event JSON from stdin and removes the entry keyed by `session_id`.
    /// If `session_id` is missing or empty, exits 0 silently.
    Clear,
    /// Print the tmux status-right line. Empty output if no sessions are waiting.
    Status,
    /// Print TSV (pane, project, event) of all waiting sessions, one per line.
    List,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let store = StateStore::from_env();
    let agent: &dyn Agent = &ClaudeCodeAgent;
    debug_assert_eq!(agent.name(), "claude-code");
    // Placeholder until Task 5 wires --agent routing; keeps `by_name` reachable for clippy.
    let _ = agents::by_name("claude-code");

    let result = match cli.command {
        Cmd::Set { event } => run_set(&store, agent, &event),
        Cmd::Clear => run_clear(&store, agent),
        Cmd::Status => run_status(&store, &mut io::stdout().lock()),
        Cmd::List => run_list(&store, &mut io::stdout().lock()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("claude-status: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_set(store: &StateStore, agent: &dyn Agent, event: &str) -> io::Result<()> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;

    let Some(session_id) = agent.extract_session_id(&buf) else {
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

fn run_clear(store: &StateStore, agent: &dyn Agent) -> io::Result<()> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let Some(session_id) = agent.extract_session_id(&buf) else {
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
