# Split `commands.rs` into per-command files Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the 663-line `crates/agent-status/src/commands.rs` into one file per command-helper and bring every source file into compliance with the project's "public API at the top, private parts at the bottom" rule.

**Architecture:** Convert `commands.rs` (single file, items grouped by feature with public and private items interleaved) into a `commands/` directory with one file per CLI subcommand helper (`set.rs`, `status.rs`, `list.rs`, `agent_extension.rs`) plus a `mod.rs` that re-exports the public API and owns the shared `needs_attention` filter. Pure code reorganization — no behavior change, no API change at `crate::commands::*` for consumers (re-exports preserved). Tests move with their function under test. Also fix `agent-switcher/src/ui.rs` which has two private `const`s above `pub fn draw`.

**Tech Stack:** Rust 2024 edition, Cargo workspace, `cargo test` and `cargo clippy --all-targets --all-features --locked -- -D warnings` are the gates per CLAUDE.md.

---

## Audit summary (read before starting)

Files inspected, with verdict:

| File | Verdict | Notes |
|---|---|---|
| `crates/agent-status/src/commands.rs` | **Major violation** | 663 lines; public/private interleaved by feature grouping (`build_extension` + 4 private helpers, then `build_entry`, then `format_status`, then `format_list` + 3 private helpers). Target of the main split. |
| `crates/agent-switcher/src/ui.rs` | **Minor violation** | `const SPINNER_FRAMES` and `const MESSAGE_CAP` are private declarations sitting above `pub fn draw`. Move them below the public functions. |
| `crates/agent-status/src/state.rs` | Compliant | `pub struct AttentionEntry`, `pub struct StateStore` + impl with public methods at top of impl, private `is_pid_alive` / `validate_session_id` at the bottom. |
| `crates/agent-status/src/agents/mod.rs` | Compliant | All items public (`pub mod`, `pub enum`, `pub trait`, `pub fn by_name`). |
| `crates/agent-status/src/agents/claude_code.rs` | Compliant | `pub struct ClaudeCodeAgent` + `impl Agent` at top, private helpers (`format_pre_tool_use_activity` etc.) at bottom. |
| `crates/agent-status/src/agents/opencode.rs` | Compliant | `pub struct OpencodeAgent` + `impl Agent`, no private items. |
| `crates/agent-status/src/agents/pi_coding_agent.rs` | Compliant | Same shape as opencode. |
| `crates/agent-switcher/src/app.rs` | Compliant | `pub enum KeyOutcome`, `pub struct App`, impl with all `pub fn` methods (`new`, `tick`, `visible_indices`, `selected_entry`, `handle_key`) followed by private `move_down`, `move_up`, `clamp_selection`. |
| `crates/agent-switcher/src/filter.rs` | Compliant | All public. |
| `crates/agent-status/src/main.rs`, `crates/agent-switcher/src/main.rs` | N/A | Binary entry points; `fn main` at the top is the conventional Rust shape and doesn't have a public/private distinction. |

The two files in the "violation" rows are what this plan touches. Everything else is already correct and is not modified.

---

## File structure after this plan

```
crates/agent-status/src/commands/
├── mod.rs                  # re-exports + crate-private `needs_attention`
├── set.rs                  # pub fn build_entry  (used by `set` subcommand)
├── status.rs               # pub fn format_status (used by `status` subcommand)
├── list.rs                 # pub fn format_list  (used by `list` subcommand)
└── agent_extension.rs      # pub struct ExtensionFile + pub fn build_extension
                            #   (used by `agent-extension` subcommand)
```

`crates/agent-status/src/commands.rs` is deleted.

Public-API re-exports at the crate root (`lib.rs`) stay unchanged:

```rust
pub use commands::{
    build_entry, build_extension, format_list, format_status, ExtensionFile,
};
```

`commands::needs_attention` becomes `pub(crate)`, accessible to `commands::status` and `commands::list` via `use super::needs_attention;`.

---

## Why one big commit for the split

Rust's module resolution cannot tolerate both `commands.rs` and `commands/mod.rs` existing at the same time — the compiler errors out with "file for module `commands` found at both ... and ...". So the split has to be atomic: delete the old file *and* create the new directory in the same commit. Intermediate states between Step 1 and Step 5 do not compile, but no test runs between them — `cargo test` is only invoked at Step 6 once everything is in place.

