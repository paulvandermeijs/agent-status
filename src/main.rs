mod agents;
mod commands;
mod state;

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

use commands::{build_entry, build_settings_json, format_list, format_preview, format_status};
use state::StateStore;

/// Tmux-integrated indicator showing which AI coding agent sessions are waiting on user input.
///
/// Each agent's hooks invoke `set`/`clear` with `--agent <name>`; `status` and `list` are
/// agent-neutral and aggregate state from every agent. Currently registered: `claude-code`,
/// `pi-coding-agent`, `opencode`. Claude Code's hooks: `set` on `Notification` / `Stop`;
/// `clear` on `UserPromptSubmit` / `PreToolUse` / `SessionStart` / `SessionEnd`. tmux
/// `status-right` calls `status` periodically; the popup picker calls `list`.
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
    /// Print TSV (`session_id\tpane\tdisplay`) of all waiting sessions, one per line.
    List,
    /// Print a multi-line detail block for one session — used by fzf's `--preview`.
    ///
    /// If no entry matches `session_id`, exits 0 with empty output (the picker treats
    /// the preview as transient and recovers on the next selection).
    Preview {
        /// Session identifier as emitted in column 1 of `list`.
        session_id: String,
    },
    /// Generate the agent's extension/settings file and print its path.
    ///
    /// Intended for use as a shell alias (Claude Code: `alias claude='claude
    /// --settings "$(agent-status agent-extension)"'`; pi: `alias pi='pi -e
    /// "$(agent-status agent-extension --agent pi-coding-agent)"'`). Writes
    /// a fresh file (using the current `agent-status` binary's absolute path)
    /// to `${XDG_RUNTIME_DIR:-/tmp}/agent-status/extensions/<agent>.<ext>`
    /// and prints that path on stdout. Each agent emits the file type its
    /// loader expects (`.json` for Claude Code, `.ts` for pi/opencode).
    AgentExtension {
        /// Identifier of the agent the extension file should target.
        #[arg(long, default_value = "claude-code")]
        agent: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let store = StateStore::from_env();

    let result = match cli.command {
        Cmd::Set { event, agent } => run_set(&store, &agent, &event),
        Cmd::Clear { agent } => run_clear(&store, &agent),
        Cmd::Status => run_status(&store, &mut io::stdout().lock()),
        Cmd::List => run_list(&store, &mut io::stdout().lock()),
        Cmd::Preview { session_id } => {
            run_preview(&store, &session_id, &mut io::stdout().lock())
        }
        Cmd::AgentExtension { agent } => run_agent_extension(&agent, &mut io::stdout().lock()),
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
        .map_or(0, |d| d.as_secs());

    let message = agent.extract_message(&buf);
    let pid = std::os::unix::process::parent_id();
    let entry = build_entry(
        agent.name(),
        event,
        &cwd,
        &pane,
        ts,
        message.as_deref(),
        Some(pid),
    );
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
    // Only refresh tmux when we actually removed something. `PreToolUse` fires
    // on every tool call; the typical case is "no state file present, this is
    // a no-op clear" and we shouldn't redraw the status bar for that.
    if store.remove(&session_id)? {
        refresh_tmux();
    }
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

fn run_preview(store: &StateStore, session_id: &str, out: &mut impl Write) -> io::Result<()> {
    let entries = store.list()?;
    let Some((_, entry)) = entries.into_iter().find(|(sid, _)| sid == session_id) else {
        return Ok(());
    };
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    write!(out, "{}", format_preview(&entry, now_ts))?;
    Ok(())
}

fn run_agent_extension(agent_name: &str, out: &mut impl Write) -> io::Result<()> {
    let Some(agent) = agents::by_name(agent_name) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown agent: {agent_name}"),
        ));
    };
    let bin_path = std::env::current_exe()?;
    let bin_str = bin_path.to_string_lossy();
    let Some(json) = build_settings_json(&bin_str, agent.name()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("agent has no --settings integration: {agent_name}"),
        ));
    };
    let settings_path = settings_path_for(agent.name());
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&settings_path, json)?;
    writeln!(out, "{}", settings_path.display())?;
    Ok(())
}

fn settings_path_for(agent_name: &str) -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(|| std::path::PathBuf::from("/tmp"), std::path::PathBuf::from);
    base.join("agent-status")
        .join("extensions")
        .join(format!("{agent_name}.json"))
}

fn refresh_tmux() {
    let _ = std::process::Command::new("tmux")
        .args(["refresh-client", "-S"])
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
}
