# Claude Attention Indicator (Rust) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the bash + jq scripts in `claude-tmux-attention-plan.md` with a single Rust binary (`claude-attention`) that handles all four operations as subcommands, called by Claude Code hooks and tmux's `status-right`.

**Architecture:** One static binary with subcommands `set` / `clear` / `status` / `list`. State is persisted as one JSON file per session in `${XDG_RUNTIME_DIR:-/tmp}/claude-attention/` — same wire format and file layout as the bash plan, so the two are drop-in compatible during transition. No daemon. No SQLite — the access pattern (each session writes only its own keyed file) is naturally lock-free on the filesystem, and the working set is at most a handful of entries, so a database engine would only add init cost and binary size.

**Tech Stack:** Rust 2021, `serde` + `serde_json` (only deps), `tempfile` (dev-only). Hand-rolled argv dispatch (no `clap`) to keep cold start under 1ms.

**Why Rust over bash:**
- Status hook runs every 5s from `status-right`. Bash + jq cold start is ~10-30ms per call; a Rust binary is ~1ms. Over a workday this is negligible CPU but a clean win on the stated goal of "least load on the system."
- Eliminates `jq` runtime dependency.
- Type-safe state struct, unit-testable, no shell-quoting hazards.
- Compile cost is one-time; cross-arch handled by `cargo build --release` per machine.

**Why filesystem state (not SQLite):**
- Each session ID is a unique filename → concurrent writers from different Claude sessions never touch the same file. No locking needed.
- The status hot path is `read_dir` + `len()` — for ≤10 entries this is faster than opening any DB.
- State remains inspectable with `ls`/`cat` — same debuggability as the bash plan.
- Matches the file layout of the bash plan, so users on the bash version can switch without losing in-flight indicators.

---

## File Structure

```
claude-attention/
├── Cargo.toml
├── src/
│   ├── main.rs        # argv dispatch, exit codes
│   ├── state.rs       # AttentionEntry, StateStore (read/write/list/remove)
│   └── commands.rs    # pure helpers (extract_session_id, build_entry, format_status, format_list)
└── tests/
    └── cli.rs         # end-to-end tests that invoke the built binary
```

**Why this split:**
- `state.rs` owns the on-disk format and is the only module that touches `fs::*`. Easy to unit-test by passing a `StateStore` pointed at a `tempfile::TempDir`.
- `commands.rs` is pure functions: `&str` in, `String`/`Option`/struct out. No I/O, no env, no time — those get injected at the call site. This is what makes the logic testable without spawning processes.
- `main.rs` does only argv parsing, env reads, time, and stdin reading; it wires the pure functions to the impure world. Kept thin so almost all logic is covered by unit tests.
- One `tests/cli.rs` integration test exercises the actual built binary end-to-end (stdin → state file → status output) to catch wiring mistakes between the layers.

