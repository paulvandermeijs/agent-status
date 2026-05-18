# agent-switcher TUI (ratatui) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the fzf-based tmux popup picker with a dedicated `agent-switcher` TUI binary built on ratatui. Restructure the project into a Cargo workspace with two crates (`crates/agent-status`, `crates/agent-switcher`), expose the existing state/commands code as a library, and add a `working` status to track in-flight Claude Code sessions so the switcher can render a spinner for them.

**Architecture:** Three coordinated changes that build on each other.

1. **Workspace split.** The repo becomes a Cargo workspace. `crates/agent-status` keeps the existing hook-facing binary but also exposes a library (`state`, `commands`, `agents`) for other crates to consume. `crates/agent-switcher` is a new binary that depends on `agent-status` as a library and on `ratatui`/`crossterm` for the TUI. Lints, edition, and release profile move to the workspace manifest.

2. **`working` status.** `AttentionEntry.event` accepts a third value `"working"` alongside `"notify"`/`"done"`. The Claude Code extension's `UserPromptSubmit` and `PreToolUse` hooks switch from `clear` to `set working` so an active turn is recorded in the state directory while the agent is mid-flight. The tmux indicator (`agent-status status`) and the legacy `list` command filter out `working` so the existing bar contract is preserved — only the new switcher surfaces working sessions. The fzf-only `preview` subcommand is dropped in this same phase (no remaining consumer once the popup binding is replaced). pi/opencode hook semantics are unchanged in this plan; they can adopt `working` later.

3. **TUI switcher.** `agent-switcher` opens a fullscreen alternate-screen terminal, polls the state directory every 250 ms, and renders three regions: a filter input at the top, a sessions table in the middle, a help strip at the bottom. The table columns are status (spinner for working, `!` for notify, `✓` for done), session/project name, agent type, and a single-line snippet of the last message. Filter input is captured character-by-character; <kbd>Ctrl-N</kbd>/<kbd>Ctrl-P</kbd> move the selection; <kbd>Enter</kbd> runs `tmux switch-client -t <pane>` and exits; <kbd>Esc</kbd> / <kbd>Ctrl-C</kbd> exit without switching. The tmux popup binding in `README.md` becomes a one-liner that invokes `agent-switcher` directly.

**Tech Stack:** Rust 2021 (Cargo workspaces, clap derive, serde, serde_json, tempfile), `ratatui = "0.29"` + `crossterm = "0.28"` for the TUI, tmux for pane switching.

---

## Scope check

Three subsystems live in this plan because the user asked for them together and each has a non-trivial dependency on the previous one (the switcher needs the library split; the spinner needs the working status). If you want a smaller deliverable, the natural split is:

- Phase A (workspace + library split, Tasks 1–4) is independently shippable: same functionality, cleaner structure.
- Phase B (`working` status, Tasks 5–8) is independently shippable: adds a new event value behind unchanged user-facing output.
- Phase C (`agent-switcher` binary, Tasks 9–14) depends on Phase A only; if Phase B isn't done, the spinner renders for any entry whose `event == "working"` (i.e. never).

The plan below is written so each phase ends in a green build and green test suite.

## File Structure

**New layout after this plan:**

```
agent-status/                              (workspace root, was crate root)
├── Cargo.toml                             (workspace manifest, no [package])
├── Cargo.lock
├── README.md                              (updated)
├── CLAUDE.md                              (updated)
├── crates/
│   ├── agent-status/                      (existing crate, now lib+bin)
│   │   ├── Cargo.toml
│   │   ├── extensions/
│   │   │   ├── opencode.ts                (moved)
│   │   │   └── pi-coding-agent.ts         (moved)
│   │   ├── src/
│   │   │   ├── lib.rs                     (new — re-exports modules)
│   │   │   ├── main.rs                    (was src/main.rs, imports updated)
│   │   │   ├── state.rs                   (moved)
│   │   │   ├── commands.rs                (moved)
│   │   │   └── agents/                    (moved)
│   │   │       ├── mod.rs
│   │   │       ├── claude_code.rs
│   │   │       ├── opencode.rs
│   │   │       └── pi_coding_agent.rs
│   │   └── tests/
│   │       └── cli.rs                     (moved)
│   └── agent-switcher/                    (new crate, bin only)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                    (terminal setup + event loop)
│           ├── app.rs                     (App state + key handling)
│           ├── ui.rs                      (ratatui rendering)
│           └── filter.rs                  (filter logic, pure & testable)
└── docs/
    └── superpowers/plans/                  (unchanged)
```

**Module responsibilities (existing constraints preserved):**

- `state.rs` — all filesystem I/O, `AttentionEntry`, `StateStore`.
- `commands.rs` — pure helpers (`build_entry`, `format_status`, `format_list`, `build_extension`).
- `main.rs` (agent-status) — clap glue, impure adapter.
- `agents/*` — per-agent JSON parsing.
- `app.rs` (agent-switcher) — pure-ish app state; the `tick()` reads `StateStore` but key handling is testable in isolation.
- `ui.rs` (agent-switcher) — pure rendering, takes `&App`.
- `filter.rs` (agent-switcher) — pure filter/match logic, fully testable.
- `main.rs` (agent-switcher) — terminal raw-mode setup, event loop, exit handling.

---

## Task 1: Convert to a Cargo workspace

**Files:**
- Create: `Cargo.toml` (workspace root, will replace existing single-crate one)
- Create: `crates/agent-status/Cargo.toml`
- Move (rename): all `src/**` → `crates/agent-status/src/**`
- Move (rename): all `tests/**` → `crates/agent-status/tests/**`
- Move (rename): all `extensions/**` → `crates/agent-status/extensions/**`

- [ ] **Step 1: Move the existing crate into `crates/agent-status/`**

Run from repo root:

```sh
mkdir -p crates/agent-status
git mv src crates/agent-status/src
git mv tests crates/agent-status/tests
git mv extensions crates/agent-status/extensions
git mv Cargo.toml crates/agent-status/Cargo.toml
```

`Cargo.lock` stays at the repo root — it's per-workspace.

- [ ] **Step 2: Write the workspace `Cargo.toml` at the repo root**

Create `Cargo.toml` with this content:

```toml
[workspace]
resolver = "2"
members = ["crates/agent-status", "crates/agent-switcher"]

[workspace.package]
version = "0.3.0"
edition = "2021"

[workspace.lints.rust]
unsafe_code = "forbid"
nonstandard_style = { level = "deny", priority = -1 }

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

Note `members` lists both crates even though `agent-switcher` doesn't exist yet — cargo will complain about the missing member until Task 9 creates it. To unblock the in-progress test runs for Tasks 1–8, **temporarily** set `members = ["crates/agent-status"]` and add `agent-switcher` back in Task 9 Step 1.

- [ ] **Step 3: Rewrite `crates/agent-status/Cargo.toml` for the workspace**

Replace the contents of `crates/agent-status/Cargo.toml` with:

```toml
[package]
name = "agent-status"
version.workspace = true
edition.workspace = true
description = "Tmux-integrated indicator showing which AI coding agent sessions are waiting on user input."