`git mv` is not useful here either; we're not preserving a 1:1 file rename, we're fanning one file out into five.

---

### Task 1: Split `commands.rs` into `commands/` per-command files

**Files:**
- Delete: `crates/agent-status/src/commands.rs`
- Create: `crates/agent-status/src/commands/mod.rs`
- Create: `crates/agent-status/src/commands/set.rs`
- Create: `crates/agent-status/src/commands/status.rs`
- Create: `crates/agent-status/src/commands/list.rs`
- Create: `crates/agent-status/src/commands/agent_extension.rs`

- [ ] **Step 1: Create `crates/agent-status/src/commands/mod.rs`**

Full content:

```rust
//! Helpers used by each `agent-status` subcommand. One file per subcommand
//! (`set`, `status`, `list`, `agent-extension`); `mod.rs` re-exports the
//! public API and houses the shared `needs_attention` filter consumed by
//! both `format_status` and `format_list`.

mod agent_extension;
mod list;
mod set;
mod status;

pub use agent_extension::{build_extension, ExtensionFile};
pub use list::format_list;
pub use set::build_entry;
pub use status::format_status;

/// Whether an `event` value represents a session that wants the user's eyes
/// right now. `notify` is an explicit "Claude is blocked on you" signal;
/// `done` is the just-finished state that the next prompt will move on from.
/// Other values (`working`, `idle`, or anything the agent layer invents
/// later) are alive-but-not-asking and are hidden from the tmux indicator
/// and the legacy fzf TSV. The switcher reads the store directly and
/// surfaces every event value, so this filter does NOT apply there.
pub(crate) fn needs_attention(event: &str) -> bool {
    !matches!(event, "working" | "idle")
}
```

Visibility note: `needs_attention` is `pub(crate)` so the sibling sub-modules `commands::status` and `commands::list` can `use super::needs_attention;`. `pub(crate)` is the minimum visibility that lets both siblings see it without making it part of the external crate API.

- [ ] **Step 2: Create `crates/agent-status/src/commands/set.rs`**

Full content:

```rust
use crate::state::AttentionEntry;
use std::path::Path;

/// Construct an [`AttentionEntry`] from raw inputs.
///
/// `project` is derived as the basename of `cwd`. When `cwd` has no basename (e.g. `/`
/// or empty string), `project` falls back to `cwd` itself. `message` is the agent's
/// last-response text, when the hook payload supplies one; pass `None` otherwise.
pub fn build_entry(
    agent: &str,
    event: &str,
    cwd: &str,
    tmux_pane: &str,
    ts: u64,
    message: Option<&str>,
    pid: Option<u32>,
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
        message: message.map(str::to_string),
        pid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("claude-code", "notify", "/Users/me/work/claude-status", "%5", 42, None, None);
        assert_eq!(e.agent, "claude-code");
        assert_eq!(e.project, "claude-status");
        assert_eq!(e.cwd, "/Users/me/work/claude-status");
        assert_eq!(e.event, "notify");
        assert_eq!(e.tmux_pane, "%5");
        assert_eq!(e.ts, 42);
        assert!(e.message.is_none());
    }

    #[test]
    fn build_entry_falls_back_to_cwd_when_no_basename() {
        let e = build_entry("claude-code", "notify", "/", "", 0, None, None);
        assert_eq!(e.project, "/");
        assert_eq!(e.agent, "claude-code");
    }

    #[test]
    fn build_entry_stores_message_when_some() {
        let e = build_entry(
            "claude-code",
            "notify",
            "/Users/me/work/app",
            "%5",
            42,
            Some("Permission required"),
            None,
        );
        assert_eq!(e.message.as_deref(), Some("Permission required"));
    }

    #[test]
    fn build_entry_leaves_message_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", "%1", 1, None, None);
        assert!(e.message.is_none());
    }

    #[test]
    fn build_entry_stores_pid_when_some() {
        let e = build_entry(
            "claude-code", "notify", "/Users/me/work/app", "%5", 42,
            Some("Permission required"), Some(12345),
        );
        assert_eq!(e.pid, Some(12345));
    }

    #[test]
    fn build_entry_leaves_pid_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", "%1", 1, None, None);
        assert!(e.pid.is_none());
    }
}
```

All six `build_entry_*` tests in this file are the verbatim bodies from the current `commands.rs:239-289` — no edits.

- [ ] **Step 3: Create `crates/agent-status/src/commands/status.rs`**

Full content:

