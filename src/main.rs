mod agents;
mod commands;
mod state;

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

use commands::{build_entry, format_list, format_status};
use state::StateStore;

/// Tmux-integrated indicator showing which AI coding agent sessions are waiting on user input.
///
/// Each agent's hooks invoke `set`/`clear` with `--agent <name>`; `status` and `list` are
/// agent-neutral and aggregate state from every agent. Currently registered: `claude-code`,
/// `pi-coding-agent`. Claude Code's hooks: `set` on `Notification` / `Stop`; `clear` on
/// `UserPromptSubmit` / `SessionStart` / `SessionEnd`. tmux `status-right` calls `status`
/// periodically; the popup picker calls `list`.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Mark this agent session as waiting on user attention.
    ///
    /// Reads the hook event JSON from stdin and stores an entry keyed by `session_id`.
    /// If `session_id` is missing or empty, exits 0 silently.
    Set {
        /// Event label stored with the entry (e.g. `notify`, `done`).
        #[arg(default_value = "attention")]
        event: String,
        /// Identifier of the agent invoking the hook (e.g. `claude-code`).
        #[arg(long, default_value = "claude-code")]
        agent: String,
    },
    /// Clear this agent session's attention state.
    ///
    /// Reads the hook event JSON from stdin and removes the entry keyed by the agent's
    /// session identifier. If the field is missing or empty, exits 0 silently.
    Clear {
        /// Identifier of the agent invoking the hook (e.g. `claude-code`).
        #[arg(long, default_value = "claude-code")]
        agent: String,
    },
    /// Print the tmux status-right line. Empty output if no sessions are waiting.
    Status,
    /// Print TSV (pane, project, event) of all waiting sessions, one per line.
    List,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let store = StateStore::from_env();

    let result = match cli.command {
        Cmd::Set { event, agent } => run_set(&store, &agent, &event),
        Cmd::Clear { agent } => run_clear(&store, &agent),
        Cmd::Status => run_status(&store, &mut io::stdout().lock()),
        Cmd::List => run_list(&store, &mut io::stdout().lock()),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("agent-status: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_set(store: &StateStore, agent_name: &str, event: &str) -> io::Result<()> {
    let Some(agent) = agents::by_name(agent_name) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown agent: {agent_name}"),
        ));
    };

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

    let message = agent.extract_message(&buf);
    let entry = build_entry(agent.name(), event, &cwd, &pane, ts, message.as_deref());
    store.write(&session_id, &entry)?;
    refresh_tmux();
    Ok(())
}

fn run_clear(store: &StateStore, agent_name: &str) -> io::Result<()> {
    let Some(agent) = agents::by_name(agent_name) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown agent: {agent_name}"),
        ));
    };

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