[lints]
workspace = true

[lib]
path = "src/lib.rs"

[[bin]]
name = "agent-status"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
```

Note: no `[profile.*]` or `[lints.*]` — both inherit from the workspace.

- [ ] **Step 4: Run `cargo check` to confirm the workspace builds**

Run:

```sh
cargo check
```

Expected: clean build of `agent-status`. If you see "no targets specified in the manifest" for `agent-switcher`, you forgot the temporary `members = ["crates/agent-status"]` override from Step 2.

- [ ] **Step 5: Run the full test suite to confirm the move was clean**

Run:

```sh
cargo test
```

Expected: all 100 tests pass (87 unit + 13 integration).

- [ ] **Step 6: Commit**

```sh
git add -A
git commit -m "refactor: convert to a Cargo workspace with crates/agent-status"
```

---

## Task 2: Split `agent-status` into a library + binary

**Files:**
- Create: `crates/agent-status/src/lib.rs`
- Modify: `crates/agent-status/src/main.rs`

The library and the binary live in the same crate. `lib.rs` owns all submodules; `main.rs` imports from the library via `agent_status::…` (NOT `crate::…`, because in a crate that has both a lib and a bin, the binary cannot see the library's modules through `crate::` — it sees them through the library's name).

- [ ] **Step 1: Create `crates/agent-status/src/lib.rs`**

Write:

```rust
//! Library face of the `agent-status` crate.
//!
//! Exposes the pieces other workspace members (notably `agent-switcher`) need:
//! the on-disk entry shape and state store, the pure formatting helpers, and
//! the registered-agent registry.

pub mod agents;
pub mod commands;
pub mod state;
```

- [ ] **Step 2: Rewrite the top of `crates/agent-status/src/main.rs` to import from the library**

In `crates/agent-status/src/main.rs`, replace lines 1–13 (the existing `mod agents; mod commands; mod state;` block and the local `use commands::…; use state::…;` lines) with:

```rust
use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

use agent_status::commands::{build_entry, build_extension, format_list, format_preview, format_status};
use agent_status::state::StateStore;
use agent_status::agents;
```

Leave the rest of `main.rs` unchanged.

- [ ] **Step 3: Run `cargo build` to verify the binary still compiles against the library**

Run:

```sh
cargo build
```

Expected: clean build. If you see `error[E0432]: unresolved import 'crate::…'` somewhere, the bin still has a stale `crate::` path — switch it to `agent_status::`.

- [ ] **Step 4: Run the full test suite**

Run:

```sh
cargo test
```

Expected: all 100 tests pass. The `tests/cli.rs` integration test uses `env!("CARGO_BIN_EXE_agent-status")` which resolves to the new bin path automatically.

- [ ] **Step 5: Run clippy**

Run:

```sh
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean. If pedantic clippy flags anything new from the import change, fix it inline.

- [ ] **Step 6: Commit**

```sh
git add -A
git commit -m "refactor(agent-status): split into lib+bin crate, expose state/commands/agents"
```

---

## Task 3: Make the lib's public types `pub` and add a doctest-friendly re-export

**Files:**
- Modify: `crates/agent-status/src/lib.rs`
- Modify: `crates/agent-status/src/state.rs`
- Modify: `crates/agent-status/src/commands.rs`

The submodules and the types they expose are already `pub`. The only thing missing is convenience re-exports at the crate root so downstream code can write `agent_status::AttentionEntry` instead of `agent_status::state::AttentionEntry`.

- [ ] **Step 1: Add re-exports to `lib.rs`**

Update `crates/agent-status/src/lib.rs` to:

```rust
//! Library face of the `agent-status` crate.

pub mod agents;
pub mod commands;
pub mod state;

pub use agents::{Agent, by_name};
pub use commands::{
    build_entry, build_extension, format_list, format_status, ExtensionFile,
};
pub use state::{AttentionEntry, StateStore};
```

- [ ] **Step 2: Run `cargo build` and the test suite**

Run:

```sh
cargo build
cargo test
```

Expected: clean build, all 100 tests pass. Re-exports don't change behavior; they're purely additive.

- [ ] **Step 3: Run clippy**

Run:

```sh
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 4: Commit**

```sh
git add -A
git commit -m "refactor(agent-status): re-export public API at the crate root"
```

---

## Task 4: Add `working` as a valid status, filtered from `format_status` and `format_list`

**Files:**
- Modify: `crates/agent-status/src/commands.rs`

`AttentionEntry.event` is a plain `String`, so no schema change is needed to accept a new value — only the consumers (`format_status`, `format_list`) need to learn to skip `working` entries. The new switcher will read entries directly and handle `working` itself.

- [ ] **Step 1: Write failing tests for the filter behavior**

In `crates/agent-status/src/commands.rs`, inside `mod tests`, add (next to the existing `format_status_*` tests):

```rust
    #[test]
    fn format_status_ignores_working_entries() {
        let mut e = entry("alpha", "%1", "working");
        e.event = "working".into();
        // A working-only entry should produce no indicator at all.
        assert_eq!(format_status(&[("s1".into(), e)]), None);
    }

    #[test]
    fn format_status_counts_only_non_working_entries() {
        let mut working = entry("alpha", "%1", "working");
        working.event = "working".into();
        let waiting = entry("beta", "%2", "notify");
        let entries = vec![
            ("s1".into(), working),
            ("s2".into(), waiting),
        ];
        // Only the waiting entry should count toward the status line.
        assert_eq!(format_status(&entries).as_deref(), Some("[!] beta"));
    }

    #[test]
    fn format_list_ignores_working_entries() {
        let mut working = entry("alpha", "%1", "working");
        working.event = "working".into();
        let waiting = entry("beta", "%2", "notify");
        let out = format_list(&[
            ("s1".into(), working),
            ("s2".into(), waiting),
        ]);
        // The working row must not appear; only the notify row should.
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1, "got: {lines:?}");
        assert!(lines[0].contains("beta"));
        assert!(!lines[0].contains("alpha"));
    }