```rust
use super::needs_attention;
use crate::state::AttentionEntry;

/// Format the tmux `status-right` line for the given entries.
///
/// Returns `None` when there are no entries so the caller can omit the line entirely.
/// One entry shows the project name; multiple entries show a count. The output is
/// plain text — styling is left to the tmux config so users can pick their own colors.
#[must_use]
pub fn format_status(entries: &[(String, AttentionEntry)]) -> Option<String> {
    let waiting: Vec<&AttentionEntry> = entries
        .iter()
        .filter(|(_, e)| needs_attention(&e.event))
        .map(|(_, e)| e)
        .collect();
    match waiting.len() {
        0 => None,
        1 => Some(format!("[!] {}", waiting[0].project)),
        n => Some(format!("[!] {n} projects waiting")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: pane.into(),
            ts: 1,
            message: None,
            pid: None,
        }
    }

    #[test]
    fn format_status_empty_returns_none() {
        assert_eq!(format_status(&[]), None);
    }

    #[test]
    fn format_status_single_entry_shows_project_name() {
        let entries = vec![("s1".into(), entry("alpha", "%1", "notify"))];
        assert_eq!(format_status(&entries).as_deref(), Some("[!] alpha"));
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
            Some("[!] 3 projects waiting")
        );
    }

    #[test]
    fn format_status_ignores_working_entries() {
        let e = entry("alpha", "%1", "working");
        assert_eq!(format_status(&[("s1".into(), e)]), None);
    }

    #[test]
    fn format_status_counts_only_non_working_entries() {
        let working = entry("alpha", "%1", "working");
        let waiting = entry("beta", "%2", "notify");
        let entries = vec![
            ("s1".into(), working),
            ("s2".into(), waiting),
        ];
        assert_eq!(format_status(&entries).as_deref(), Some("[!] beta"));
    }

    #[test]
    fn format_status_ignores_idle_entries() {
        let e = entry("alpha", "%1", "idle");
        assert_eq!(format_status(&[("s1".into(), e)]), None);
    }

    #[test]
    fn format_status_counts_only_attention_needing_entries() {
        let idle = entry("alpha", "%1", "idle");
        let working = entry("beta", "%2", "working");
        let waiting = entry("gamma", "%3", "notify");
        let entries = vec![
            ("s1".into(), idle),
            ("s2".into(), working),
            ("s3".into(), waiting),
        ];
        assert_eq!(format_status(&entries).as_deref(), Some("[!] gamma"));
    }
}
```

