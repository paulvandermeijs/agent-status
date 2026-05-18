//! Tmux popup TUI for switching between waiting AI coding agent sessions.

mod app;
mod filter;
mod ui;

use std::io::{self, stdout};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use agent_status::StateStore;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{App, KeyOutcome};

const TICK_RATE: Duration = Duration::from_millis(250);

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("agent-switcher: {e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> io::Result<()> {
    let store = StateStore::from_env();
    let mut app = App::new(store);

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app);

    // Always tear the terminal down, even on error — otherwise the user's
    // shell is left in raw mode after a panic.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    match result? {
        ExitReason::Cancel => {}
        ExitReason::Activate => activate(&app),
    }
    Ok(())
}

enum ExitReason {
    Cancel,
    Activate,
}

fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<ExitReason> {
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match app.handle_key(key) {
                    KeyOutcome::Continue => {}
                    KeyOutcome::Cancel => return Ok(ExitReason::Cancel),
                    KeyOutcome::Activate => return Ok(ExitReason::Activate),
                }
            }
        }
        if last_tick.elapsed() >= TICK_RATE {
            app.tick();
            last_tick = Instant::now();
        }
    }
}

fn activate(app: &App) {
    let Some((_, entry)) = app.selected_entry() else {
        return;
    };
    if entry.tmux_pane.is_empty() {
        return;
    }
    let _ = Command::new("tmux")
        .args(["switch-client", "-t", &entry.tmux_pane])
        .status();
}