```

- [ ] **Step 2: Run the new tests to verify they fail**

Run:

```sh
cargo test --package agent-status format_status_ignores_working_entries format_status_counts_only_non_working_entries format_list_ignores_working_entries
```

Expected: 3 failures, because `format_status`/`format_list` currently include every entry regardless of `event`.

- [ ] **Step 3: Update `format_status` to skip working entries**

Replace the body of `format_status` in `crates/agent-status/src/commands.rs` with:

```rust
pub fn format_status(entries: &[(String, AttentionEntry)]) -> Option<String> {
    let waiting: Vec<&AttentionEntry> = entries
        .iter()
        .filter(|(_, e)| e.event != "working")
        .map(|(_, e)| e)
        .collect();
    match waiting.len() {
        0 => None,
        1 => Some(format!("[!] {}", waiting[0].project)),
        n => Some(format!("[!] {n} projects waiting")),
    }
}
```

- [ ] **Step 4: Update `format_list` to skip working entries**

In `format_list`, immediately after the `if entries.is_empty() { return String::new(); }` guard, add a filter. Replace the existing function body's pad-and-render loop so that the per-entry loop iterates only over non-working entries:

```rust
pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    const PROJECT_CAP: usize = 30;
    const AGENT_CAP: usize = 16;
    const MESSAGE_CAP: usize = 80;

    let visible: Vec<&(String, AttentionEntry)> = entries
        .iter()
        .filter(|(_, e)| e.event != "working")
        .collect();
    if visible.is_empty() {
        return String::new();
    }

    let project_width = visible
        .iter()
        .map(|(_, e)| e.project.chars().count().min(PROJECT_CAP))
        .max()
        .unwrap_or(0);
    let agent_width = visible
        .iter()
        .map(|(_, e)| e.agent.chars().count().min(AGENT_CAP))
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for (sid, e) in &visible {
        let marker = if e.event == "notify" { "[!]" } else { "[*]" };
        let project = truncate_chars(&e.project, PROJECT_CAP);
        let agent = truncate_chars(&e.agent, AGENT_CAP);
        let snippet = e
            .message
            .as_deref()
            .map(|m| one_line(m, MESSAGE_CAP))
            .unwrap_or_default();

        let mut display = format!("{marker} {project:<project_width$}  {agent:<agent_width$}");
        if !snippet.is_empty() {
            display.push_str("  ");
            display.push_str(&snippet);
        }

        out.push_str(sid);
        out.push('\t');
        out.push_str(&e.tmux_pane);
        out.push('\t');
        out.push_str(display.trim_end());
        out.push('\n');
    }
    out
}
```

- [ ] **Step 5: Run the new tests to confirm they pass and the whole suite is green**

Run:

```sh
cargo test
```

Expected: all 103 tests pass (100 existing + 3 new). If any existing test fails, it's likely one that constructs entries with `event = "working"` for some unrelated reason — re-read its intent before changing it.

- [ ] **Step 6: Run clippy**

Run:

```sh
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 7: Commit**

```sh
git add -A
git commit -m "feat(agent-status): treat 'working' as a non-waiting status, filter from indicator"
```

---

## Task 5: Generate Claude Code hooks that record `working` instead of clearing

**Files:**
- Modify: `crates/agent-status/src/commands.rs` (function `build_claude_code_settings`)

Today the generator wires `UserPromptSubmit` / `PreToolUse` / `SessionStart` / `SessionEnd` to `clear --agent claude-code`. To track the working state we need `UserPromptSubmit` and `PreToolUse` to instead `set --agent claude-code working`. `SessionStart` / `SessionEnd` still genuinely terminate the entry, so they stay on `clear`.

`set` always writes a fresh state file (overwriting any prior `notify` / `done` value), so the post-permission transition that `PreToolUse: clear` used to provide is still correct: the entry transitions from `notify` → `working` rather than `notify` → absent.

- [ ] **Step 1: Write failing tests for the new hook wiring**

In `crates/agent-status/src/commands.rs`, inside `mod tests`, add (next to the existing `build_extension_claude_code_*` tests):

```rust
    #[test]
    fn build_extension_claude_code_user_prompt_submit_sets_working() {
        let ext = build_extension("/path/agent-status", "claude-code").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        let cmd = parsed
            .pointer("/hooks/UserPromptSubmit/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("UserPromptSubmit command");
        assert!(
            cmd.contains("set --agent claude-code working"),
            "got: {cmd}",
        );
    }

    #[test]
    fn build_extension_claude_code_pre_tool_use_sets_working() {
        let ext = build_extension("/path/agent-status", "claude-code").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        let cmd = parsed
            .pointer("/hooks/PreToolUse/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("PreToolUse command");
        assert!(
            cmd.contains("set --agent claude-code working"),
            "got: {cmd}",
        );
    }

    #[test]
    fn build_extension_claude_code_session_lifecycle_still_clears() {
        // SessionStart and SessionEnd remain `clear` — they end the session,
        // they don't represent active work.
        let ext = build_extension("/path/agent-status", "claude-code").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        for event in ["SessionStart", "SessionEnd"] {
            let cmd = parsed
                .pointer(&format!("/hooks/{event}/0/hooks/0/command"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| panic!("missing {event} command"));
            assert!(
                cmd.contains("clear --agent claude-code"),
                "{event} should still clear; got: {cmd}",
            );
        }
    }
```

- [ ] **Step 2: Run the new tests to verify they fail**

Run:

```sh
cargo test build_extension_claude_code_user_prompt_submit_sets_working build_extension_claude_code_pre_tool_use_sets_working build_extension_claude_code_session_lifecycle_still_clears
```

Expected: two failures (the working ones) and one pass (the session-lifecycle one, since `SessionStart` / `SessionEnd` already use `clear`).

- [ ] **Step 3: Update `build_claude_code_settings` to emit the working-set hooks**

Replace the body of `build_claude_code_settings` in `crates/agent-status/src/commands.rs` with:

```rust
fn build_claude_code_settings(bin_path: &str) -> String {
    let set_notify = format!("{bin_path} set --agent claude-code notify");
    let set_done = format!("{bin_path} set --agent claude-code done");
    let set_working = format!("{bin_path} set --agent claude-code working");
    let clear = format!("{bin_path} clear --agent claude-code");

    let value = serde_json::json!({
        "hooks": {
            "Notification":     [{"hooks": [{"type": "command", "command": set_notify}]}],
            "Stop":             [{"hooks": [{"type": "command", "command": set_done}]}],
            "UserPromptSubmit": [{"hooks": [{"type": "command", "command": set_working}]}],
            "PreToolUse":       [{"hooks": [{"type": "command", "command": set_working}]}],
            "SessionStart":     [{"hooks": [{"type": "command", "command": clear}]}],
            "SessionEnd":       [{"hooks": [{"type": "command", "command": clear}]}],
        }
    });
    serde_json::to_string_pretty(&value).expect("serde_json::Value always serializes")
}
```

- [ ] **Step 4: Run the new tests to confirm they pass**

Run:

```sh
cargo test build_extension_claude_code
```

Expected: all `build_extension_claude_code_*` tests pass.

- [ ] **Step 5: Run the integration tests to confirm nothing downstream broke**

Run:

```sh
cargo test --test cli
```

Expected: all 13 integration tests pass. The existing `agent_extension_writes_file_and_prints_path` test asserts that all six hook events are present; the new wiring still lists all six, so it stays green.

- [ ] **Step 6: Run the full suite + clippy**

Run:

```sh
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 7: Commit**

```sh
git add -A
git commit -m "feat(agent-status): claude-code UserPromptSubmit/PreToolUse hooks record 'working'"
```

---

## Task 6: Acceptance test — a `working` entry round-trips through `set`/`status`/`list`

**Files:**
- Modify: `crates/agent-status/tests/cli.rs`

We have unit-level coverage of the filter and the hook generator, but no end-to-end coverage of the new value through the binary. Add one integration test exercising it.

- [ ] **Step 1: Write the failing integration test**

In `crates/agent-status/tests/cli.rs`, add at the end (after `agent_extension_opencode_writes_ts_file`):

```rust
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
```

- [ ] **Step 2: Run the new test to verify it passes (it should, given the work in Tasks 4–5)**

Run:

```sh
cargo test --test cli working_status_is_recorded_but_hidden_from_indicator_and_list
```

Expected: PASS. If it fails, double-check Tasks 4 and 5 — this test verifies the end-to-end consequence of those changes.

- [ ] **Step 3: Run the full suite**

Run:

```sh
cargo test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```sh
git add -A
git commit -m "test(agent-status): working entries do not surface in status/list output"
```

---

## Task 7: Remove the `preview` CLI subcommand and `format_preview` helper

**Files:**
- Modify: `crates/agent-status/src/main.rs`
- Modify: `crates/agent-status/src/commands.rs`
- Modify: `crates/agent-status/tests/cli.rs`

The `preview` subcommand existed solely to drive fzf's `--preview` window. With the fzf binding being replaced by `agent-switcher` (Task 15), `preview` has no remaining consumer in this repo and `format_preview` is unused. Drop both. `list` stays — it's a generic TSV surface that external scripts may rely on.

- [ ] **Step 1: Remove the `Preview` variant from the clap enum**

In `crates/agent-status/src/main.rs`, delete the `Preview { session_id }` variant from `enum Cmd` (the entire block starting at the `/// Print a multi-line detail block for one session — used by fzf's \`--preview\`.` doc comment through the closing brace of the variant), and delete the `Cmd::Preview { session_id } => …` arm from the `match cli.command` block inside `main()`.

- [ ] **Step 2: Remove the `run_preview` function**

In `crates/agent-status/src/main.rs`, delete the entire `run_preview` function definition (the `fn run_preview(…) -> io::Result<()> { … }` block).

- [ ] **Step 3: Drop `format_preview` from the import line**

In `crates/agent-status/src/main.rs`, remove `format_preview` from the `use agent_status::commands::{…};` line so the import reads:

```rust
use agent_status::commands::{build_entry, build_extension, format_list, format_status};
```

- [ ] **Step 4: Delete `format_preview` and its helper from `commands.rs`**

In `crates/agent-status/src/commands.rs`, delete:

- The `format_preview` function (its `///` doc through its closing `}`).
- The private `format_age` helper (only used by `format_preview`).
- The five `format_preview_*` unit tests inside `mod tests`:
  - `format_preview_includes_core_fields`
  - `format_preview_omits_message_section_when_none`
  - `format_preview_includes_message_section_when_some`
  - `format_preview_age_handles_seconds_minutes_hours_days`
  - `format_preview_age_clamps_when_now_before_ts`

- [ ] **Step 5: Delete the preview integration tests**

In `crates/agent-status/tests/cli.rs`, delete two tests:

- `preview_prints_multi_line_detail_for_known_session`
- `preview_unknown_session_id_exits_zero_with_empty_output`

- [ ] **Step 6: Run the full test suite + clippy**

Run:

```sh
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean. The `agent-status` crate now has 88 unit + 12 integration = 100 tests (down from 107 after this task).

- [ ] **Step 7: Commit**

```sh
git add -A
git commit -m "refactor(agent-status): drop preview subcommand and format_preview (no remaining consumers)"
```

---

## Task 8: Update CLAUDE.md to document the workspace layout, `working` status, and lib API

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update the "Build / test / lint" section**

Replace the existing "Build / test / lint" block in `CLAUDE.md` with:

```markdown
## Build / test / lint

This repo is a Cargo workspace. The binaries live in `crates/agent-status` (the
hook-facing CLI, also exposes a library) and `crates/agent-switcher` (the
ratatui TUI popup).

```sh
cargo test                                                            # workspace-wide
cargo clippy --all-targets --all-features --locked -- -D warnings     # required gate
cargo build --release                                                 # both binaries
```

Run a single unit test or a specific integration test:

```sh
cargo test -p agent-status entry_roundtrips_through_json
cargo test -p agent-status --test cli end_to_end_set_status_clear
cargo test -p agent-switcher filter_matches_project_substring
```

Workspace-level `[workspace.lints]` enforce `unsafe_code = "forbid"`,
`nonstandard_style = "deny"`, `clippy::all = "deny"`, `clippy::pedantic = "warn"`
across both crates.
```

- [ ] **Step 2: Update the "Module split" section**

Replace the "Module split (load-bearing)" section with:

```markdown
## Module split (load-bearing)

`agent-status` is split into a library and a binary in the same crate:

- `crates/agent-status/src/state.rs` — owns all filesystem I/O. `AttentionEntry`
  and `StateStore`. Tests use `tempfile::TempDir` for isolation.
- `crates/agent-status/src/commands.rs` — pure helpers (`build_entry`,
  `format_status`, `format_list`, `build_extension`). No `std::env`,
  `std::io`, `std::time`, or `std::fs` imports.
- `crates/agent-status/src/main.rs` — clap glue, impure adapter; imports from
  the library via `agent_status::…`.
- `crates/agent-status/src/lib.rs` — module declarations + crate-root
  re-exports (`AttentionEntry`, `StateStore`, `Agent`, etc.) consumed by
  `agent-switcher`.
- `crates/agent-status/src/agents/…` — per-agent JSON parsing.

`agent-switcher` is binary-only:

- `crates/agent-switcher/src/main.rs` — terminal setup + crossterm event loop.
- `crates/agent-switcher/src/app.rs` — `App` state and key-event reducer.
- `crates/agent-switcher/src/ui.rs` — ratatui rendering (pure, takes `&App`).
- `crates/agent-switcher/src/filter.rs` — pure filter/match logic.