Note: the local `fn entry(...)` test helper is duplicated here (and in `list.rs`'s test module) because the original `commands.rs` test module shared it across both groups. Duplication is cheaper than a `pub(crate)` test-support module for an 11-line helper. All seven `format_status_*` tests are verbatim from `commands.rs:293-450`.

- [ ] **Step 4: Create `crates/agent-status/src/commands/list.rs`**

Full content:

```rust
use super::needs_attention;
use crate::state::AttentionEntry;

/// Format the popup picker output: `session_id<TAB>pane<TAB>display\n` per entry.
///
/// The first two columns are machine-consumed (`session_id` is the preview key, pane is
/// the `tmux switch-client` target). The third column is a single space-padded display
/// string safe for fzf's `--with-nth=3`: a `[!]`/`[*]` marker (so fzf cannot fuzzy-match
/// the raw event word `notify`/`done`), then the project and agent names padded to the
/// max width in this list, then a one-line snippet of the agent's message if any.
#[must_use]
pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    const PROJECT_CAP: usize = 30;
    const AGENT_CAP: usize = 16;
    const MESSAGE_CAP: usize = 80;

    let visible: Vec<&(String, AttentionEntry)> = entries
        .iter()
        .filter(|(_, e)| needs_attention(&e.event))
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

fn truncate_chars(s: &str, cap: usize) -> String {
    s.chars().take(cap).collect()
}

fn one_line(s: &str, cap: usize) -> String {
    let mut flat = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if !flat.ends_with(' ') {
                flat.push(' ');
            }
        } else {
            flat.push(ch);
        }
    }
    truncate_chars(flat.trim(), cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: pane.into(),
            ts: 1,
            message: None,
            pid: None,
        }
    }

    #[test]
    fn format_list_emits_session_id_pane_display_columns() {
        let entries = vec![
            ("sess-1".into(), entry("alpha", "%1", "notify")),
            ("sess-2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert_eq!(line.matches('\t').count(), 2, "line: {line:?}");
        }
        let cols0: Vec<&str> = lines[0].split('\t').collect();
        assert_eq!(cols0[0], "sess-1");
        assert_eq!(cols0[1], "%1");
        assert!(cols0[2].contains("alpha"));
        assert!(cols0[2].contains("claude-code"));
    }

    #[test]
    fn format_list_uses_bracket_marker_not_event_word() {
        let entries = vec![
            ("s1".into(), entry("alpha", "%1", "notify")),
            ("s2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        for line in out.lines() {
            let display = line.split('\t').nth(2).unwrap();
            assert!(!display.contains("notify"), "display: {display:?}");
            assert!(!display.contains("done"), "display: {display:?}");
        }
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].split('\t').nth(2).unwrap().starts_with("[!] "));
        assert!(lines[1].split('\t').nth(2).unwrap().starts_with("[*] "));
    }

    #[test]
    fn format_list_pads_project_and_agent_columns_to_max_width() {
        let mut a = entry("short", "%1", "notify");
        a.agent = "claude-code".into();
        let mut b = entry("a-much-longer-project-name", "%2", "done");
        b.agent = "pi-coding-agent".into();
        let entries = vec![("s1".into(), a), ("s2".into(), b)];
        let out = format_list(&entries);
        let lines: Vec<&str> = out.lines().collect();
        let display0 = lines[0].split('\t').nth(2).unwrap();
        let display1 = lines[1].split('\t').nth(2).unwrap();
        let agent0 = display0.find("claude-code").expect("agent on line 0");
        let agent1 = display1.find("pi-coding-agent").expect("agent on line 1");
        assert_eq!(agent0, agent1, "agent column not aligned: {display0:?} vs {display1:?}");
    }

    #[test]
    fn format_list_appends_message_snippet_when_present() {
        let mut e = entry("alpha", "%1", "notify");
        e.message = Some("Permission required to read /etc/passwd".into());
        let out = format_list(&[("s1".into(), e)]);
        let display = out.lines().next().unwrap().split('\t').nth(2).unwrap();
        assert!(display.contains("Permission required"), "display: {display:?}");
    }

    #[test]
    fn format_list_collapses_newlines_in_message_snippet() {
        let mut e = entry("alpha", "%1", "notify");
        e.message = Some("line one\nline two\r\nline three".into());
        let out = format_list(&[("s1".into(), e)]);
        assert_eq!(out.matches('\n').count(), 1, "got: {out:?}");
        let display = out.lines().next().unwrap().split('\t').nth(2).unwrap();
        assert!(!display.contains('\n'));
        assert!(!display.contains('\r'));
    }

    #[test]
    fn format_list_truncates_long_message_snippet() {
        let mut e = entry("alpha", "%1", "notify");
        e.message = Some("x".repeat(500));
        let out = format_list(&[("s1".into(), e)]);
        let display = out.lines().next().unwrap().split('\t').nth(2).unwrap();
        assert!(display.len() < 200, "display too long: {} chars", display.len());
    }

    #[test]
    fn format_list_empty_input_returns_empty_string() {
        assert_eq!(format_list(&[]), "");
    }

    #[test]
    fn format_list_ignores_idle_entries() {
        let idle = entry("alpha", "%1", "idle");
        let waiting = entry("beta", "%2", "notify");
        let out = format_list(&[("s1".into(), idle), ("s2".into(), waiting)]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1, "got: {lines:?}");
        assert!(lines[0].contains("beta"));
        assert!(!lines[0].contains("alpha"));
    }

    #[test]
    fn format_list_ignores_working_entries() {
        let working = entry("alpha", "%1", "working");
        let waiting = entry("beta", "%2", "notify");
        let out = format_list(&[
            ("s1".into(), working),
            ("s2".into(), waiting),
        ]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1, "got: {lines:?}");
        assert!(lines[0].contains("beta"));
        assert!(!lines[0].contains("alpha"));
    }
}
```

All nine `format_list_*` tests are verbatim from `commands.rs:317-476`. The private helpers `truncate_chars` and `one_line` move with `format_list`. They are placed after the public `format_list` per the "public API at the top" rule.

- [ ] **Step 5: Create `crates/agent-status/src/commands/agent_extension.rs`**

Full content:

```rust
use crate::agents::AgentName;

/// One generated extension/settings file: the filename to write it as and the
/// content to fill it with. Returned by [`build_extension`] for agents that
/// support a per-launch file-loaded integration (Claude Code's `--settings`,
/// pi's `-e <path>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionFile {
    pub filename: String,
    pub content: String,
}

/// Build the extension/settings file an alias-installed agent loads at launch.
///
/// Every [`AgentName`] variant has a branch (the match is exhaustive), so the
/// caller always gets an [`ExtensionFile`]. `claude-code` uses `--settings
/// <file>`, `pi-coding-agent` uses `-e <file>`, and `opencode`'s in-process
/// plugin file can be copied once. The `filename` member is the basename to
/// write as (`claude-code.json`, `pi-coding-agent.ts`, `opencode.ts`); the
/// `content` member is the file body.
#[must_use]
pub fn build_extension(bin_path: &str, agent: AgentName) -> ExtensionFile {
    match agent {
        AgentName::ClaudeCode => ExtensionFile {
            filename: "claude-code.json".to_string(),
            content: build_claude_code_settings(bin_path),
        },
        AgentName::PiCodingAgent => ExtensionFile {
            filename: "pi-coding-agent.ts".to_string(),
            content: build_pi_extension(bin_path),
        },
        AgentName::Opencode => ExtensionFile {
            filename: "opencode.ts".to_string(),
            content: build_opencode_extension(bin_path),
        },
    }
}

fn build_claude_code_settings(bin_path: &str) -> String {
    let set_notify = format!("{bin_path} set --agent claude-code notify");
    let set_done = format!("{bin_path} set --agent claude-code done");
    let set_working = format!("{bin_path} set --agent claude-code working");
    let set_idle = format!("{bin_path} set --agent claude-code idle");
    let clear = format!("{bin_path} clear --agent claude-code");

    let value = serde_json::json!({
        "hooks": {
            "Notification":      [{"hooks": [{"type": "command", "command": &set_notify}]}],
            "PermissionRequest": [{"hooks": [{"type": "command", "command": set_notify}]}],
            "Stop":              [{"hooks": [{"type": "command", "command": set_done}]}],
            "UserPromptSubmit":  [{"hooks": [{"type": "command", "command": &set_working}]}],
            "PreToolUse":        [{"hooks": [{"type": "command", "command": set_working}]}],
            "SessionStart":      [{"hooks": [{"type": "command", "command": set_idle}]}],
            "SessionEnd":        [{"hooks": [{"type": "command", "command": clear}]}],
        }
    });
    serde_json::to_string_pretty(&value).expect("serde_json::Value always serializes")
}

fn build_pi_extension(bin_path: &str) -> String {
    let template = include_str!("../../extensions/pi-coding-agent.ts");
    let serialized = serde_json::to_string(bin_path).expect("path serializes");
    let replacement = format!("const BIN = {serialized};");
    template.replacen(TS_BIN_RESOLUTION_LINE, &replacement, 1)
}

fn build_opencode_extension(bin_path: &str) -> String {
    let template = include_str!("../../extensions/opencode.ts");
    let serialized = serde_json::to_string(bin_path).expect("path serializes");
    let replacement = format!("const BIN = {serialized};");
    template.replacen(TS_BIN_RESOLUTION_LINE, &replacement, 1)
}

/// The exact BIN-resolution line shared by `extensions/pi-coding-agent.ts`
/// and `extensions/opencode.ts`. Matched verbatim by `str::replacen` so the
/// embedded template can be specialized with an absolute path. If this line
/// drifts in the .ts source, the substitution silently no-ops and the file
/// keeps its env-fallback resolution at runtime — still functional, just
/// not alias-optimized.
const TS_BIN_RESOLUTION_LINE: &str =
    "const BIN = process.env.AGENT_STATUS_BIN ?? \"agent-status\";";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_extension_returns_extension_for_claude_code() {
        let ext = build_extension("/x/agent-status", AgentName::ClaudeCode);
        assert_eq!(ext.filename, "claude-code.json");
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        assert!(parsed.get("hooks").is_some(), "missing top-level hooks key");
    }

    #[test]
    fn build_extension_claude_code_wires_all_hook_events() {
        let ext = build_extension("/x/agent-status", AgentName::ClaudeCode);
        for event in [
            "Notification",
            "PermissionRequest",
            "Stop",
            "UserPromptSubmit",
            "PreToolUse",
            "SessionStart",
            "SessionEnd",
        ] {
            assert!(ext.content.contains(event), "missing hook event {event}");
        }
    }

    #[test]
    fn build_extension_claude_code_uses_set_and_clear_correctly() {
        let ext = build_extension("/path/to/agent-status", AgentName::ClaudeCode);
        assert!(ext.content.contains("set --agent claude-code notify"));
        assert!(ext.content.contains("set --agent claude-code done"));
        assert!(ext.content.contains("clear --agent claude-code"));
        assert!(ext.content.contains("/path/to/agent-status"));
    }

    #[test]
    fn build_extension_escapes_unsafe_chars_in_bin_path() {
        let ext = build_extension(
            r#"/x/has"quote\and-backslash/agent-status"#,
            AgentName::ClaudeCode,
        );
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        let command = parsed
            .pointer("/hooks/Notification/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("notification command string");
        assert!(command.contains(r#"has"quote\and-backslash"#), "got: {command}");
    }

    #[test]
    fn build_extension_returns_pi_coding_agent_extension() {
        let ext = build_extension("/abs/path/agent-status", AgentName::PiCodingAgent);
        assert_eq!(ext.filename, "pi-coding-agent.ts");
        assert!(
            ext.content.contains(r#"const BIN = "/abs/path/agent-status";"#),
            "missing substituted BIN; got:\n{}",
            ext.content,
        );
        assert!(
            !ext.content.contains("process.env.AGENT_STATUS_BIN ??"),
            "env-fallback line should have been replaced",
        );
        assert!(ext.content.contains("export default function"));
    }

    #[test]
    fn build_extension_pi_extension_json_escapes_bin_path() {
        let ext = build_extension(
            r#"/x/has"quote\and-backslash/agent-status"#,
            AgentName::PiCodingAgent,
        );
        assert!(
            ext.content.contains(r#"const BIN = "/x/has\"quote\\and-backslash/agent-status";"#),
            "BIN line not escaped correctly; got:\n{}",
            ext.content,
        );
    }

    #[test]
    fn build_extension_returns_opencode_extension() {
        let ext = build_extension("/abs/path/agent-status", AgentName::Opencode);
        assert_eq!(ext.filename, "opencode.ts");
        assert!(
            ext.content.contains(r#"const BIN = "/abs/path/agent-status";"#),
            "missing substituted BIN; got:\n{}",
            ext.content,
        );
        assert!(
            !ext.content.contains("process.env.AGENT_STATUS_BIN ??"),
            "env-fallback line should have been replaced",
        );
        assert!(ext.content.contains("AgentStatusPlugin"));
    }

    #[test]
    fn build_extension_opencode_extension_json_escapes_bin_path() {
        let ext = build_extension(
            r#"/x/has"quote\and-backslash/agent-status"#,
            AgentName::Opencode,
        );
        assert!(
            ext.content.contains(r#"const BIN = "/x/has\"quote\\and-backslash/agent-status";"#),
            "BIN line not escaped correctly; got:\n{}",
            ext.content,
        );
    }

    #[test]
    fn build_extension_claude_code_user_prompt_submit_sets_working() {
        let ext = build_extension("/path/agent-status", AgentName::ClaudeCode);
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
        let ext = build_extension("/path/agent-status", AgentName::ClaudeCode);
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
    fn build_extension_claude_code_permission_request_sets_notify() {
        // PermissionRequest fires when Claude Code shows a tool-permission dialog
        // (after PreToolUse, before the user clicks Yes/No). Without this hook the
        // PreToolUse-emitted `working` state stays until the user resolves the
        // dialog — so the tmux indicator and agent-switcher would silently miss
        // the "needs you now" transition.
        let ext = build_extension("/path/agent-status", AgentName::ClaudeCode);
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        let cmd = parsed
            .pointer("/hooks/PermissionRequest/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("PermissionRequest command");
        assert!(
            cmd.contains("set --agent claude-code notify"),
            "got: {cmd}",
        );
    }

    #[test]
    fn build_extension_claude_code_session_start_sets_idle() {
        // SessionStart registers the session as `idle` so every Claude session
        // appears in the switcher from the moment it starts — even before the
        // user has typed their first prompt. Clearing on SessionStart (the
        // previous behavior) made the row invisible until UserPromptSubmit or
        // PreToolUse fired.
        let ext = build_extension("/path/agent-status", AgentName::ClaudeCode);
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        let cmd = parsed
            .pointer("/hooks/SessionStart/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("SessionStart command");
        assert!(
            cmd.contains("set --agent claude-code idle"),
            "got: {cmd}",
        );
    }

    #[test]
    fn build_extension_claude_code_session_end_still_clears() {
        // SessionEnd is the only lifecycle event that should remove the row.
        let ext = build_extension("/path/agent-status", AgentName::ClaudeCode);
        let parsed: serde_json::Value = serde_json::from_str(&ext.content).unwrap();
        let cmd = parsed
            .pointer("/hooks/SessionEnd/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("SessionEnd command");
        assert!(
            cmd.contains("clear --agent claude-code"),
            "SessionEnd should still clear; got: {cmd}",
        );
    }
}
```

**Three important details in this file (different from a verbatim move):**

1. **`include_str!` paths change.** The old `commands.rs` was at `crates/agent-status/src/commands.rs`, so `include_str!("../extensions/pi-coding-agent.ts")` resolved to `crates/agent-status/extensions/pi-coding-agent.ts`. The new file is at `crates/agent-status/src/commands/agent_extension.rs`, one directory deeper, so the path becomes `"../../extensions/pi-coding-agent.ts"`. Same change for the opencode `include_str!`.

2. **`TS_BIN_RESOLUTION_LINE` moves to the bottom.** In the old file it sat between `build_pi_extension` and `build_opencode_extension` because it was lexically next to its only callers. With public-API-at-top discipline the const is private, so it goes below the private functions that use it. Rust allows forward references for `const`, so this compiles fine.

3. **The 13 `build_extension_*` tests are verbatim** from `commands.rs:479-672`. No edits.

- [ ] **Step 6: Delete `crates/agent-status/src/commands.rs`**

Run: `rm crates/agent-status/src/commands.rs`

Verification: `ls crates/agent-status/src/` should no longer show `commands.rs`, but should show a `commands/` directory.

- [ ] **Step 7: Run the workspace test gate**

Run: `cargo test`

Expected: identical pass count to before the split. Every test that lived in the old `commands.rs` now lives in one of the new sub-files, and each is exercised under its file's `mod tests`. The other suites (state.rs unit tests, agents/* unit tests, agent-switcher unit tests, the cli integration test) are unaffected. Total should match the pre-refactor baseline (currently 169).

If any test fails: most likely cause is an `include_str!` path that wasn't updated (Step 5, detail 1) or a missing `use super::needs_attention;` in status.rs or list.rs.

- [ ] **Step 8: Run the clippy gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`

Expected: clean. The workspace lints enforce `clippy::pedantic = "warn"`, so any new pedantic finding introduced by the split (e.g. `module_name_repetitions` on `commands::agent_extension::build_extension` — clippy is OK with this because the function name doesn't repeat the module name) needs investigation.

If clippy flags `module_inception` or similar, the most likely cause is the `mod tests { use super::*; ... }` pattern: that's the standard Rust test pattern and is allowed by the existing test modules in `state.rs`, so it should pass here too.

- [ ] **Step 9: Commit**

```bash
git add crates/agent-status/src/commands.rs crates/agent-status/src/commands/
git commit -m "$(cat <<'EOF'
refactor(agent-status): split commands.rs into per-command files

commands.rs had grown to 663 lines with public and private items
interleaved by feature grouping (build_extension and its 4 private
helpers, then build_entry, then format_status, then format_list and its
3 private helpers). Per the project's "public API at the top" rule,
that violated visibility ordering and made navigation hard.

Split into a commands/ directory with one file per CLI subcommand
helper: set.rs (build_entry), status.rs (format_status), list.rs
(format_list + its private helpers), agent_extension.rs (ExtensionFile,
build_extension, and its private builders). commands/mod.rs re-exports
the public API and houses the crate-private needs_attention filter
shared between format_status and format_list. Within each new file,
public items come first and private helpers come after.

Pure code reorganization — no behavior change, no public API change
(lib.rs re-exports still resolve identically).
EOF
)"
```

Verification: `git log --stat -1` should show 1 file deleted and 5 files created, with the line counts roughly summing to the deleted file's count plus a small amount for the `mod.rs` plumbing.

---

### Task 2: Move private `const`s in `agent-switcher/src/ui.rs` below the public functions

`SPINNER_FRAMES` and `MESSAGE_CAP` are private declarations currently sitting between the `use` block and `pub fn draw`. They should live at the bottom with the other private items per the same "public API at the top" rule.

**Files:**
- Modify: `crates/agent-switcher/src/ui.rs:13-14` (move the two `const` declarations)

- [ ] **Step 1: Move the constants**

In `crates/agent-switcher/src/ui.rs`, find these two lines (currently at lines 13-14):

```rust
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MESSAGE_CAP: usize = 80;
```

Delete them from their current location (the blank line between them and `pub fn draw` should also collapse — make sure there is exactly one blank line between the `use` block and `pub fn draw`).

Add them at the bottom of the file, just before the `#[cfg(test)] mod tests { ... }` block, with one blank line on each side:

```rust
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MESSAGE_CAP: usize = 80;

#[cfg(test)]
mod tests {
    ...
}
```

Rust resolves `const`s by name within the module regardless of declaration order, so all existing references (`sessions_table` reads `SPINNER_FRAMES`, `one_line` reads `MESSAGE_CAP`) continue working without any change.

- [ ] **Step 2: Run the test gate**

Run: `cargo test -p agent-switcher`

Expected: all 20 agent-switcher unit tests pass.

- [ ] **Step 3: Run the clippy gate**

Run: `cargo clippy -p agent-switcher --all-targets --all-features --locked -- -D warnings`

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/agent-switcher/src/ui.rs
git commit -m "$(cat <<'EOF'
style(agent-switcher): move private consts in ui.rs below public fns

SPINNER_FRAMES and MESSAGE_CAP are private to the module and were
sitting above pub fn draw — same shape as the commands.rs issue.
Rust resolves consts by name regardless of declaration order, so
this is a pure reorder with no behavior or visibility change.
EOF
)"
```

---

### Task 3: Update `CLAUDE.md` "Module split (load-bearing)" section to describe the new layout

The CLAUDE.md section currently has one bullet for `commands.rs`. After the split it should describe the `commands/` directory and its files.

**Files:**
- Modify: `CLAUDE.md` (the "Module split (load-bearing)" section)

- [ ] **Step 1: Replace the commands.rs bullet**

In `CLAUDE.md`, locate this bullet inside the "Module split (load-bearing)" section:

```markdown
- `crates/agent-status/src/commands.rs` — pure helpers (`build_entry`,
  `format_status`, `format_list`, `build_extension`). No `std::env`,
  `std::io`, `std::time`, or `std::fs` imports.
```

Replace with:

```markdown
- `crates/agent-status/src/commands/` — pure helpers organized one file
  per CLI subcommand. `mod.rs` re-exports the public API
  (`build_entry`, `format_status`, `format_list`, `build_extension`,
  `ExtensionFile`) and owns the crate-private `needs_attention` filter
  shared by `format_status` and `format_list`. `set.rs`, `status.rs`,
  `list.rs`, `agent_extension.rs` each implement one subcommand's
  helper, public API at the top, private helpers below. No `std::env`,
  `std::io`, `std::time`, or `std::fs` imports anywhere.
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude.md): describe the commands/ directory layout after the split"
```

---

## Self-review

**1. Spec coverage:** The user asked for two things — split `commands.rs` per command (Task 1) and audit other files for the same violation (audit summary at the top + Task 2 fixes the one other violation found in `ui.rs`). The docs update (Task 3) keeps `CLAUDE.md`'s "load-bearing" module-split description accurate.

**2. Placeholder scan:** No "TBD", no "implement later", no "similar to Task N", no "write tests for the above" without code. Every code step has the literal content the engineer types into the file. The two places where I said "verbatim from commands.rs:NNN-MMM" are themselves fully specified pointers — the engineer can copy the bytes directly with no judgment call.

**3. Type consistency:** `needs_attention` keeps the same signature (`fn needs_attention(event: &str) -> bool`) and the same body (`!matches!(event, "working" | "idle")`) across the move. `ExtensionFile`'s definition is the verbatim three-field struct. `build_entry`'s seven-parameter signature is preserved. All public re-exports from `commands::*` (`build_entry`, `build_extension`, `format_list`, `format_status`, `ExtensionFile`) are kept, so `lib.rs`'s top-level re-export still resolves. The `include_str!` paths in `agent_extension.rs` are corrected from `"../extensions/..."` to `"../../extensions/..."` to compensate for the deeper file location — this is explicitly called out in Step 5 detail 1.
