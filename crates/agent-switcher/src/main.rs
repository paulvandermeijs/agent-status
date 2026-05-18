//! Tmux popup TUI for switching between waiting AI coding agent sessions.

// `app` and `filter` are wired into the binary's event loop in Task 13. Until
// then `main` doesn't reach them, but the tests do — silence the bin-crate
// dead-code lint without masking the same lint inside the modules themselves.
#[allow(dead_code)]
mod app;
#[allow(dead_code)]
mod filter;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("agent-switcher: not implemented yet");
    ExitCode::from(1)
}