Both share the `agent-status` library, which is the only crate that depends on
serde/clap/serde_json.
```

- [ ] **Step 3: Add a `working` paragraph to "Wire compatibility"**

After the existing `pid` paragraph in the "Wire compatibility" section, add:

```markdown
The `event` field accepts a third value `"working"` in addition to `"notify"`
and `"done"`. The Claude Code extension's `UserPromptSubmit` and `PreToolUse`
hooks emit `set working` so an in-flight session is recorded in the state
directory. `format_status` and `format_list` filter `working` entries out, so
the tmux indicator and the `list` TSV output are unchanged. `agent-switcher`
is the only consumer that surfaces working entries (with a spinner). pi and
opencode do not yet emit `working`; their hook semantics are unchanged.
```

- [ ] **Step 4: Update the "Dev / installed binary divergence" section**

The reinstall command needs to copy both binaries. Replace the existing snippet with:

```markdown
```sh
cargo build --release
install -m 0755 target/release/agent-status   ~/.claude/bin/agent-status
install -m 0755 target/release/agent-switcher ~/.claude/bin/agent-switcher
```
```

- [ ] **Step 5: Update the test count near the top**

The current header says `100 tests (87 unit + 13 integration)`. Update the comment in the `cargo test` line to reflect the final post-plan totals (3 new `commands` filter tests, 3 new `build_extension` tests, 5 removed `format_preview` tests; 1 new `cli` integration test, 2 removed `cli` preview tests; 7 + 10 + 3 new `agent-switcher` unit tests → `120 tests (108 unit + 12 integration)`).

- [ ] **Step 6: Commit**

```sh
git add -A
git commit -m "docs(claude.md): document workspace layout, working status, switcher crate"
```

---

## Task 9: Scaffold the `agent-switcher` crate

**Files:**
- Modify: `Cargo.toml` (workspace root — re-enable the `agent-switcher` member)
- Create: `crates/agent-switcher/Cargo.toml`
- Create: `crates/agent-switcher/src/main.rs`

- [ ] **Step 1: Re-enable the workspace member if you temporarily removed it in Task 1**

In the root `Cargo.toml`, ensure `members` lists both crates:

```toml
members = ["crates/agent-status", "crates/agent-switcher"]
```

- [ ] **Step 2: Create `crates/agent-switcher/Cargo.toml`**

Write:

```toml
[package]
name = "agent-switcher"
version.workspace = true
edition.workspace = true
description = "Tmux popup TUI for switching between waiting AI coding agent sessions."

[lints]
workspace = true

[[bin]]
name = "agent-switcher"
path = "src/main.rs"

[dependencies]
agent-status = { path = "../agent-status" }
ratatui = "0.29"
crossterm = "0.28"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create a minimal `crates/agent-switcher/src/main.rs` so the crate builds**

Write:

```rust
//! Tmux popup TUI for switching between waiting AI coding agent sessions.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("agent-switcher: not implemented yet");
    ExitCode::from(1)
}
```

- [ ] **Step 4: Run `cargo build` to confirm the new crate compiles**

Run:

```sh
cargo build -p agent-switcher
```

Expected: clean build. Some warnings about unused dependencies are OK at this stage.

- [ ] **Step 5: Run the full test suite to confirm the workspace is still green**

Run:

```sh
cargo test
```

Expected: all `agent-status` tests still pass; `agent-switcher` has no tests yet.

- [ ] **Step 6: Commit**

```sh
git add -A
git commit -m "feat(agent-switcher): scaffold the new crate"
```

---

## Task 10: Implement the pure filter logic (`filter.rs`)

**Files:**
- Create: `crates/agent-switcher/src/filter.rs`
- Modify: `crates/agent-switcher/src/main.rs` (declare the module)

The filter is pure and standalone — easiest piece to TDD before any terminal code.

- [ ] **Step 1: Declare the module in `main.rs`**

Replace `crates/agent-switcher/src/main.rs` with:

```rust
//! Tmux popup TUI for switching between waiting AI coding agent sessions.

mod filter;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("agent-switcher: not implemented yet");
    ExitCode::from(1)
}
```

- [ ] **Step 2: Write the failing tests in `filter.rs`**

Create `crates/agent-switcher/src/filter.rs`:

```rust
//! Pure filter logic for the switcher's list. Lives outside `app.rs` so it can
//! be unit-tested without touching `AttentionEntry`/`StateStore`.

/// Subset of [`agent_status::AttentionEntry`] fields the filter cares about.
/// Borrowing-only so callers don't pay for clones during filter evaluation.
#[derive(Debug, Clone, Copy)]
pub struct FilterRow<'a> {
    pub session_id: &'a str,
    pub project: &'a str,
    pub agent: &'a str,
    pub message: Option<&'a str>,
}

/// Return `true` if `row` should be visible given the user's filter text.
///
/// Matching is case-insensitive substring across `project`, `agent`, `message`,
/// and `session_id`. An empty filter matches everything.
#[must_use]
pub fn matches(row: FilterRow<'_>, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let needle = filter.to_lowercase();
    let haystacks = [
        row.project,
        row.agent,
        row.session_id,
        row.message.unwrap_or(""),
    ];
    haystacks.iter().any(|h| h.to_lowercase().contains(&needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row<'a>(project: &'a str, agent: &'a str, message: Option<&'a str>) -> FilterRow<'a> {
        FilterRow {
            session_id: "sess-x",
            project,
            agent,
            message,
        }
    }

    #[test]
    fn empty_filter_matches_everything() {
        assert!(matches(row("alpha", "claude-code", None), ""));
        assert!(matches(row("beta", "opencode", Some("hi")), ""));
    }

    #[test]
    fn filter_matches_project_substring() {
        assert!(matches(row("alpha-project", "claude-code", None), "alpha"));
        assert!(matches(row("alpha-project", "claude-code", None), "PROJ"));
    }

    #[test]
    fn filter_matches_agent_substring() {
        assert!(matches(row("p", "pi-coding-agent", None), "pi"));
        assert!(matches(row("p", "pi-coding-agent", None), "CODING"));
    }

    #[test]
    fn filter_matches_message_substring() {
        assert!(matches(
            row("p", "claude-code", Some("Permission required")),
            "permission",
        ));
    }

    #[test]
    fn filter_matches_session_id_substring() {
        assert!(matches(
            FilterRow {
                session_id: "abc-123-def",
                project: "p",
                agent: "a",
                message: None,
            },
            "123",
        ));
    }

    #[test]
    fn filter_rejects_when_nothing_matches() {
        assert!(!matches(row("alpha", "claude-code", Some("hi")), "xyz"));
    }

    #[test]
    fn filter_handles_missing_message_as_empty() {
        // Missing message shouldn't match "" arms-length checks unless filter is empty.
        // We already test the empty-filter path; ensure a non-empty filter doesn't
        // false-match against the unwrap_or("") default.
        assert!(!matches(row("p", "a", None), "anything"));
    }
}
```

- [ ] **Step 3: Run the new tests to verify they pass**

Run:

```sh
cargo test -p agent-switcher
```

Expected: 7 tests pass. (We TDD-wrote the implementation alongside the tests in one shot here because the function is trivial; if you prefer strict red→green, comment out the `matches` body and run first.)

- [ ] **Step 4: Run clippy**

Run:

```sh
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Commit**

```sh
git add -A
git commit -m "feat(agent-switcher): pure filter logic with case-insensitive substring match"
```

---

## Task 11: Implement `app.rs` (state, tick, key handling)

**Files:**
- Create: `crates/agent-switcher/src/app.rs`
- Modify: `crates/agent-switcher/src/main.rs`

The `App` holds the snapshot of entries, the filter string, the selected index, and a tick counter for spinner animation. Most of its behavior is testable without a terminal.

- [ ] **Step 1: Declare `app` in `main.rs`**

Update `crates/agent-switcher/src/main.rs`:

```rust
//! Tmux popup TUI for switching between waiting AI coding agent sessions.

