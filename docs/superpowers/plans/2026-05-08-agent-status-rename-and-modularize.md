# Agent-Status Rename + Modular Agent Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename `claude-status` to `agent-status` and refactor the existing Claude-Code-only implementation into an `Agent` trait + per-agent module structure so pluggable support for OpenAI Codex CLI, Cursor CLI, and OpenCode can be added in subsequent plans without further architectural churn.

**Architecture:**
- Introduce `pub trait Agent` (in `src/agents/mod.rs`) with `name()` and `extract_session_id()` methods.
- First (and only, in this plan) implementation: `ClaudeCodeAgent` in `src/agents/claude_code.rs`, replicating today's `commands::extract_session_id` logic against the `session_id` field.
- Registry function `agents::by_name(&str) -> Option<Box<dyn Agent>>` in `src/agents/mod.rs`.
- CLI gains `--agent <name>` flag on `set` and `clear` subcommands (default `claude-code` → keeps current hook configs working).
- `AttentionEntry` gains `agent: String` so `status`/`list` and any future tooling can attribute each waiting session to its agent of origin.
- Cargo package + binary renamed `claude-status` → `agent-status`. State directory renamed `${XDG_RUNTIME_DIR}/claude-status/` → `${XDG_RUNTIME_DIR}/agent-status/`.

**Tech Stack:** Rust 2021, `clap` 4 (derive), `serde`/`serde_json`. No new dependencies.

**Out of scope (future plans):** the actual Codex / Cursor / OpenCode agent impls. This plan only sets the table.

---

## File Structure (target after this plan)

```
src/
├── main.rs               # clap CLI: --agent on set/clear; routes through agents::by_name
├── state.rs              # AttentionEntry (with `agent` field), StateStore (dir → agent-status)
├── commands.rs           # build_entry (now takes agent), format_status, format_list
└── agents/
    ├── mod.rs            # pub trait Agent; by_name registry
    └── claude_code.rs    # struct ClaudeCodeAgent; impl Agent
tests/
└── cli.rs                # integration tests, exercising --agent flag explicitly + default
```

**What moves:**
- `commands::extract_session_id` (free fn) → `Agent::extract_session_id` (trait method on each agent impl). Its unit tests move from `commands.rs` to `src/agents/claude_code.rs`.
- `commands::build_entry` gains an `agent: &str` parameter; the new value lands in `AttentionEntry::agent`.
- `main.rs::run_set` / `run_clear` resolve `--agent <name>` via `agents::by_name(...)`, error on unknown names, then call the resolved agent's `extract_session_id`.

**What does NOT change:**
- Wire format compat with the bash precursor: the original 5 fields (`project`, `cwd`, `event`, `tmux_pane`, `ts`) keep their names and types. New `agent` field is a strict addition.
- Surface for `status` and `list` (agent-neutral — no `--agent` flag).
- The fzf popup-picker → `tmux switch-client` flow.

---

## Task 1: Add `Agent` trait and `ClaudeCodeAgent` impl

**Files:**
- Create: `src/agents/mod.rs`
- Create: `src/agents/claude_code.rs`
- Modify: `src/main.rs` (add `mod agents;` declaration)

- [ ] **Step 1: Scaffold the agents module and the trait stub**

Write `src/agents/mod.rs`:

```rust
pub mod claude_code;

/// An agent implementation: knows how to extract a session ID from the JSON payload that
/// agent's hook delivers on stdin.
pub trait Agent {
    /// Stable, lowercase, hyphenated identifier (e.g. `"claude-code"`). Used for the
    /// `--agent` CLI flag and the `agent` field on persisted entries.
    fn name(&self) -> &'static str;

    /// Extract the session ID from the agent's hook event JSON. Returns `None` for
    /// invalid JSON, missing field, non-string value, or empty string.
    fn extract_session_id(&self, stdin_json: &str) -> Option<String>;
}
```

Write `src/agents/claude_code.rs`:

```rust
use crate::agents::Agent;

/// Claude Code (`claude.ai/code`).
///
/// Reads `session_id` from the hook event payload that Claude Code pipes to stdin.
pub struct ClaudeCodeAgent;

impl Agent for ClaudeCodeAgent {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn extract_session_id(&self, stdin_json: &str) -> Option<String> {
        todo!()
    }
}
```

Modify `src/main.rs` — add `mod agents;` next to the other module declarations:

```rust
mod agents;
mod commands;
mod state;
```

- [ ] **Step 2: Write failing tests for `ClaudeCodeAgent`**

Append to `src/agents/claude_code.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_claude_code() {
        assert_eq!(ClaudeCodeAgent.name(), "claude-code");
    }

    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(
            ClaudeCodeAgent.extract_session_id(json).as_deref(),
            Some("abc-123")
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_missing_field() {
        assert_eq!(
            ClaudeCodeAgent.extract_session_id(r#"{"other":1}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(
            ClaudeCodeAgent.extract_session_id(r#"{"session_id":""}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(ClaudeCodeAgent.extract_session_id("not json"), None);
    }
}
```

- [ ] **Step 3: Run tests, verify they fail with `not yet implemented`**

Run: `cargo test agents::claude_code`
Expected: 5 tests; 4 panic with `not yet implemented` (the `name()` test passes immediately because the implementation returned `"claude-code"` directly in step 1 — re-verify; if it fails, that's also fine, we're TDD'ing). Capture the output.

- [ ] **Step 4: Replace the `todo!()` with the real implementation**

In `src/agents/claude_code.rs`, change `extract_session_id` to:

```rust
    fn extract_session_id(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let id = v.get("session_id")?.as_str()?;
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }
```

- [ ] **Step 5: Run tests, verify all pass**

Run: `cargo test agents::claude_code`
Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Step 6: Commit**

```bash
git add src/agents/ src/main.rs
git commit -m "feat(agents): introduce Agent trait and ClaudeCodeAgent impl"
```

---

## Task 2: Add `agents::by_name` registry

**Files:**
- Modify: `src/agents/mod.rs`

- [ ] **Step 1: Append failing tests to `src/agents/mod.rs`**

Add at the bottom of `src/agents/mod.rs`:

```rust
/// Resolve an agent by its `--agent` flag value.
pub fn by_name(name: &str) -> Option<Box<dyn Agent>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_name_resolves_claude_code() {
        let agent = by_name("claude-code").expect("claude-code is a registered agent");
        assert_eq!(agent.name(), "claude-code");
    }

    #[test]
    fn by_name_returns_none_for_unknown() {
        assert!(by_name("frobnicator").is_none());
    }

    #[test]
    fn by_name_is_case_sensitive() {
        assert!(by_name("Claude-Code").is_none());
    }
}
```

- [ ] **Step 2: Run tests, verify they fail with `not yet implemented`**

Run: `cargo test agents::tests`
Expected: 3 panics from `not yet implemented`.

- [ ] **Step 3: Implement `by_name`**

Replace the `todo!()` body:

```rust
pub fn by_name(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        "claude-code" => Some(Box::new(claude_code::ClaudeCodeAgent)),
        _ => None,
    }
}
```

- [ ] **Step 4: Run tests, verify all pass**

Run: `cargo test agents::tests`
Expected: `test result: ok. 3 passed; 0 failed`.

Then run the whole suite to confirm nothing else broke:
Run: `cargo test`
Expected: still all green (no regressions).

- [ ] **Step 5: Commit**

```bash
git add src/agents/mod.rs
git commit -m "feat(agents): add by_name registry"
```

---

## Task 3: Add `agent` field to `AttentionEntry`

**Files:**
- Modify: `src/state.rs`

Adding the field forces every existing test that constructs an `AttentionEntry` to include the new field. Walk through each test deliberately — don't pattern-search-and-replace blindly.

- [ ] **Step 1: Add the field to the struct definition**

In `src/state.rs`, change:

```rust
pub struct AttentionEntry {
    pub project: String,
    pub cwd: String,
    pub event: String,
    pub tmux_pane: String,
    pub ts: u64,
}
```

to:

```rust
pub struct AttentionEntry {
    /// Stable identifier of the agent that wrote this entry (e.g. `"claude-code"`).
    pub agent: String,
    /// Basename of the project directory (typically the cwd's last component).
    pub project: String,
    /// Absolute path of the project directory at the time the hook fired.
    pub cwd: String,
    /// Hook event label, for example `notify` or `done`.
    pub event: String,
    /// Tmux pane id (such as `%17`), or empty if the hook fired outside tmux.
    pub tmux_pane: String,
    /// Unix timestamp (seconds) when the entry was written.
    pub ts: u64,
}
```

(Move the existing doc comment for `project` down — `agent` is now the new lead doc'd field.)

- [ ] **Step 2: Run tests; expect compile errors at every `AttentionEntry { ... }` site**

Run: `cargo test`
Expected: compilation errors of the form `missing field `agent` in initializer of `AttentionEntry``. There are several call sites in `src/state.rs` (the `entry_roundtrips_through_json` and `entry_matches_bash_plan_field_names` tests, and the `sample_entry` helper) and in `src/commands.rs` (the `entry` test helper).

- [ ] **Step 3: Update `src/state.rs` test sites**

Update the `sample_entry` helper:

```rust
    fn sample_entry(project: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: "notify".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        }
    }
```

Update `entry_roundtrips_through_json`:

```rust
    #[test]
    fn entry_roundtrips_through_json() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "claude-status".into(),
            cwd: "/Users/x/work/claude-status".into(),
            event: "notify".into(),
            tmux_pane: "%42".into(),
            ts: 1_700_000_000,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: AttentionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
    }
```

Update `entry_matches_bash_plan_field_names` to verify the original 5 fields ARE present AND the new `agent` field is also present:

```rust
    #[test]
    fn entry_matches_bash_plan_field_names() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "p".into(),
            cwd: "/c".into(),
            event: "done".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        };
        let v: serde_json::Value = serde_json::to_value(&entry).unwrap();
        // Original fields from the bash precursor — must not be renamed/removed.
        assert!(v.get("project").is_some());
        assert!(v.get("cwd").is_some());
        assert!(v.get("event").is_some());
        assert!(v.get("tmux_pane").is_some());
        assert!(v.get("ts").is_some());
        // New attribution field added when this CLI grew multi-agent support.
        assert!(v.get("agent").is_some());
    }
```

- [ ] **Step 4: Update `src/commands.rs` test sites**

Update the `entry` helper:

```rust
    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: pane.into(),
            ts: 1,
        }
    }
```

(`build_entry` itself will be updated in Task 4.)

- [ ] **Step 5: Run tests, verify they all pass**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/state.rs src/commands.rs
git commit -m "feat(state): add agent field to AttentionEntry"
```

---

## Task 4: Update `build_entry` to take an `agent` parameter

**Files:**
- Modify: `src/commands.rs`

- [ ] **Step 1: Update tests to expect the new signature**

In `src/commands.rs`, update `build_entry_uses_basename_of_cwd_as_project`:

```rust
    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("claude-code", "notify", "/Users/me/work/claude-status", "%5", 42);
        assert_eq!(e.agent, "claude-code");
        assert_eq!(e.project, "claude-status");
        assert_eq!(e.cwd, "/Users/me/work/claude-status");
        assert_eq!(e.event, "notify");
        assert_eq!(e.tmux_pane, "%5");
        assert_eq!(e.ts, 42);
    }
```

Update `build_entry_falls_back_to_cwd_when_no_basename`:

```rust
    #[test]
    fn build_entry_falls_back_to_cwd_when_no_basename() {
        let e = build_entry("claude-code", "notify", "/", "", 0);
        assert_eq!(e.project, "/");
        assert_eq!(e.agent, "claude-code");
    }
```

- [ ] **Step 2: Run tests, expect compile errors**

Run: `cargo test`
Expected: errors of the form `expected 4 arguments, got 5`.

- [ ] **Step 3: Update `build_entry` signature and body**

Change the `build_entry` definition in `src/commands.rs` to:

```rust
/// Construct an [`AttentionEntry`] from raw inputs.
///
/// `project` is derived as the basename of `cwd`. When `cwd` has no basename (e.g. `/`
/// or empty string), `project` falls back to `cwd` itself.
pub fn build_entry(
    agent: &str,
    event: &str,
    cwd: &str,
    tmux_pane: &str,
    ts: u64,
) -> AttentionEntry {
    let project = Path::new(cwd)
        .file_name()
        .map_or_else(|| cwd.to_string(), |s| s.to_string_lossy().into_owned());
    AttentionEntry {
        agent: agent.to_string(),
        project,
        cwd: cwd.to_string(),
        event: event.to_string(),
        tmux_pane: tmux_pane.to_string(),
        ts,
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test`
Expected: all green. NOTE: `main.rs::run_set` still calls `build_entry(event, &cwd, &pane, ts)` — this will be a compile error. Fix it temporarily by passing `"claude-code"` as the first arg until Task 5 wires it through properly:

In `src/main.rs::run_set`, change:

```rust
    let entry = build_entry(event, &cwd, &pane, ts);
```

to:

```rust
    let entry = build_entry("claude-code", event, &cwd, &pane, ts);
```

(This hard-coded value goes away in Task 5.)

Re-run: `cargo test`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add src/commands.rs src/main.rs
git commit -m "feat(commands): build_entry takes agent name as first arg"
```

---

## Task 5: Wire CLI `--agent` flag and remove the free `extract_session_id`

**Files:**
- Modify: `src/main.rs`
- Modify: `src/commands.rs`

This task ties the trait, the registry, and the CLI together; afterwards the duplicate `extract_session_id` in `commands.rs` is dead and removed.

- [ ] **Step 1: Add `--agent` to `Cmd::Set` and `Cmd::Clear`, plumb through `run_set` / `run_clear`**

In `src/main.rs`, change:

```rust
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
```

to:

```rust
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
```

Update the dispatch in `main`:

```rust
    let result = match cli.command {
        Cmd::Set { event, agent } => run_set(&store, &agent, &event),
        Cmd::Clear { agent } => run_clear(&store, &agent),
        Cmd::Status => run_status(&store, &mut io::stdout().lock()),
        Cmd::List => run_list(&store, &mut io::stdout().lock()),
    };
```

Update `run_set`:

```rust
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

    let entry = build_entry(agent.name(), event, &cwd, &pane, ts);
    store.write(&session_id, &entry)?;
    refresh_tmux();
    Ok(())
}
```

Update `run_clear`:

```rust
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
```

Remove the now-unused import line at the top of `main.rs`:

```rust
use commands::{build_entry, extract_session_id, format_list, format_status};
```

becomes:

```rust
use commands::{build_entry, format_list, format_status};
```

- [ ] **Step 2: Remove the free `extract_session_id` from `commands.rs`**

Delete the entire `extract_session_id` function from `src/commands.rs` (lines starting `pub fn extract_session_id` through its closing `}`). Also delete its four unit tests inside `mod tests` (they have already been ported to `src/agents/claude_code.rs` in Task 1):

- `extract_session_id_returns_id`
- `extract_session_id_returns_none_for_missing`
- `extract_session_id_returns_none_for_empty_string`
- `extract_session_id_returns_none_for_invalid_json`

The `commands.rs` file's public surface is now just `build_entry`, `format_status`, `format_list`.

- [ ] **Step 3: Build, test, and clippy-check**

Run: `cargo build && cargo test && cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: clean build, all tests pass, zero clippy warnings.

- [ ] **Step 4: Smoke-test the new `--agent` flag manually**

```bash
TMP=$(mktemp -d)

echo "1: empty status"
XDG_RUNTIME_DIR="$TMP" cargo run --quiet -- status

echo "2: set with default agent (no flag)"
echo '{"session_id":"sess-A"}' | XDG_RUNTIME_DIR="$TMP" cargo run --quiet -- set notify
XDG_RUNTIME_DIR="$TMP" cargo run --quiet -- status
ls "$TMP/claude-status/"

echo "3: clear with explicit agent"
echo '{"session_id":"sess-A"}' | XDG_RUNTIME_DIR="$TMP" cargo run --quiet -- clear --agent claude-code
XDG_RUNTIME_DIR="$TMP" cargo run --quiet -- status

echo "4: unknown agent rejected"
echo '{"session_id":"x"}' | XDG_RUNTIME_DIR="$TMP" cargo run --quiet -- set --agent nopealope notify
echo "exit=$?"

rm -rf "$TMP"
```

Expected:
1. Empty.
2. After set: `[!] <basename>` line; state file `sess-A` exists.
3. After clear: empty.
4. Exit code 1, stderr contains `unknown agent: nopealope`.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs src/commands.rs
git commit -m "feat(cli): wire --agent flag through agent registry"
```

---

## Task 6: Rename state directory suffix `claude-status` → `agent-status`

**Files:**
- Modify: `src/state.rs`

- [ ] **Step 1: Update `StateStore::from_env`**

In `src/state.rs`, change:

```rust
        Self::new(base.join("claude-status"))
```

to:

```rust
        Self::new(base.join("agent-status"))
```

- [ ] **Step 2: Update the `from_env_path_ends_with_claude_status` test**

Rename the test and update its assertion:

```rust
    #[test]
    fn from_env_path_ends_with_agent_status() {
        let store = StateStore::from_env();
        assert!(store.dir().ends_with("agent-status"));
    }
```

- [ ] **Step 3: Update integration test references**

In `tests/cli.rs`, replace all three occurrences of:

```rust
    let state_dir = tmp.path().join("claude-status");
```

with:

```rust
    let state_dir = tmp.path().join("agent-status");
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs tests/cli.rs
git commit -m "refactor(state): rename runtime dir suffix to agent-status"
```

---

## Task 7: Rename Cargo package, binary, and CLI prefix to `agent-status`

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`
- Modify: `tests/cli.rs`

- [ ] **Step 1: Update `Cargo.toml`**

Change:

```toml
[package]
name = "claude-status"
version = "0.1.0"
edition = "2021"
description = "Tmux-integrated indicator showing which Claude Code sessions are waiting on user input."

[[bin]]
name = "claude-status"
path = "src/main.rs"
```

to:

```toml
[package]
name = "agent-status"
version = "0.2.0"
edition = "2021"
description = "Tmux-integrated indicator showing which AI coding agent sessions are waiting on user input."

[[bin]]
name = "agent-status"
path = "src/main.rs"
```

(Bumping minor version to `0.2.0` because the schema and CLI surface both change.)

- [ ] **Step 2: Update `eprintln!` prefixes in `src/main.rs`**

Replace all three `claude-status:` literal strings (in the unknown-subcommand branch, missing-subcommand branch, and final error branch) with `agent-status:`. Also update the doc comment on the `Cli` struct:

```rust
/// Tmux-integrated indicator showing which AI coding agent sessions are waiting on user input.
///
/// Each agent's hooks invoke `set`/`clear` with `--agent <name>`; `status` and `list` are
/// agent-neutral and aggregate state from every agent. Currently registered: `claude-code`.
```

(`Notification`, `Stop`, `UserPromptSubmit`, `SessionStart`, `SessionEnd` references in the doc comment are Claude-Code-specific — keep them but prefix with "Claude Code's hooks: ..." so the doc reads correctly when other agents come online.)

- [ ] **Step 3: Update integration test `CARGO_BIN_EXE` reference**

In `tests/cli.rs`:

```rust
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_agent-status")
}
```

- [ ] **Step 4: Build, test**

Run: `cargo build && cargo test`
Expected: clean build, all green.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs tests/cli.rs
git commit -m "refactor: rename crate and binary to agent-status (v0.2.0)"
```

---

## Task 8: Update README and CLAUDE.md

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update `README.md`**

Replace the title and opening line:

```markdown
# agent-status

A small Rust CLI that shows in tmux's `status-right` which AI coding agent sessions are waiting on user input. Currently supports Claude Code; the architecture is set up to plug in additional agents (Codex CLI, Cursor CLI, OpenCode) without restructuring.
```

Replace every other `claude-status` occurrence with `agent-status` throughout (binary name, install path, command examples, state path).

Update the **Configure → Claude Code hooks** section to use the `--agent claude-code` flag:

```json
{
  "hooks": {
    "Notification":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status set --agent claude-code notify" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status set --agent claude-code done"   }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/agent-status clear --agent claude-code"      }] }]
  }
}
```

(The `--agent` flag is technically optional because of the default, but listing it explicitly documents the integration point for readers who later add another agent.)

Update the **Usage** section's subcommand list:

```sh
agent-status --help                       # top-level help
agent-status set [EVENT] [--agent NAME]   # mark this session as waiting (reads JSON on stdin)
agent-status clear [--agent NAME]         # clear this session's state (reads JSON on stdin)
agent-status status                       # print the status-right line, empty if nothing waiting
agent-status list                         # print TSV (pane, project, event) per waiting session
```

Update the **State location** sample JSON to include the `agent` field:

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565}
```

- [ ] **Step 2: Update `CLAUDE.md`**

Replace top-of-file references to `claude-status` with `agent-status`. Update the test count if it changed (run `cargo test 2>&1 | tail -3` to confirm the new total).

Add a section after "Module split (load-bearing)" called **"Adding a new agent"**:

```markdown
## Adding a new agent

Each AI coding agent we integrate with lives in its own file under `src/agents/`. To plug in a new one:

1. Create `src/agents/<agent>.rs` with a unit struct (e.g. `pub struct CodexCliAgent;`) implementing `agents::Agent` (in `src/agents/mod.rs`). Implement `name()` returning the lowercase hyphenated identifier (e.g. `"codex-cli"`) and `extract_session_id()` parsing whatever field the agent's hook payload uses for the session/conversation key.
2. Register the new agent in `agents::by_name` so the CLI's `--agent` flag can resolve it.
3. Add unit tests for `extract_session_id` covering the four standard cases (valid id, missing field, empty string, invalid JSON) plus any field-name-specific edge cases (e.g. Cursor's `conversation_id` vs `session_id` switch on `sessionStart`).
4. Document the agent's hook config in README.md alongside the existing Claude Code section.

No changes to `state.rs`, `commands.rs`, or `main.rs` should be needed for a typical new agent — that's the test of the abstraction.
```

Update the **Wire compatibility** section: the `agent` field is a recent addition that breaks bash-precursor compatibility for newly-written entries; old bash-version entries can still be read (serde will reject them on deserialize because `agent` is non-optional, so `list` skips them — equivalent to "stale state cleanup happens naturally on first `set`/`clear` of each session"). Note this explicitly.

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: update README and CLAUDE.md for agent-status rename"
```

---

## Task 9: Reinstall binary, update Claude Code + tmux configs, clean stale state

**Files:**
- (External) `~/.claude/bin/agent-status` (new), `~/.claude/bin/claude-status` (delete)
- (External) `~/.claude/settings.json`
- (External) `~/.tmux.conf` (symlinked to user's dotfiles repo)
- (External) `/tmp/claude-status/` (stale state)

This task touches user-shared files; back up and show diffs before applying.

- [ ] **Step 1: Build release binary**

Run: `cargo build --release`
Expected: produces `target/release/agent-status` (~500 KB).

- [ ] **Step 2: Install new binary, remove old**

```bash
install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
rm -f ~/.claude/bin/claude-status
ls -l ~/.claude/bin/
```
Expected: `agent-status` present and executable; `claude-status` gone.

- [ ] **Step 3: Verify the installed binary works**

```bash
~/.claude/bin/agent-status --version
~/.claude/bin/agent-status --help | head -10
```
Expected: version line printed; help text shows the `set`/`clear`/`status`/`list` subcommands.

- [ ] **Step 4: Back up and update `~/.claude/settings.json` hook commands**

```bash
cp ~/.claude/settings.json ~/.claude/settings.json.bak.$(date +%Y%m%d-%H%M%S)
NEW=$(jq '
  .hooks
  |= with_entries(
       .value
       |= map(
            .hooks
            |= map(
                 .command
                 |= sub("\\$HOME/.claude/bin/claude-status set ([a-z]+)$"; "$HOME/.claude/bin/agent-status set --agent claude-code \\1")
                 | sub("\\$HOME/.claude/bin/claude-status clear$"; "$HOME/.claude/bin/agent-status clear --agent claude-code")
               )
          )
     )
' ~/.claude/settings.json)
echo "$NEW" > ~/.claude/settings.json.new
diff -u ~/.claude/settings.json ~/.claude/settings.json.new
```

Verify the diff updates exactly the 5 hook commands from `claude-status` form to `agent-status --agent claude-code` form. If the diff looks right:

```bash
mv ~/.claude/settings.json.new ~/.claude/settings.json
jq -r '.hooks | to_entries[] | "\(.key): \(.value[0].hooks[0].command)"' ~/.claude/settings.json
```
Expected: 5 lines, each command starting `$HOME/.claude/bin/agent-status`.

- [ ] **Step 5: Back up and update `~/.tmux.conf`**

The `~/.tmux.conf` is a symlink into the user's dotfiles repo. Resolve it and edit the target:

```bash
TARGET=$(readlink -f ~/.tmux.conf 2>/dev/null || readlink ~/.tmux.conf)
echo "tmux config target: $TARGET"
cp "$TARGET" "$TARGET.bak.$(date +%Y%m%d-%H%M%S)"
sed -i.tmp 's|claude-status|agent-status|g' "$TARGET" && rm "$TARGET.tmp"
diff -u "$TARGET.bak."* "$TARGET" | tail -20
```

(`sed -i.tmp` works portably across macOS/GNU; the `rm` removes the backup file that BSD sed creates.)

Reload tmux:

```bash
tmux source-file ~/.tmux.conf 2>&1 && echo "reloaded"
```
Expected: `reloaded` (or, if not in tmux, the source-file may fail silently — that's fine; the config will be picked up next start).

- [ ] **Step 6: Clean up stale state from the old binary**

```bash
rm -rf /tmp/claude-status
```

(If `XDG_RUNTIME_DIR` is set, the path is `${XDG_RUNTIME_DIR}/claude-status` — adjust accordingly.)

- [ ] **Step 7: End-to-end verify**

```bash
# Drop a fake state file under the new dir, confirm status shows it.
mkdir -p /tmp/agent-status
echo '{"agent":"claude-code","project":"verify","cwd":"/tmp","event":"notify","tmux_pane":"","ts":0}' \
  > /tmp/agent-status/fake-session
~/.claude/bin/agent-status status
~/.claude/bin/agent-status list
rm /tmp/agent-status/fake-session
~/.claude/bin/agent-status status
```
Expected:
- After `set`: `[!] verify`
- After `list`: a line `\tverify\tnotify` (empty pane)
- After cleanup: empty.

- [ ] **Step 8: Wait for a real Claude Code hook to fire and verify state lands in the new dir**

Trigger a Claude Code hook by running any command in the active session (the `Stop` hook fires on every turn end). Then:

```bash
ls /tmp/agent-status/
jq . /tmp/agent-status/*
```
Expected: at least one file with the new schema (`agent`, `project`, `cwd`, `event`, `tmux_pane`, `ts`).

If nothing appears within a turn or two, recheck `~/.claude/settings.json`'s hook commands.

---

## Notes & caveats

- **No `SessionEnd` mapping concern (yet).** All five Claude Code events keep their existing roles; the rename + `--agent` flag is purely additive.
- **`Stop`-hook noisiness still applies.** Same caveat as before — every turn end fires it.
- **In-flight state during the rollout.** Between Task 6 (state dir rename in source) and Task 9 Step 4 (settings.json updated to call the new binary), if anyone runs the new binary while old hooks still call `claude-status`, the Claude session will write to the new dir while old hooks try to clear from… nothing. The race is short (a few minutes during the install) and self-heals on the next `Stop` after the settings update. Tolerable; if you want it even tighter, do Task 9 Step 4 immediately after Step 2.

---

## Self-review

**Spec coverage:** Three explicit user requirements:
1. Rename to `agent-status` → Tasks 6, 7, 8, 9.
2. Modular architecture with `Agent` trait + per-agent module → Tasks 1, 2, 5.
3. Make Claude Code one of N agents (don't hard-code) → Tasks 1–5.

User-locked decisions: trait-based modularization (Task 1), `--agent` flag (Task 5), state dir rename + `agent` field (Tasks 3, 6).

**Placeholder scan:** No `TODO`/`TBD`. The `todo!()` macros in Tasks 1 and 2 Step 1 are deliberate (TDD red phase) and replaced in the same task.

**Type consistency:**
- `Agent::name(&self) -> &'static str`: used identically in Tasks 1, 2, 5.
- `Agent::extract_session_id(&self, &str) -> Option<String>`: same shape across all tasks.
- `agents::by_name(&str) -> Option<Box<dyn Agent>>`: used in Tasks 2 and 5 with the same signature.
- `AttentionEntry` with new `agent: String` field: ordered `agent, project, cwd, event, tmux_pane, ts` consistently across Tasks 3, 4, 8.
- `build_entry(agent, event, cwd, tmux_pane, ts)`: parameter order locked in Task 4 and used in Task 5.
- CLI subcommand surface: `Cmd::Set { event, agent }`, `Cmd::Clear { agent }` — locked in Task 5.

**Decision log:** Trait + Box<dyn Agent> chosen over enum + match arms (option declined in clarifying Q): one-file-per-agent makes adding/removing agents a clean local change, and `Box<dyn Agent>` allocation cost (one heap alloc per CLI invocation) is negligible relative to the existing `read_to_string(stdin)` and JSON parse. State dir renamed to `agent-status` (matching the binary) rather than kept as `claude-status` for compat: chosen for consistency with the rename and because there's no remaining bash-version dependency.