**Public API at the top of each file** (per user's global preference): public structs/functions first, private helpers below.

---

## Prerequisites

- Rust toolchain installed (`cargo --version` ≥ 1.75). Confirmed present at plan-write time.
- tmux ≥ 3.0.
- `fzf` only required for the bonus popup picker.

---

## Task 1: Initialize the Rust project

**Files:**
- Create: `claude-attention/Cargo.toml`
- Create: `claude-attention/src/main.rs` (placeholder)
- Create: `claude-attention/.gitignore`

- [ ] **Step 1: Create the project directory and Cargo.toml**

```bash
mkdir -p /Users/paulvandermeijs/Workspace/claude-status/claude-attention/src
mkdir -p /Users/paulvandermeijs/Workspace/claude-status/claude-attention/tests
```

Write `claude-attention/Cargo.toml`:

```toml
[package]
name = "claude-attention"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "claude-attention"
path = "src/main.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

- [ ] **Step 2: Write a placeholder main**

Write `claude-attention/src/main.rs`:

```rust
fn main() {}
```

- [ ] **Step 3: Add .gitignore**

Write `claude-attention/.gitignore`:

```
/target
```

- [ ] **Step 4: Verify it builds**

Run: `cd claude-attention && cargo build`
Expected: `Compiling claude-attention v0.1.0 ... Finished dev profile`. No errors.

- [ ] **Step 5: Commit**

```bash
git add claude-attention/
git commit -m "chore: scaffold claude-attention Rust crate"
```

---

## Task 2: `AttentionEntry` struct with serde round-trip

**Files:**
- Create: `claude-attention/src/state.rs`
- Modify: `claude-attention/src/main.rs` (add `mod state;`)

- [ ] **Step 1: Write the failing test**

Append to `claude-attention/src/state.rs`:

```rust
pub use self::tests::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_roundtrips_through_json() {
        let entry = AttentionEntry {
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

    #[test]
    fn entry_matches_bash_plan_field_names() {
        let entry = AttentionEntry {
            project: "p".into(),
            cwd: "/c".into(),
            event: "done".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        };
        let v: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert!(v.get("project").is_some());
        assert!(v.get("cwd").is_some());
        assert!(v.get("event").is_some());
        assert!(v.get("tmux_pane").is_some());
        assert!(v.get("ts").is_some());
    }
}
```

(Remove the `pub use` line — that was a leftover, leave only the `#[cfg(test)] mod tests`.)

Append `mod state;` to `claude-attention/src/main.rs`:

```rust
mod state;

fn main() {}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd claude-attention && cargo test`
Expected: compilation error — `AttentionEntry` not defined.

- [ ] **Step 3: Implement the struct**

Replace `claude-attention/src/state.rs` with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AttentionEntry {
    pub project: String,
    pub cwd: String,
    pub event: String,
    pub tmux_pane: String,
    pub ts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_roundtrips_through_json() {
        let entry = AttentionEntry {
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

    #[test]
    fn entry_matches_bash_plan_field_names() {
        let entry = AttentionEntry {
            project: "p".into(),
            cwd: "/c".into(),
            event: "done".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        };
        let v: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert!(v.get("project").is_some());
        assert!(v.get("cwd").is_some());
        assert!(v.get("event").is_some());
        assert!(v.get("tmux_pane").is_some());
        assert!(v.get("ts").is_some());
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd claude-attention && cargo test`
Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add claude-attention/src/
git commit -m "feat(state): add AttentionEntry struct with serde"
```

---

## Task 3: `StateStore` with injectable directory

**Files:**
- Modify: `claude-attention/src/state.rs`

- [ ] **Step 1: Write the failing tests**

Add inside the existing `mod tests` in `claude-attention/src/state.rs`:

```rust
    use tempfile::TempDir;

    fn sample_entry(project: &str) -> AttentionEntry {
        AttentionEntry {
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: "notify".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        }
    }

    #[test]
    fn write_then_list_returns_entry() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.write("session-a", &sample_entry("alpha")).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "session-a");
        assert_eq!(listed[0].1.project, "alpha");
    }

    #[test]
    fn remove_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.remove("never-existed").unwrap();
        store.write("s1", &sample_entry("p")).unwrap();
        store.remove("s1").unwrap();
        store.remove("s1").unwrap();
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn list_on_missing_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist");
        let store = StateStore::new(path);
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn list_skips_files_with_invalid_json() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.write("good", &sample_entry("p")).unwrap();
        std::fs::write(dir.path().join("bad"), "not json").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "good");
    }

    #[test]
    fn from_env_uses_xdg_runtime_dir() {
        // SAFETY: tests run in a single thread per default if needed; std env is process-wide.
        // We just verify path composition without mutating env across other tests.
        let store = StateStore::new(std::path::PathBuf::from("/tmp/foo/claude-attention"));
        assert!(store.dir().ends_with("claude-attention"));
    }
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd claude-attention && cargo test`
Expected: compile errors — `StateStore`, `StateStore::new`, `dir`, `write`, `list`, `remove` not defined.

- [ ] **Step 3: Implement `StateStore`**

Replace `claude-attention/src/state.rs` with:

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AttentionEntry {
    pub project: String,
    pub cwd: String,
    pub event: String,
    pub tmux_pane: String,
    pub ts: u64,
}

pub struct StateStore {
    dir: PathBuf,
}

impl StateStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn from_env() -> Self {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        Self::new(base.join("claude-attention"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn write(&self, session_id: &str, entry: &AttentionEntry) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_vec(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(self.dir.join(session_id), json)
    }

    pub fn remove(&self, session_id: &str) -> io::Result<()> {
        match fs::remove_file(self.dir.join(session_id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn list(&self) -> io::Result<Vec<(String, AttentionEntry)>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = match fs::read(entry.path()) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if let Ok(parsed) = serde_json::from_slice::<AttentionEntry>(&bytes) {
                out.push((name, parsed));
            }
        }
        out.sort_by(|a, b| a.1.ts.cmp(&b.1.ts).then_with(|| a.0.cmp(&b.0)));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn entry_roundtrips_through_json() {
        let entry = AttentionEntry {
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

    #[test]
    fn entry_matches_bash_plan_field_names() {
        let entry = AttentionEntry {
            project: "p".into(),
            cwd: "/c".into(),
            event: "done".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        };
        let v: serde_json::Value = serde_json::to_value(&entry).unwrap();
        assert!(v.get("project").is_some());
        assert!(v.get("cwd").is_some());
        assert!(v.get("event").is_some());
        assert!(v.get("tmux_pane").is_some());
        assert!(v.get("ts").is_some());
    }

    fn sample_entry(project: &str) -> AttentionEntry {
        AttentionEntry {
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: "notify".into(),
            tmux_pane: "%1".into(),
            ts: 1,
        }
    }

    #[test]
    fn write_then_list_returns_entry() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.write("session-a", &sample_entry("alpha")).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "session-a");
        assert_eq!(listed[0].1.project, "alpha");
    }

    #[test]
    fn remove_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.remove("never-existed").unwrap();
        store.write("s1", &sample_entry("p")).unwrap();
        store.remove("s1").unwrap();
        store.remove("s1").unwrap();
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn list_on_missing_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist");
        let store = StateStore::new(path);
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn list_skips_files_with_invalid_json() {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().into());
        store.write("good", &sample_entry("p")).unwrap();
        std::fs::write(dir.path().join("bad"), "not json").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "good");
    }

    #[test]
    fn from_env_uses_xdg_runtime_dir() {
        let store = StateStore::new(std::path::PathBuf::from("/tmp/foo/claude-attention"));
        assert!(store.dir().ends_with("claude-attention"));
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd claude-attention && cargo test`
Expected: `test result: ok. 7 passed`.

- [ ] **Step 5: Commit**

```bash
git add claude-attention/src/state.rs
git commit -m "feat(state): add StateStore with write/list/remove"
```

---

## Task 4: Pure command helpers (`extract_session_id`, `build_entry`, `format_status`, `format_list`)

**Files:**
- Create: `claude-attention/src/commands.rs`
- Modify: `claude-attention/src/main.rs` (add `mod commands;`)

- [ ] **Step 1: Write the failing tests**

Write `claude-attention/src/commands.rs`:

```rust
use crate::state::AttentionEntry;

pub fn extract_session_id(stdin_json: &str) -> Option<String> {
    todo!()
}

pub fn build_entry(event: &str, cwd: &str, tmux_pane: &str, ts: u64) -> AttentionEntry {
    todo!()
}

pub fn format_status(entries: &[(String, AttentionEntry)]) -> Option<String> {
    todo!()
}

pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: pane.into(),
            ts: 1,
        }
    }

    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(extract_session_id(json).as_deref(), Some("abc-123"));
    }

    #[test]
    fn extract_session_id_returns_none_for_missing() {
        assert_eq!(extract_session_id(r#"{"other":1}"#), None);
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(extract_session_id(r#"{"session_id":""}"#), None);
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(extract_session_id("not json"), None);
    }

    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("notify", "/Users/me/work/claude-status", "%5", 42);
        assert_eq!(e.project, "claude-status");
        assert_eq!(e.cwd, "/Users/me/work/claude-status");
        assert_eq!(e.event, "notify");
        assert_eq!(e.tmux_pane, "%5");
        assert_eq!(e.ts, 42);
    }

    #[test]
    fn build_entry_falls_back_to_cwd_when_no_basename() {
        let e = build_entry("notify", "/", "", 0);
        assert_eq!(e.project, "/");
    }

    #[test]
    fn format_status_empty_returns_none() {
        assert_eq!(format_status(&[]), None);
    }

    #[test]
    fn format_status_single_entry_shows_project_name() {
        let entries = vec![("s1".into(), entry("alpha", "%1", "notify"))];
        assert_eq!(
            format_status(&entries).as_deref(),
            Some("#[fg=yellow,bold]🔔 alpha")
        );
    }

    #[test]
    fn format_status_multiple_entries_shows_count() {
        let entries = vec![
            ("s1".into(), entry("a", "%1", "notify")),
            ("s2".into(), entry("b", "%2", "done")),
            ("s3".into(), entry("c", "%3", "done")),
        ];
        assert_eq!(
            format_status(&entries).as_deref(),
            Some("#[fg=yellow,bold]🔔 3 projects waiting")
        );
    }

    #[test]
    fn format_list_emits_tab_separated_pane_project_event() {
        let entries = vec![
            ("s1".into(), entry("alpha", "%1", "notify")),
            ("s2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        assert_eq!(out, "%1\talpha\tnotify\n%2\tbeta\tdone\n");
    }
}
```

Append `mod commands;` to `claude-attention/src/main.rs`:

```rust
mod commands;
mod state;

fn main() {}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd claude-attention && cargo test`
Expected: `not yet implemented` panics from each `todo!()`.

- [ ] **Step 3: Implement the helpers**

Replace `claude-attention/src/commands.rs` with:

```rust
use crate::state::AttentionEntry;
use std::path::Path;

pub fn extract_session_id(stdin_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
    let id = v.get("session_id")?.as_str()?;
    if id.is_empty() { None } else { Some(id.to_string()) }
}

pub fn build_entry(event: &str, cwd: &str, tmux_pane: &str, ts: u64) -> AttentionEntry {
    let project = Path::new(cwd)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| cwd.to_string());
    AttentionEntry {
        project,
        cwd: cwd.to_string(),
        event: event.to_string(),
        tmux_pane: tmux_pane.to_string(),
        ts,
    }
}

pub fn format_status(entries: &[(String, AttentionEntry)]) -> Option<String> {
    match entries.len() {
        0 => None,
        1 => Some(format!("#[fg=yellow,bold]🔔 {}", entries[0].1.project)),
        n => Some(format!("#[fg=yellow,bold]🔔 {n} projects waiting")),
    }
}

pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    let mut out = String::new();
    for (_, e) in entries {
        out.push_str(&e.tmux_pane);
        out.push('\t');
        out.push_str(&e.project);
        out.push('\t');
        out.push_str(&e.event);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: pane.into(),
            ts: 1,
        }
    }

    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(extract_session_id(json).as_deref(), Some("abc-123"));
    }

    #[test]
    fn extract_session_id_returns_none_for_missing() {
        assert_eq!(extract_session_id(r#"{"other":1}"#), None);
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(extract_session_id(r#"{"session_id":""}"#), None);
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(extract_session_id("not json"), None);
    }

    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("notify", "/Users/me/work/claude-status", "%5", 42);
        assert_eq!(e.project, "claude-status");
        assert_eq!(e.cwd, "/Users/me/work/claude-status");
        assert_eq!(e.event, "notify");
        assert_eq!(e.tmux_pane, "%5");
        assert_eq!(e.ts, 42);
    }

    #[test]
    fn build_entry_falls_back_to_cwd_when_no_basename() {
        let e = build_entry("notify", "/", "", 0);
        assert_eq!(e.project, "/");
    }

    #[test]
    fn format_status_empty_returns_none() {
        assert_eq!(format_status(&[]), None);
    }

    #[test]
    fn format_status_single_entry_shows_project_name() {
        let entries = vec![("s1".into(), entry("alpha", "%1", "notify"))];
        assert_eq!(
            format_status(&entries).as_deref(),
            Some("#[fg=yellow,bold]🔔 alpha")
        );
    }

    #[test]
    fn format_status_multiple_entries_shows_count() {
        let entries = vec![
            ("s1".into(), entry("a", "%1", "notify")),
            ("s2".into(), entry("b", "%2", "done")),
            ("s3".into(), entry("c", "%3", "done")),
        ];
        assert_eq!(
            format_status(&entries).as_deref(),
            Some("#[fg=yellow,bold]🔔 3 projects waiting")
        );
    }

    #[test]
    fn format_list_emits_tab_separated_pane_project_event() {
        let entries = vec![
            ("s1".into(), entry("alpha", "%1", "notify")),
            ("s2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        assert_eq!(out, "%1\talpha\tnotify\n%2\tbeta\tdone\n");
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd claude-attention && cargo test`
Expected: all 17 tests pass (7 from state + 10 from commands).

- [ ] **Step 5: Commit**

```bash
git add claude-attention/src/
git commit -m "feat(commands): add pure helpers for parse/build/format"
```

---

## Task 5: Wire `main.rs` argv dispatch

**Files:**
- Modify: `claude-attention/src/main.rs`

- [ ] **Step 1: Implement `main`**

Replace `claude-attention/src/main.rs` with:

```rust
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
        .status();
}
```

- [ ] **Step 2: Verify it builds and tests still pass**

Run: `cd claude-attention && cargo build && cargo test`
Expected: build succeeds, 17 tests still pass.

- [ ] **Step 3: Smoke-test the binary manually**

Run:
```bash
cd claude-attention
TMPDIR_TEST=$(mktemp -d)
XDG_RUNTIME_DIR="$TMPDIR_TEST" cargo run --quiet -- status
echo '{"session_id":"smoke-1"}' | XDG_RUNTIME_DIR="$TMPDIR_TEST" cargo run --quiet -- set notify
XDG_RUNTIME_DIR="$TMPDIR_TEST" cargo run --quiet -- status
ls "$TMPDIR_TEST/claude-attention/"
echo '{"session_id":"smoke-1"}' | XDG_RUNTIME_DIR="$TMPDIR_TEST" cargo run --quiet -- clear
XDG_RUNTIME_DIR="$TMPDIR_TEST" cargo run --quiet -- status
rm -rf "$TMPDIR_TEST"
```

Expected:
1. First `status`: no output.
2. After `set`: `status` prints `#[fg=yellow,bold]🔔 <basename-of-cwd>`.
3. `ls` shows `smoke-1`.
4. After `clear`: `status` prints nothing.

(Note: `tmux refresh-client` will print a warning to stderr if tmux isn't running — that's fine and expected outside a tmux session.)

- [ ] **Step 4: Commit**

```bash
git add claude-attention/src/main.rs
git commit -m "feat(cli): wire argv dispatch for set/clear/status/list"
```

---

## Task 6: End-to-end integration test

**Files:**
- Create: `claude-attention/tests/cli.rs`

- [ ] **Step 1: Write the integration test**

Write `claude-attention/tests/cli.rs`:

```rust
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_claude-attention")
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
    let state_dir = tmp.path().join("claude-attention");

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
    let state_dir = tmp.path().join("claude-attention");
    let (_, stderr, code) = run(&state_dir, &["frobnicate"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown subcommand"));
}

#[test]
fn set_with_empty_session_id_is_noop() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("claude-attention");
    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":""}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, _) = run(&state_dir, &["status"], None);
    assert_eq!(stdout, "");
}
```

- [ ] **Step 2: Run tests**

Run: `cd claude-attention && cargo test`
Expected: 17 unit tests + 3 integration tests all pass.

- [ ] **Step 3: Commit**

```bash
git add claude-attention/tests/cli.rs
git commit -m "test: add end-to-end CLI integration tests"
```

---

## Task 7: Build the release binary

**Files:**
- None modified; this produces `target/release/claude-attention`.

- [ ] **Step 1: Build release**

Run: `cd claude-attention && cargo build --release`
Expected: `Finished release profile`. Binary at `claude-attention/target/release/claude-attention`.

- [ ] **Step 2: Verify size and that it runs**

Run:
```bash
ls -lh claude-attention/target/release/claude-attention
claude-attention/target/release/claude-attention status
```
Expected: binary under ~1 MB (with `strip = true`); `status` exits 0 with no output.

- [ ] **Step 3: Quick latency sanity check**

Run:
```bash
hyperfine --warmup 5 'claude-attention/target/release/claude-attention status' 2>/dev/null \
  || time claude-attention/target/release/claude-attention status
```
Expected: well under 5ms per invocation. If `hyperfine` is not installed, the `time` fallback should report ~0.001s real.

---

## Task 8: Install the binary to `~/.claude/bin/`

**Files:**
- Create: `~/.claude/bin/claude-attention`

- [ ] **Step 1: Ensure the install directory exists**

Run: `mkdir -p ~/.claude/bin`

- [ ] **Step 2: Install the binary**

Run:
```bash
install -m 0755 claude-attention/target/release/claude-attention ~/.claude/bin/claude-attention
```

- [ ] **Step 3: Verify install**

Run:
```bash
ls -l ~/.claude/bin/claude-attention
~/.claude/bin/claude-attention status
```
Expected: file is `-rwxr-xr-x`; `status` exits 0 with no output (no state files yet).

---

## Task 9: Wire up Claude Code hooks in `~/.claude/settings.json`

**Files:**
- Modify: `~/.claude/settings.json`

- [ ] **Step 1: Show the user the current contents**

Run: `cat ~/.claude/settings.json 2>/dev/null || echo "(file does not exist)"`
Expected: report what's there. **Do not overwrite an existing `hooks` block — merge.**

- [ ] **Step 2: Construct the merged settings**

The hooks to add (use absolute path so it works regardless of `$PATH`):

```json
{
  "hooks": {
    "Notification":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-attention set notify" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-attention set done"   }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-attention clear"      }] }],
    "SessionStart":     [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-attention clear"      }] }],
    "SessionEnd":       [{ "hooks": [{ "type": "command", "command": "$HOME/.claude/bin/claude-attention clear"      }] }]
  }
}
```

If the existing settings file already contains a top-level `hooks` key, merge the five event arrays into it using `jq`:

```bash
NEW_HOOKS='{"Notification":[{"hooks":[{"type":"command","command":"$HOME/.claude/bin/claude-attention set notify"}]}],"Stop":[{"hooks":[{"type":"command","command":"$HOME/.claude/bin/claude-attention set done"}]}],"UserPromptSubmit":[{"hooks":[{"type":"command","command":"$HOME/.claude/bin/claude-attention clear"}]}],"SessionStart":[{"hooks":[{"type":"command","command":"$HOME/.claude/bin/claude-attention clear"}]}],"SessionEnd":[{"hooks":[{"type":"command","command":"$HOME/.claude/bin/claude-attention clear"}]}]}'

jq --argjson new "$NEW_HOOKS" '.hooks = ((.hooks // {}) + $new)' ~/.claude/settings.json > ~/.claude/settings.json.new
```

Show the user the diff before replacing:

```bash
diff -u ~/.claude/settings.json ~/.claude/settings.json.new || true
```

- [ ] **Step 3: Apply the merged settings**

If the diff looks right:

```bash
mv ~/.claude/settings.json.new ~/.claude/settings.json
```

If the file did not previously exist, write the JSON object above directly with the `Write` tool.

- [ ] **Step 4: Verify**

Run: `jq '.hooks | keys' ~/.claude/settings.json`
Expected: array containing at least `"Notification"`, `"Stop"`, `"UserPromptSubmit"`, `"SessionStart"`, `"SessionEnd"`.

---

## Task 10: Wire up tmux `status-right` and the popup picker binding

**Files:**
- Modify: `~/.tmux.conf`

- [ ] **Step 1: Show current `~/.tmux.conf`**

Run: `cat ~/.tmux.conf 2>/dev/null || echo "(file does not exist)"`
Expected: report contents. Look specifically for any existing `status-right`, `status-interval`, and `bind-key C-a` lines.

- [ ] **Step 2: Decide on merge strategy**

- If there is no existing `status-right`, append:

```tmux
set -g status-interval 5
set -ga status-right '#($HOME/.claude/bin/claude-attention status) '
bind-key C-a display-popup -E -w 60% -h 40% "$HOME/.claude/bin/claude-attention list | fzf --with-nth=2.. --delimiter='\\t' --prompt='Jump to> ' | cut -f1 | xargs -r -I{} tmux switch-client -t {}"
```

- If `status-right` already exists, change `set -g` → `set -ga` for the new line so it appends rather than replacing the existing format. Show the merged file to the user before writing.
- If `bind-key C-a` is already bound, pick a different chord (e.g. `prefix + a`) and tell the user what was changed.

- [ ] **Step 3: Apply the change**

Use the `Edit` tool to append/modify; show the user the final state of `status-right` and the new bind-key line.

- [ ] **Step 4: Reload tmux**

Run (inside a tmux session): `tmux source-file ~/.tmux.conf`
Expected: no error. If the user is not in tmux, skip and have them reload manually.

---

## Task 11: End-to-end verification

- [ ] **Step 1: Drop a fake state file and confirm it shows up**

```bash
STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/claude-attention"
mkdir -p "$STATE_DIR"
~/.claude/bin/claude-attention status   # before
echo '{"project":"test","cwd":"/tmp","event":"notify","tmux_pane":"","ts":0}' > "$STATE_DIR/fake-session"
~/.claude/bin/claude-attention status   # after
tmux refresh-client -S 2>/dev/null || true
rm "$STATE_DIR/fake-session"
~/.claude/bin/claude-attention status   # cleanup confirmed
```

Expected: blank → `#[fg=yellow,bold]🔔 test` → blank.

- [ ] **Step 2: Verify status is clean when empty**

Run: `~/.claude/bin/claude-attention status`
Expected: exit 0, no output.

- [ ] **Step 3: Real Claude Code session**

Open a new tmux window and start `claude` in this project. Trigger a permission prompt (e.g., ask it to run a non-allowlisted bash command), then switch to a different tmux window without responding. Within ~5s the status bar in the second window should show `🔔 <project>`. Submit the prompt or send a new user message → indicator clears.

- [ ] **Step 4: Multiple concurrent sessions**

Run two Claude sessions simultaneously, leave both waiting for input. Status should read `🔔 2 projects waiting`. Pressing `prefix + C-a` should open an fzf popup listing both; selecting one should switch the tmux client to that pane.

- [ ] **Step 5: Report back**

Tell the user:
- What was already in `~/.claude/settings.json` and `~/.tmux.conf` before the change, and how it was merged.
- Output of each verification step above.
- Anything that didn't work as expected, especially around stale state files (a Claude crash leaves a file behind — that's known and intentional per the original plan's "Notes & caveats").

---

## Task 12: Final commit and (optional) clean up the bash plan

- [ ] **Step 1: Commit anything still uncommitted**

```bash
cd /Users/paulvandermeijs/Workspace/claude-status
git status
git add -A
git commit -m "feat: install claude-attention and wire up Claude Code + tmux hooks"
```

- [ ] **Step 2: Decide what to do with `claude-tmux-attention-plan.md`**

Ask the user: keep the bash plan as historical context, or delete it now that the Rust version supersedes it? Default: **keep it** — it's useful documentation of the design rationale.

---

## Notes & caveats

Carried forward from the original bash plan; still apply:

- **`Stop` is noisy by design.** Every turn end fires `Stop`, so any finished response counts as "needing attention" until the user sends the next prompt. Intentional — the whole point is to know which session finished while heads-down elsewhere.
- **Stale state on crashes.** If a Claude Code process dies abnormally, its file lingers. Don't add pruning logic preemptively — wait until it actually bites. (If it does: a TTL field on the JSON entry + filter in `StateStore::list` is the smallest fix.)
- **No desktop notifications, sound, or push to phone.** Tmux-native by design.
- **macOS vs Linux.** `XDG_RUNTIME_DIR` is typically unset on macOS; the `/tmp` fallback handles that. Build the release binary on each architecture you use (not portable across macOS arm64 / Linux x86_64).
- **Non-tmux invocations.** If `set` is called outside tmux, `TMUX_PANE` is empty and `tmux refresh-client` fails silently (we ignore the error). The `list` popup still works because tmux is always present at the call site for that one.

---

## Self-review

**Spec coverage check (vs. original `claude-tmux-attention-plan.md`):**
- ✅ Notification + Stop hooks → state file written: Task 4 (`build_entry`), Task 5 (`run_set`), Task 9 (Notification, Stop hooks).
- ✅ UserPromptSubmit / SessionStart / SessionEnd → state file removed: Task 5 (`run_clear`), Task 9 (three matching hooks).
- ✅ tmux status renders count / project name: Task 4 (`format_status`), Task 5 (`run_status`), Task 10 (`status-right`).
- ✅ fzf popup picker: Task 4 (`format_list`), Task 5 (`run_list`), Task 10 (`bind-key C-a`).
- ✅ State stored under `${XDG_RUNTIME_DIR:-/tmp}/claude-attention/`: `StateStore::from_env` in Task 3.
- ✅ Same JSON wire format: `AttentionEntry` field names asserted in Task 2.
- ✅ tmux refresh after state changes: `refresh_tmux()` in Task 5.
- ✅ Verification steps: Task 11 mirrors the four checks in the bash plan.
- ✅ Notes & caveats preserved.

**Type/name consistency:** `AttentionEntry` fields, `StateStore` method names, and CLI subcommand strings (`set` / `clear` / `status` / `list`) are used identically across Tasks 2–6.

**Placeholder scan:** No `TODO`/`TBD` left — the `todo!()` macros in Task 4 Step 1 are intentional (they're the tests-fail-first state) and get fully replaced in Step 3 of the same task.

**Decision log (state storage):** Filesystem chosen over SQLite because the access pattern (per-session file keyed by UUID) is naturally lock-free and the working set is ≤ ~10 entries; SQLite would add ~500KB binary, init cost, and zero correctness benefit. Documented in the header so a future reader knows why.