mod app;
mod filter;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("agent-switcher: not implemented yet");
    ExitCode::from(1)
}
```

- [ ] **Step 2: Write the failing tests in `app.rs`**

Create `crates/agent-switcher/src/app.rs`:

```rust
//! Switcher app state — entries, filter input, selection, spinner tick.
//!
//! The `tick` method reads `StateStore`, so it's exercised in integration
//! tests; everything else (key handling, filter, selection clamping) is pure.

use agent_status::{AttentionEntry, StateStore};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::filter::{matches, FilterRow};

/// Outcome of one key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Keep running.
    Continue,
    /// User pressed Esc or Ctrl-C — exit without switching.
    Cancel,
    /// User pressed Enter — switch to the selected session's pane, then exit.
    Activate,
}

pub struct App {
    store: StateStore,
    pub entries: Vec<(String, AttentionEntry)>,
    pub filter: String,
    pub selected: usize,
    pub tick: u64,
}

impl App {
    pub fn new(store: StateStore) -> Self {
        let entries = store.list().unwrap_or_default();
        Self {
            store,
            entries,
            filter: String::new(),
            selected: 0,
            tick: 0,
        }
    }

    /// Re-read the state directory and bump the spinner tick. Called from the
    /// event loop on each ~250ms timer.
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.entries = self.store.list().unwrap_or_default();
        self.clamp_selection();
    }

    /// Indices into `self.entries` that pass the filter, preserving order.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, (sid, e))| {
                matches(
                    FilterRow {
                        session_id: sid,
                        project: &e.project,
                        agent: &e.agent,
                        message: e.message.as_deref(),
                    },
                    &self.filter,
                )
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The entry at the current selected row in the filtered view, if any.
    pub fn selected_entry(&self) -> Option<&(String, AttentionEntry)> {
        let idx = *self.visible_indices().get(self.selected)?;
        self.entries.get(idx)
    }

    /// Reduce one key event into a state change. Pure-ish: the only side effect
    /// is mutating `self`.
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => KeyOutcome::Cancel,
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => KeyOutcome::Cancel,
            (KeyCode::Enter, _) => KeyOutcome::Activate,
            (KeyCode::Char('n'), KeyModifiers::CONTROL) | (KeyCode::Down, _) => {
                self.move_down();
                KeyOutcome::Continue
            }
            (KeyCode::Char('p'), KeyModifiers::CONTROL) | (KeyCode::Up, _) => {
                self.move_up();
                KeyOutcome::Continue
            }
            (KeyCode::Backspace, _) => {
                self.filter.pop();
                self.selected = 0;
                KeyOutcome::Continue
            }
            (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) => {
                self.filter.push(c);
                self.selected = 0;
                KeyOutcome::Continue
            }
            _ => KeyOutcome::Continue,
        }
    }

    fn move_down(&mut self) {
        let n = self.visible_indices().len();
        if n == 0 {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1) % n;
        }
    }

    fn move_up(&mut self) {
        let n = self.visible_indices().len();
        if n == 0 {
            self.selected = 0;
        } else {
            self.selected = if self.selected == 0 { n - 1 } else { self.selected - 1 };
        }
    }

    fn clamp_selection(&mut self) {
        let n = self.visible_indices().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_status::AttentionEntry;
    use tempfile::TempDir;

    fn sample(project: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: "%1".into(),
            ts: 1,
            message: None,
            pid: None,
        }
    }

    fn app_with_entries(entries: &[(&str, &str)]) -> App {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().to_path_buf());
        for (sid, project) in entries {
            store.write(sid, &sample(project, "notify")).unwrap();
        }
        // Leak the tempdir so the store keeps working for the test. The OS
        // cleans it up at process exit anyway.
        let _ = Box::leak(Box::new(dir));
        App::new(store)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn empty_app_has_no_visible_entries() {
        let app = app_with_entries(&[]);
        assert_eq!(app.visible_indices().len(), 0);
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn ctrl_n_wraps_to_the_top_at_the_bottom() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta"), ("s3", "gamma")]);
        assert_eq!(app.selected, 0);
        assert_eq!(app.handle_key(ctrl('n')), KeyOutcome::Continue);
        assert_eq!(app.selected, 1);
        assert_eq!(app.handle_key(ctrl('n')), KeyOutcome::Continue);
        assert_eq!(app.selected, 2);
        assert_eq!(app.handle_key(ctrl('n')), KeyOutcome::Continue);
        assert_eq!(app.selected, 0, "should wrap");
    }

    #[test]
    fn ctrl_p_wraps_to_the_bottom_at_the_top() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta"), ("s3", "gamma")]);
        assert_eq!(app.handle_key(ctrl('p')), KeyOutcome::Continue);
        assert_eq!(app.selected, 2, "should wrap to bottom");
    }

    #[test]
    fn typing_chars_appends_to_filter_and_resets_selection() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta"), ("s3", "gamma")]);
        app.selected = 2;
        app.handle_key(key(KeyCode::Char('b')));
        assert_eq!(app.filter, "b");
        assert_eq!(app.selected, 0);
        // Only one entry matches "b" → beta.
        let visible = app.visible_indices();
        assert_eq!(visible.len(), 1);
        let (sid, _) = &app.entries[visible[0]];
        assert_eq!(sid, "s2");
    }

    #[test]
    fn backspace_pops_one_filter_char() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        app.filter.push_str("xyz");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.filter, "xy");
    }

    #[test]
    fn esc_returns_cancel() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), KeyOutcome::Cancel);
    }

    #[test]
    fn ctrl_c_returns_cancel() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        assert_eq!(app.handle_key(ctrl('c')), KeyOutcome::Cancel);
    }

    #[test]
    fn enter_returns_activate() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), KeyOutcome::Activate);
    }

    #[test]
    fn selection_clamps_when_filter_shrinks_visible_set() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta")]);
        app.selected = 1;
        // Type "alp" — only "alpha" matches, so selected should reset.
        for c in "alp".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn tick_increments_and_does_not_panic_on_empty_store() {
        let mut app = app_with_entries(&[]);
        let before = app.tick;
        app.tick();
        assert_eq!(app.tick, before.wrapping_add(1));
    }
}
```

- [ ] **Step 3: Run the new tests**

Run:

```sh
cargo test -p agent-switcher
```

Expected: 7 filter tests + 10 app tests = 17 tests pass. If you see a `cannot find module 'app'` error, you forgot the `mod app;` declaration in `main.rs`.

- [ ] **Step 4: Run clippy**

Run:

```sh
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean. The `Box::leak` for `TempDir` may trip a pedantic lint — if so, add `#[allow(clippy::…)]` directly above the leak with a one-line `// reason: …` comment.

- [ ] **Step 5: Commit**

```sh
git add -A
git commit -m "feat(agent-switcher): app state, key handling, selection wraparound"
```

---

## Task 12: Implement `ui.rs` (ratatui rendering)

**Files:**
- Create: `crates/agent-switcher/src/ui.rs`
- Modify: `crates/agent-switcher/src/main.rs`

The UI is a pure function of `&App`. Animation is controlled by `app.tick`.

- [ ] **Step 1: Declare `ui` in `main.rs`**

Update `crates/agent-switcher/src/main.rs`:

```rust
//! Tmux popup TUI for switching between waiting AI coding agent sessions.

mod app;
mod filter;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("agent-switcher: not implemented yet");
    ExitCode::from(1)
}
```

- [ ] **Step 2: Write `ui.rs`**

Create `crates/agent-switcher/src/ui.rs`:

```rust
//! Ratatui rendering. Pure function of `&App`; called from the event loop.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MESSAGE_CAP: usize = 80;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    f.render_widget(filter_widget(&app.filter), chunks[0]);
    f.render_widget(sessions_table(app), chunks[1]);
    f.render_widget(help_widget(), chunks[2]);
}

fn filter_widget(filter: &str) -> Paragraph<'_> {
    Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(filter),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Filter"))
}

fn sessions_table<'a>(app: &'a App) -> Table<'a> {
    let visible = app.visible_indices();
    let spinner = SPINNER_FRAMES[(app.tick as usize) % SPINNER_FRAMES.len()];

    let rows: Vec<Row<'a>> = visible
        .iter()
        .enumerate()
        .map(|(view_idx, &entries_idx)| {
            let (sid, e) = &app.entries[entries_idx];
            let (status_text, status_color) = match e.event.as_str() {
                "working" => (spinner.to_string(), Color::Cyan),
                "notify" => ("!".to_string(), Color::Yellow),
                "done" => ("✓".to_string(), Color::Green),
                other => (other.chars().next().unwrap_or('?').to_string(), Color::Gray),
            };
            let session = display_session(sid, &e.project);
            let snippet = e
                .message
                .as_deref()
                .map(one_line)
                .unwrap_or_default();
            let mut row = Row::new(vec![
                Cell::from(status_text).style(Style::default().fg(status_color)),
                Cell::from(session),
                Cell::from(e.agent.clone()),
                Cell::from(snippet),
            ]);
            if view_idx == app.selected {
                row = row.style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            }
            row
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(28),
        Constraint::Length(16),
        Constraint::Min(0),
    ];

    Table::new(rows, widths)
        .header(
            Row::new(vec!["", "Session", "Agent", "Last response"])
                .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().borders(Borders::ALL).title("Sessions"))
}

fn help_widget() -> Paragraph<'static> {
    Paragraph::new(
        "Ctrl-N/P or ↓/↑: navigate · Enter: switch pane · Esc / Ctrl-C: cancel",
    )
    .style(Style::default().fg(Color::DarkGray))
}

fn display_session(session_id: &str, project: &str) -> String {
    // Project name is the primary handle; session_id is the disambiguator on
    // the rare event that two sessions share a project.
    let short_sid: String = session_id.chars().take(8).collect();
    format!("{project} ({short_sid})")
}

fn one_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out.chars().take(MESSAGE_CAP).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_collapses_newlines_and_tabs_to_single_spaces() {
        assert_eq!(one_line("a\nb\n\nc"), "a b c");
        assert_eq!(one_line("a\tb\rc"), "a b c");
    }

    #[test]
    fn one_line_caps_long_input() {
        let long = "x".repeat(500);
        let result = one_line(&long);
        assert!(result.chars().count() <= MESSAGE_CAP);
    }

    #[test]
    fn display_session_truncates_session_id_to_eight_chars() {
        let out = display_session("abcdef-1234-very-long-session-id", "alpha");
        assert_eq!(out, "alpha (abcdef-1)");
    }
}
```

- [ ] **Step 3: Run the new tests**

Run:

```sh
cargo test -p agent-switcher
```

Expected: 7 filter + 10 app + 3 ui = 20 tests pass.

- [ ] **Step 4: Run clippy**

Run:

```sh
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean. Pedantic may flag `as usize` casts in `(app.tick as usize)`; if so, switch to `usize::try_from(app.tick).unwrap_or(0)` or `#[allow(clippy::cast_possible_truncation)]` with a one-line reason.

- [ ] **Step 5: Commit**

```sh
git add -A
git commit -m "feat(agent-switcher): ratatui rendering — filter, table, spinner, help"
```

---

## Task 13: Wire up the event loop in `main.rs`

**Files:**
- Modify: `crates/agent-switcher/src/main.rs`

This is where the terminal goes into raw + alternate-screen mode and the event loop runs.

- [ ] **Step 1: Rewrite `main.rs`**

Replace `crates/agent-switcher/src/main.rs` with:

```rust
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
```

- [ ] **Step 2: Build the binary**

Run:

```sh
cargo build -p agent-switcher
```

Expected: clean build. If crossterm complains about a `Backend` trait import, the explicit `use ratatui::backend::Backend;` may be needed in the `event_loop` generic — adjust the bound to `<B: ratatui::backend::Backend>` (already written that way above).

- [ ] **Step 3: Run the full test suite**

Run:

```sh
cargo test
```

Expected: all tests (across both crates) pass. The terminal-setup code itself isn't unit-tested (it can't be — it needs a real tty), but `App`/`ui`/`filter` are.

- [ ] **Step 4: Smoke-test the binary manually**

Run:

```sh
# Seed a few entries in the dev state dir.
target/debug/agent-status set notify   <<<'{"session_id":"sess-a","message":"Permission required to read /etc/passwd"}'
target/debug/agent-status set working  <<<'{"session_id":"sess-b"}'
target/debug/agent-status set done     <<<'{"session_id":"sess-c","message":"All checks passed"}'

target/debug/agent-switcher
```

Expected: a TUI opens showing three rows (one with a spinner, one with `!`, one with `✓`). Type `permission` to filter; press Ctrl-N/P to move the selection; press Esc to exit. If running inside tmux, press Enter on a row whose pane matches a real pane to jump to it; outside tmux, Enter just exits (tmux call fails silently).

Clean up:

```sh
target/debug/agent-status clear <<<'{"session_id":"sess-a"}'
target/debug/agent-status clear <<<'{"session_id":"sess-b"}'
target/debug/agent-status clear <<<'{"session_id":"sess-c"}'
```

- [ ] **Step 5: Run clippy**

Run:

```sh
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 6: Commit**

```sh
git add -A
git commit -m "feat(agent-switcher): event loop, terminal setup, tmux switch-client on Enter"
```

---

## Task 14: Catch panics so the terminal is restored before the process exits

**Files:**
- Modify: `crates/agent-switcher/src/main.rs`

A panic inside `event_loop` (e.g. ratatui draw error) currently leaves the terminal in raw mode because the cleanup block is bypassed by the unwind. Wrap the loop in `std::panic::catch_unwind` *or* register a panic hook that restores the terminal before the default hook runs. The latter is simpler.

- [ ] **Step 1: Write the failing test**

This behavior can't be unit-tested directly — it depends on the terminal state. Instead, write a smoke test that simulates a panic via a feature-flagged debug helper.

Skip the test for this task and rely on manual verification: the criterion is "after a forced panic, your shell prompt is still usable".

- [ ] **Step 2: Install a panic hook in `run()`**

In `crates/agent-switcher/src/main.rs`, just before `enable_raw_mode()?;`, insert:

```rust
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        default_hook(info);
    }));
```

This hook fires before the default unwinder prints the panic message, so the user sees the panic on a sane terminal.

- [ ] **Step 3: Manually verify the hook by forcing a panic**

Temporarily add `panic!("test")` to the top of `event_loop` and run:

```sh
cargo build -p agent-switcher
target/debug/agent-switcher
```

Expected: the terminal opens briefly, panic prints to a clean terminal (not raw-mode garbled), and your shell prompt is usable afterwards. **Remove the temporary `panic!` before committing.**

- [ ] **Step 4: Run the full suite + clippy**

```sh
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Commit**

```sh
git add -A
git commit -m "feat(agent-switcher): restore terminal in a panic hook"
```

---

## Task 15: Update README.md — install both binaries, replace the fzf popup snippet

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the "Install" section**

Replace the existing `Install` block in `README.md` with:

```markdown
## Install

```sh
cargo build --release
mkdir -p ~/.claude/bin
install -m 0755 target/release/agent-status   ~/.claude/bin/agent-status
install -m 0755 target/release/agent-switcher ~/.claude/bin/agent-switcher
```

`~/.claude/bin` is one option; any directory works as long as the absolute path matches what you put in the hook commands and tmux config below. Both binaries are around 500 KB combined and have no runtime dependencies (tmux is invoked best-effort to refresh the status bar and switch panes; if it isn't running, the failure is silenced).
```

- [ ] **Step 2: Replace the optional fzf popup section with the agent-switcher snippet**

Find the section currently containing `bind-key C-a display-popup -E -w 80% -h 50% \` followed by the fzf pipeline. Replace it with:

```markdown
Optional popup picker (prefix + `C-a`) for jumping to the waiting pane — uses the bundled `agent-switcher` TUI:

```tmux
bind-key C-a display-popup -E -w 80% -h 50% "$HOME/.claude/bin/agent-switcher"
```

`agent-switcher` opens a small ratatui TUI: filter input at the top, the list of sessions in the middle, a help strip at the bottom. Type to filter (case-insensitive, matches across project / agent / message / session id); <kbd>Ctrl-N</kbd> and <kbd>Ctrl-P</kbd> (or arrow keys) move the selection; <kbd>Enter</kbd> runs `tmux switch-client` to the selected session's pane and exits; <kbd>Esc</kbd> or <kbd>Ctrl-C</kbd> exits without switching.

The list shows every recorded session — including those still working (animated spinner) — not just sessions waiting on your attention. That makes the popup useful as a general session jumper, while the status-bar indicator stays focused on "needs you now" sessions.
```

- [ ] **Step 3: Add a brief note in the Caveats section**

Append to the existing "Caveats" bullet list:

```markdown
- **Only Claude Code records a `working` state** today. pi and opencode sessions appear in the switcher when they're waiting (`done` / `notify`) but not while they're mid-turn. The hook semantics for those agents can be extended in a follow-up.
```

- [ ] **Step 4: Commit**

```sh
git add -A
git commit -m "docs(readme): replace fzf popup with agent-switcher; install both binaries"
```

---

## Task 16: Verify the installed-binary path still works end-to-end

**Files:**
- None (this is a manual verification + commit-only task)

The Claude Code hook generator embeds `bin_path` into the generated settings file. After this restructure, `agent-status agent-extension` (running from the workspace target dir or `~/.claude/bin`) still emits an absolute path that resolves correctly. Verify.

- [ ] **Step 1: Build and install**

```sh
cargo build --release
install -m 0755 target/release/agent-status   ~/.claude/bin/agent-status
install -m 0755 target/release/agent-switcher ~/.claude/bin/agent-switcher
```

- [ ] **Step 2: Regenerate the settings file via the alias**

```sh
~/.claude/bin/agent-status agent-extension
cat "$(~/.claude/bin/agent-status agent-extension)"
```

Expected: the printed JSON contains six hook entries; `UserPromptSubmit` and `PreToolUse` reference `set --agent claude-code working`; `Notification` / `Stop` reference `set ... notify` / `set ... done`; `SessionStart` / `SessionEnd` reference `clear`. All paths point at `~/.claude/bin/agent-status` (or whatever absolute path you installed to).

- [ ] **Step 3: Confirm the tmux popup binding works**

In tmux, with your `~/.tmux.conf` updated per Task 15:

```sh
tmux source-file ~/.tmux.conf
```

Bind-test: press prefix + `C-a`. The switcher should open in a centered popup. With at least one waiting session, you can navigate and Enter-switch.

- [ ] **Step 4: Final commit**

If you discover any drift between README instructions and reality during this verification, fix it inline and commit:

```sh
git add -A
git commit -m "docs(readme): align install + tmux instructions with verified flow"
```

(If nothing changed, no commit is needed; this task is acceptance, not implementation.)

---

## Self-review

**Spec coverage:**

- [x] Drop fzf switcher → Task 15 replaces the tmux binding.
- [x] Drop `preview` subcommand (per user follow-up); `list` stays → Task 7.
- [x] Replace with ratatui TUI → Tasks 10–13 implement the TUI.
- [x] Separate binary `agent-switcher` → Task 9 scaffolds it.
- [x] Monorepo split into `crates/agent-status` and `crates/agent-switcher` → Tasks 1–3.
- [x] Text input filter → Tasks 10–11 (filter logic + char handling in `App`).
- [x] <kbd>Ctrl-N</kbd>/<kbd>Ctrl-P</kbd> navigation → Task 11 `App::handle_key`.
- [x] Show agent/session name, status, agent type, last response → Task 12 `sessions_table`.
- [x] Spinner for working → Tasks 4–5 add the `working` status; Task 12 renders the spinner.

**Type consistency:** `AttentionEntry`, `StateStore`, `Agent`, `ExtensionFile` are re-exported from `lib.rs` (Task 3) and consumed unqualified by the switcher (Tasks 11, 13). `KeyOutcome` is the only new public enum on the switcher side and is consistently named.

**Placeholders / TODOs:** Task 14 Step 1 deliberately calls out that there's no automated test for the panic hook — verified manually. All other steps include actual code or commands.

**Risks:** ratatui 0.29 / crossterm 0.28 API shape is fixed at the time of writing; if cargo resolves to a newer minor version with breaking changes, the engineer may need to adjust the `Frame` import path or the `event::poll` signature. The `Box::leak(Box::new(dir))` in `app.rs` tests is a deliberate compromise to keep the test setup synchronous — replace with an `Arc<TempDir>` field on `App` if pedantic clippy is unhappy or memory hygiene matters.
