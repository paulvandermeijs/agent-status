# Improve Session Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the raw TSV `agent-status list` output with a tidied, padded display column plus a separate `preview` subcommand so the fzf popup picker shows aligned rows, doesn't fuzzy-match on event labels, and reveals rich detail (project, agent, age, last agent message) in a side preview.

**Architecture:** Three coordinated changes. (1) `AttentionEntry` gains an optional `message: Option<String>` so we can persist the agent's last response when the hook payload includes one. (2) `agent-status list` switches to a 3-column TSV — `session_id\tpane\tdisplay` — where `display` is a single space-padded column safe for fzf's `--with-nth=3`; this removes raw tabs from the visible output and drops the bare event word from the matchable text. (3) A new `agent-status preview <session_id>` subcommand reads the state directory and renders a multi-line detail block (project, agent, event, cwd, pane, age, message) for fzf's `--preview`. Wire format stays backward-compatible: `message` is `#[serde(default, skip_serializing_if = "Option::is_none")]`. The pi and opencode bridge extensions are updated to forward whatever message-shaped field their host SDK exposes; Claude Code's `Notification` payload already carries `message` directly, while its `Stop` payload does not (per scope decision: stdin-only, no transcript file I/O).

**Tech Stack:** Rust 2021 (clap derive, serde, serde_json, tempfile for tests), TypeScript (pi & opencode plugin bridges using Node's `child_process`), tmux + fzf for the picker UI.

---

## File Structure

**Files this plan touches:**

- Modify: `src/state.rs` — add `message: Option<String>` field to `AttentionEntry`; update tests.
- Modify: `src/commands.rs` — extend `build_entry` to accept message; rewrite `format_list` for the new 3-column padded format; add `format_preview`.
- Modify: `src/agents/mod.rs` — add `extract_message` to the `Agent` trait with a default `None` implementation.
- Modify: `src/agents/claude_code.rs` — implement `extract_message` (reads `message` field from Claude Code Notification payloads).
- Modify: `src/agents/opencode.rs` — implement `extract_message` (reads `message` field that the bridge will forward).
- Modify: `src/agents/pi_coding_agent.rs` — implement `extract_message` (reads `message` field that the bridge will forward).
- Modify: `src/main.rs` — wire `extract_message` into `run_set`; add `Preview { session_id }` subcommand and `run_preview` glue function.
- Modify: `tests/cli.rs` — update integration test for new `list` shape; add `preview` end-to-end test.
- Modify: `extensions/pi-coding-agent.ts` — forward a best-effort `message` field on `agent_end`.
- Modify: `extensions/opencode.ts` — forward a best-effort `message` field on `session.idle` and `permission.updated`.
- Modify: `README.md` — update tmux popup-picker snippet, the state-file example, and the `Usage` section.

Existing module boundaries are preserved: pure helpers stay in `commands.rs`, filesystem I/O stays in `state.rs`, clap glue stays in `main.rs`, agent-specific JSON parsing stays in `src/agents/*.rs`.

---

### Task 1: Add `message` field to `AttentionEntry`

**Files:**
- Modify: `src/state.rs`

- [ ] **Step 1: Write the failing test**

In `src/state.rs`, add this test inside the existing `mod tests` block (just below `entry_matches_bash_plan_field_names`):

```rust
    #[test]
    fn entry_message_field_roundtrips_when_set() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "p".into(),
            cwd: "/c".into(),
            event: "notify".into(),
            tmux_pane: "%1".into(),
            ts: 1,
            message: Some("Permission required".into()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""message":"Permission required""#));
        let parsed: AttentionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.message.as_deref(), Some("Permission required"));
    }

    #[test]
    fn entry_message_field_omitted_from_json_when_none() {
        let entry = AttentionEntry {
            agent: "claude-code".into(),
            project: "p".into(),
            cwd: "/c".into(),
            event: "done".into(),
            tmux_pane: "%1".into(),
            ts: 1,
            message: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("message"), "got: {json}");
    }

    #[test]
    fn entry_deserializes_when_message_field_absent() {
        // Old state files written before this field was added must still load.
        let json = r#"{"agent":"claude-code","project":"p","cwd":"/c","event":"done","tmux_pane":"%1","ts":1}"#;
        let parsed: AttentionEntry = serde_json::from_str(json).unwrap();
        assert!(parsed.message.is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib state::tests::entry_message_field_roundtrips_when_set state::tests::entry_message_field_omitted_from_json_when_none state::tests::entry_deserializes_when_message_field_absent`

Expected: compile error — `AttentionEntry` has no `message` field; existing struct literals in other tests will also break.

- [ ] **Step 3: Add the field with serde attributes**

In `src/state.rs`, modify the `AttentionEntry` struct:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
    /// Optional last-message text from the agent (e.g. Claude Code Notification's `message`
    /// field). Absent in the JSON when `None`; absent on entries written by older binaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
```

- [ ] **Step 4: Update existing struct literals in `state.rs` tests**

In `src/state.rs`, update the two existing `AttentionEntry { ... }` literals inside `mod tests` (in `entry_roundtrips_through_json` and `entry_matches_bash_plan_field_names`) and the `sample_entry` helper to add `message: None,` as the last field. After editing, the `sample_entry` helper looks like:

```rust
    fn sample_entry(project: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: "notify".into(),
            tmux_pane: "%1".into(),
            ts: 1,
            message: None,
        }
    }
```

The literal in `entry_roundtrips_through_json` similarly gets `message: None,` appended; same for the literal in `entry_matches_bash_plan_field_names`.

- [ ] **Step 5: Run all `state.rs` tests to verify they pass**

Run: `cargo test --lib state::`

Expected: all `state` unit tests pass, including the three new ones.

- [ ] **Step 6: Commit**

```bash
git add src/state.rs
git commit -m "feat(state): add optional message field to AttentionEntry"
```

---

### Task 2: Extend `build_entry` to accept an optional message

**Files:**
- Modify: `src/commands.rs`

- [ ] **Step 1: Write the failing test**

In `src/commands.rs`, add this test inside `mod tests` (just below `build_entry_falls_back_to_cwd_when_no_basename`):

```rust
    #[test]
    fn build_entry_stores_message_when_some() {
        let e = build_entry(
            "claude-code",
            "notify",
            "/Users/me/work/app",
            "%5",
            42,
            Some("Permission required"),
        );
        assert_eq!(e.message.as_deref(), Some("Permission required"));
    }

    #[test]
    fn build_entry_leaves_message_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", "%1", 1, None);
        assert!(e.message.is_none());
    }
```

Also update the existing `build_entry_uses_basename_of_cwd_as_project` and `build_entry_falls_back_to_cwd_when_no_basename` tests to pass `None` as the new last argument:

```rust
    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("claude-code", "notify", "/Users/me/work/claude-status", "%5", 42, None);
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
        let e = build_entry("claude-code", "notify", "/", "", 0, None);
        assert_eq!(e.project, "/");
        assert_eq!(e.agent, "claude-code");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::tests::build_entry_stores_message_when_some commands::tests::build_entry_leaves_message_none_when_none`

Expected: compile error — `build_entry`'s arity is wrong.

- [ ] **Step 3: Update `build_entry` signature and body**

In `src/commands.rs`, replace the existing `build_entry` function:

```rust
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
    }
}
```

Also update the local `entry` helper inside `mod tests` so existing test cases compile:

```rust
    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: pane.into(),
            ts: 1,
            message: None,
        }
    }
```

- [ ] **Step 4: Run `commands` tests to verify they pass**

Run: `cargo test --lib commands::`

Expected: all `commands` unit tests pass. (Note: the call site in `src/main.rs` will fail to compile right now — that's intentional and gets fixed in Task 7. Run only `--lib commands::` for this task.)

- [ ] **Step 5: Commit**

```bash
git add src/commands.rs
git commit -m "feat(commands): thread optional message through build_entry"
```

---

### Task 3: Add `extract_message` to the `Agent` trait

**Files:**
- Modify: `src/agents/mod.rs`

- [ ] **Step 1: Write the failing test**

In `src/agents/mod.rs`, add this test inside `mod tests`:

```rust
    #[test]
    fn extract_message_default_returns_none() {
        // A hand-rolled agent that doesn't override extract_message should get None.
        struct NoopAgent;
        impl Agent for NoopAgent {
            fn name(&self) -> &'static str { "noop" }
            fn extract_session_id(&self, _: &str) -> Option<String> { None }
        }
        assert!(NoopAgent.extract_message(r#"{"message":"hi"}"#).is_none());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib agents::tests::extract_message_default_returns_none`

Expected: compile error — `Agent` has no `extract_message` method.

- [ ] **Step 3: Add the trait method with a default implementation**

In `src/agents/mod.rs`, replace the `Agent` trait body:

```rust
/// An agent implementation: knows how to extract a session ID from the JSON payload that
/// agent's hook delivers on stdin.
pub trait Agent {
    /// Stable, lowercase, hyphenated identifier (e.g. `"claude-code"`). Used for the
    /// `--agent` CLI flag and the `agent` field on persisted entries.
    fn name(&self) -> &'static str;

    /// Extract the session ID from the agent's hook event JSON. Returns `None` for
    /// invalid JSON, missing field, non-string value, or empty string.
    fn extract_session_id(&self, stdin_json: &str) -> Option<String>;

    /// Extract the agent's last-response text from the hook event JSON, when the
    /// payload carries one. Returns `None` when the field is absent, empty, or
    /// non-string. Default returns `None`; override in agents whose payload includes
    /// such text.
    fn extract_message(&self, _stdin_json: &str) -> Option<String> {
        None
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib agents::tests::extract_message_default_returns_none`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/agents/mod.rs
git commit -m "feat(agents): add extract_message to Agent trait with default None"
```

---

### Task 4: Implement `extract_message` for Claude Code

**Files:**
- Modify: `src/agents/claude_code.rs`

- [ ] **Step 1: Write the failing tests**

In `src/agents/claude_code.rs`, append these tests inside `mod tests`:

```rust
    #[test]
    fn extract_message_returns_string_when_present() {
        let json = r#"{"session_id":"x","message":"Permission required"}"#;
        assert_eq!(
            ClaudeCodeAgent.extract_message(json).as_deref(),
            Some("Permission required")
        );
    }

    #[test]
    fn extract_message_returns_none_when_field_missing() {
        let json = r#"{"session_id":"x"}"#;
        assert!(ClaudeCodeAgent.extract_message(json).is_none());
    }

    #[test]
    fn extract_message_returns_none_when_empty() {
        let json = r#"{"session_id":"x","message":""}"#;
        assert!(ClaudeCodeAgent.extract_message(json).is_none());
    }

    #[test]
    fn extract_message_returns_none_for_non_string_value() {
        let json = r#"{"session_id":"x","message":42}"#;
        assert!(ClaudeCodeAgent.extract_message(json).is_none());
    }

    #[test]
    fn extract_message_returns_none_for_invalid_json() {
        assert!(ClaudeCodeAgent.extract_message("not json").is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib agents::claude_code::tests::extract_message_`

Expected: All five new tests fail because `ClaudeCodeAgent` uses the trait default (always `None`).

- [ ] **Step 3: Implement `extract_message` on `ClaudeCodeAgent`**

In `src/agents/claude_code.rs`, replace the `impl Agent for ClaudeCodeAgent` block:

```rust
impl Agent for ClaudeCodeAgent {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn extract_session_id(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let id = v.get("session_id")?.as_str()?;
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }

    fn extract_message(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let m = v.get("message")?.as_str()?;
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib agents::claude_code::tests`

Expected: all Claude Code tests pass (existing five + new five).

- [ ] **Step 5: Commit**

```bash
git add src/agents/claude_code.rs
git commit -m "feat(agents/claude-code): extract message field from Notification payload"
```

---

### Task 5: Implement `extract_message` for opencode

**Files:**
- Modify: `src/agents/opencode.rs`

- [ ] **Step 1: Write the failing tests**

In `src/agents/opencode.rs`, append inside `mod tests`:

```rust
    #[test]
    fn extract_message_returns_string_when_present() {
        let json = r#"{"session_id":"x","message":"Plan ready for review"}"#;
        assert_eq!(
            OpencodeAgent.extract_message(json).as_deref(),
            Some("Plan ready for review")
        );
    }

    #[test]
    fn extract_message_returns_none_when_field_missing() {
        assert!(OpencodeAgent.extract_message(r#"{"session_id":"x"}"#).is_none());
    }

    #[test]
    fn extract_message_returns_none_when_empty() {
        assert!(OpencodeAgent.extract_message(r#"{"session_id":"x","message":""}"#).is_none());
    }

    #[test]
    fn extract_message_returns_none_for_non_string_value() {
        assert!(OpencodeAgent.extract_message(r#"{"session_id":"x","message":[]}"#).is_none());
    }

    #[test]
    fn extract_message_returns_none_for_invalid_json() {
        assert!(OpencodeAgent.extract_message("not json").is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib agents::opencode::tests::extract_message_`

Expected: all five fail.

- [ ] **Step 3: Implement `extract_message`**

In `src/agents/opencode.rs`, replace the `impl Agent for OpencodeAgent` block:

```rust
impl Agent for OpencodeAgent {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn extract_session_id(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let id = v.get("session_id")?.as_str()?;
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }

    fn extract_message(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let m = v.get("message")?.as_str()?;
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib agents::opencode::tests`

Expected: all opencode tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/agents/opencode.rs
git commit -m "feat(agents/opencode): extract message field from bridge payload"
```

---

### Task 6: Implement `extract_message` for pi-coding-agent

**Files:**
- Modify: `src/agents/pi_coding_agent.rs`

- [ ] **Step 1: Write the failing tests**

In `src/agents/pi_coding_agent.rs`, append inside `mod tests`:

```rust
    #[test]
    fn extract_message_returns_string_when_present() {
        let json = r#"{"session_id":"x","message":"Done with refactor"}"#;
        assert_eq!(
            PiCodingAgent.extract_message(json).as_deref(),
            Some("Done with refactor")
        );
    }

    #[test]
    fn extract_message_returns_none_when_field_missing() {
        assert!(PiCodingAgent.extract_message(r#"{"session_id":"x"}"#).is_none());
    }

    #[test]
    fn extract_message_returns_none_when_empty() {
        assert!(PiCodingAgent.extract_message(r#"{"session_id":"x","message":""}"#).is_none());
    }

    #[test]
    fn extract_message_returns_none_for_non_string_value() {
        assert!(PiCodingAgent.extract_message(r#"{"session_id":"x","message":null}"#).is_none());
    }

    #[test]
    fn extract_message_returns_none_for_invalid_json() {
        assert!(PiCodingAgent.extract_message("not json").is_none());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib agents::pi_coding_agent::tests::extract_message_`

Expected: all five fail.

- [ ] **Step 3: Implement `extract_message`**

In `src/agents/pi_coding_agent.rs`, replace the `impl Agent for PiCodingAgent` block:

```rust
impl Agent for PiCodingAgent {
    fn name(&self) -> &'static str {
        "pi-coding-agent"
    }

    fn extract_session_id(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let id = v.get("session_id")?.as_str()?;
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }

    fn extract_message(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
        let m = v.get("message")?.as_str()?;
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib agents::pi_coding_agent::tests`

Expected: all pi-coding-agent tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/agents/pi_coding_agent.rs
git commit -m "feat(agents/pi-coding-agent): extract message field from bridge payload"
```

---

### Task 7: Wire `extract_message` into `run_set` and fix the call site

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update `run_set` to read the message and pass it through**

In `src/main.rs`, replace the `run_set` function:

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

    let message = agent.extract_message(&buf);
    let entry = build_entry(agent.name(), event, &cwd, &pane, ts, message.as_deref());
    store.write(&session_id, &entry)?;
    refresh_tmux();
    Ok(())
}
```

- [ ] **Step 2: Run the full unit test suite to confirm nothing else broke**

Run: `cargo test --lib`

Expected: all unit tests pass. (Integration tests still pass too — `run_set` semantics for set/clear/status are unchanged when the payload omits `message`.)

- [ ] **Step 3: Run clippy gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`

Expected: clean (no warnings).

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): forward extracted message from set hook into stored entry"
```

---

### Task 8: Rewrite `format_list` to a 3-column padded TSV

This task replaces the current `pane\tproject\tevent\n` output with `session_id\tpane\tdisplay\n`, where `display` is a single space-padded column safe for `fzf --with-nth=3`. Project and agent are padded to the max width seen in the current list (capped at sensible limits); event is encoded as a non-word marker (`[!]` for `notify`, `[*]` otherwise) so fzf doesn't fuzzy-match against the raw event word; the agent's message snippet (if any) appears at the line tail, single-line and truncated.

**Files:**
- Modify: `src/commands.rs`

- [ ] **Step 1: Write the failing tests**

In `src/commands.rs`, **delete** the existing `format_list_emits_tab_separated_pane_project_event` test (the wire format is changing; this assertion is no longer correct). Then add these tests inside `mod tests`:

```rust
    #[test]
    fn format_list_emits_session_id_pane_display_columns() {
        let entries = vec![
            ("sess-1".into(), entry("alpha", "%1", "notify")),
            ("sess-2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        // Each line has exactly two tabs: session_id<TAB>pane<TAB>display
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
        // The bare event word must not appear in the display column,
        // so fzf doesn't fuzzy-match against "notify"/"done".
        for line in out.lines() {
            let display = line.split('\t').nth(2).unwrap();
            assert!(!display.contains("notify"), "display: {display:?}");
            assert!(!display.contains("done"), "display: {display:?}");
        }
        // Notify rows get [!]; other rows get [*].
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
        // The project column slot occupies max(len("short"), len("a-much-longer-project-name")) chars.
        // Easiest invariant to check: the position of the agent token is the same on both lines.
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
        // The display column must be exactly one line (no embedded newlines).
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
        // Cap at 80 chars of the message body itself.
        assert!(display.len() < 200, "display too long: {} chars", display.len());
    }

    #[test]
    fn format_list_empty_input_returns_empty_string() {
        assert_eq!(format_list(&[]), "");
    }
```

(Note: the existing `entry` helper inside `mod tests` already builds an `AttentionEntry` with `message: None` after Task 2, so `e.message = Some(...)` works on the value returned by `entry(...)`.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib commands::tests::format_list_`

Expected: most fail. The current implementation emits `pane\tproject\tevent` not `session_id\tpane\tdisplay`, has no padding, has no marker, has no snippet handling.

- [ ] **Step 3: Replace `format_list` with the padded-display implementation**

In `src/commands.rs`, replace the existing `format_list` function:

```rust
/// Format the popup picker output: `session_id<TAB>pane<TAB>display\n` per entry.
///
/// The first two columns are machine-consumed (session_id is the preview key, pane is
/// the `tmux switch-client` target). The third column is a single space-padded display
/// string safe for fzf's `--with-nth=3`: a `[!]`/`[*]` marker (so fzf cannot fuzzy-match
/// the raw event word `notify`/`done`), then the project and agent names padded to the
/// max width in this list, then a one-line snippet of the agent's message if any.
pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    const PROJECT_CAP: usize = 30;
    const AGENT_CAP: usize = 16;
    const MESSAGE_CAP: usize = 80;

    if entries.is_empty() {
        return String::new();
    }

    let project_width = entries
        .iter()
        .map(|(_, e)| e.project.chars().count().min(PROJECT_CAP))
        .max()
        .unwrap_or(0);
    let agent_width = entries
        .iter()
        .map(|(_, e)| e.agent.chars().count().min(AGENT_CAP))
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for (sid, e) in entries {
        let marker = if e.event == "notify" { "[!]" } else { "[*]" };
        let project = truncate_chars(&e.project, PROJECT_CAP);
        let agent = truncate_chars(&e.agent, AGENT_CAP);
        let snippet = e
            .message
            .as_deref()
            .map(|m| one_line(m, MESSAGE_CAP))
            .unwrap_or_default();

        let mut display = format!(
            "{marker} {project:<pw$}  {agent:<aw$}",
            pw = project_width,
            aw = agent_width,
        );
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
```

- [ ] **Step 4: Run all `commands` tests to verify they pass**

Run: `cargo test --lib commands::`

Expected: all `commands` tests pass (existing build_entry tests, new format_list tests, format_status tests).

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/commands.rs
git commit -m "feat(commands): rewrite format_list with padded display column"
```

---

### Task 9: Add `format_preview` to `commands.rs`

The preview text is plain-ASCII multi-line: project, agent, event, cwd, pane, age (computed from `now - ts`), and message body if any. No ANSI colors — fzf renders the text plainly so it's the user's terminal that colorizes. Age uses a tiny human-readable formatter (e.g. `5s`, `2m 14s`, `1h 03m`, `3d 04h`).

**Files:**
- Modify: `src/commands.rs`

- [ ] **Step 1: Write the failing tests**

In `src/commands.rs`, add these tests inside `mod tests`:

```rust
    #[test]
    fn format_preview_includes_core_fields() {
        let mut e = entry("alpha", "%17", "notify");
        e.cwd = "/Users/x/work/alpha".into();
        e.ts = 1_000;
        let out = format_preview(&e, 1_000 + 134); // 134 seconds later
        assert!(out.contains("Project:"));
        assert!(out.contains("alpha"));
        assert!(out.contains("Agent:"));
        assert!(out.contains("claude-code"));
        assert!(out.contains("Event:"));
        assert!(out.contains("notify"));
        assert!(out.contains("CWD:"));
        assert!(out.contains("/Users/x/work/alpha"));
        assert!(out.contains("Pane:"));
        assert!(out.contains("%17"));
        assert!(out.contains("Age:"));
        assert!(out.contains("2m"), "expected 2m in: {out}");
    }

    #[test]
    fn format_preview_omits_message_section_when_none() {
        let e = entry("alpha", "%17", "done");
        let out = format_preview(&e, e.ts);
        assert!(!out.contains("Message:"), "got: {out}");
    }

    #[test]
    fn format_preview_includes_message_section_when_some() {
        let mut e = entry("alpha", "%17", "notify");
        e.message = Some("Permission required\nfor /etc/passwd".into());
        let out = format_preview(&e, e.ts);
        assert!(out.contains("Message:"));
        assert!(out.contains("Permission required"));
        // Multi-line messages should be preserved (unlike the list snippet).
        assert!(out.contains("for /etc/passwd"));
    }

    #[test]
    fn format_preview_age_handles_seconds_minutes_hours_days() {
        let e = entry("p", "%1", "done"); // ts = 1 from helper
        assert!(format_preview(&e, e.ts + 0).contains("Age:        0s"));
        assert!(format_preview(&e, e.ts + 9).contains("Age:        9s"));
        assert!(format_preview(&e, e.ts + 75).contains("Age:        1m 15s"));
        assert!(format_preview(&e, e.ts + 3_600 + 120).contains("Age:        1h 02m"));
        assert!(format_preview(&e, e.ts + 3 * 86_400 + 4 * 3_600).contains("Age:        3d 04h"));
    }

    #[test]
    fn format_preview_age_clamps_when_now_before_ts() {
        // Defense against clock skew: if now < ts, render as 0s rather than panicking.
        let mut e = entry("p", "%1", "done");
        e.ts = 100;
        let out = format_preview(&e, 50);
        assert!(out.contains("Age:        0s"), "got: {out}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib commands::tests::format_preview_`

Expected: compile error — `format_preview` does not exist.

- [ ] **Step 3: Implement `format_preview` and the age helper**

In `src/commands.rs`, append below the existing `format_list` function (and above `mod tests`):

```rust
/// Format the multi-line preview shown in fzf's `--preview` pane for one entry.
///
/// `now_ts` is the caller-supplied current Unix time in seconds, used to render `Age:`.
/// The output is plain ASCII: a label-aligned key/value block, optionally followed by
/// a `Message:` section when `entry.message` is `Some`. The `Message:` body preserves
/// embedded newlines verbatim so multi-line agent responses read naturally.
pub fn format_preview(entry: &AttentionEntry, now_ts: u64) -> String {
    let age = now_ts.saturating_sub(entry.ts);
    let mut out = String::new();
    out.push_str(&format!("Project:    {}\n", entry.project));
    out.push_str(&format!("Agent:      {}\n", entry.agent));
    out.push_str(&format!("Event:      {}\n", entry.event));
    out.push_str(&format!("CWD:        {}\n", entry.cwd));
    out.push_str(&format!(
        "Pane:       {}\n",
        if entry.tmux_pane.is_empty() {
            "-"
        } else {
            entry.tmux_pane.as_str()
        }
    ));
    out.push_str(&format!("Age:        {}\n", format_age(age)));
    if let Some(msg) = entry.message.as_deref() {
        out.push('\n');
        out.push_str("Message:\n");
        out.push_str(msg);
        if !msg.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn format_age(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m {s:02}s")
    } else if secs < 86_400 {
        let h = secs / 3_600;
        let m = (secs % 3_600) / 60;
        format!("{h}h {m:02}m")
    } else {
        let d = secs / 86_400;
        let h = (secs % 86_400) / 3_600;
        format!("{d}d {h:02}h")
    }
}
```

- [ ] **Step 4: Run the new tests to verify they pass**

Run: `cargo test --lib commands::tests::format_preview_`

Expected: all five new tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/commands.rs
git commit -m "feat(commands): add format_preview for fzf preview pane"
```

---

### Task 10: Add the `Preview` subcommand and `run_preview` glue

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add the `Preview` clap subcommand**

In `src/main.rs`, replace the `Cmd` enum:

```rust
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
}
```

Update the `match` in `main` to route the new subcommand:

```rust
    let result = match cli.command {
        Cmd::Set { event, agent } => run_set(&store, &agent, &event),
        Cmd::Clear { agent } => run_clear(&store, &agent),
        Cmd::Status => run_status(&store, &mut io::stdout().lock()),
        Cmd::List => run_list(&store, &mut io::stdout().lock()),
        Cmd::Preview { session_id } => {
            run_preview(&store, &session_id, &mut io::stdout().lock())
        }
    };
```

Update the imports near the top of `main.rs` to bring in `format_preview`:

```rust
use commands::{build_entry, format_list, format_preview, format_status};
```

- [ ] **Step 2: Add `run_preview`**

In `src/main.rs`, add this function (next to `run_list`):

```rust
fn run_preview(store: &StateStore, session_id: &str, out: &mut impl Write) -> io::Result<()> {
    let entries = store.list()?;
    let Some((_, entry)) = entries.into_iter().find(|(sid, _)| sid == session_id) else {
        return Ok(());
    };
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    write!(out, "{}", format_preview(&entry, now_ts))?;
    Ok(())
}
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cargo build`

Expected: clean build.

- [ ] **Step 4: Run the full unit test suite to confirm no regressions**

Run: `cargo test --lib`

Expected: all unit tests pass.

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): add preview subcommand for fzf preview pane"
```

---

### Task 11: Update integration tests for the new `list` shape and add a `preview` end-to-end test

**Files:**
- Modify: `tests/cli.rs`

- [ ] **Step 1: Add the new integration tests**

In `tests/cli.rs`, append:

```rust
#[test]
fn list_outputs_session_id_pane_display_columns() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":"sess-list","message":"Permission required"}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, code) = run(&state_dir, &["list"], None);
    assert_eq!(code, 0);
    let line = stdout.lines().next().expect("at least one line");
    let cols: Vec<&str> = line.split('\t').collect();
    assert_eq!(cols.len(), 3, "expected 3 columns, got: {cols:?}");
    assert_eq!(cols[0], "sess-list");
    // pane is empty in tests because TMUX_PANE is removed by `run`.
    assert_eq!(cols[1], "");
    // Display column starts with the [!]/[*] marker, not the raw event word.
    assert!(cols[2].starts_with("[!] "), "got: {:?}", cols[2]);
    assert!(!cols[2].contains("notify"), "event word leaked: {:?}", cols[2]);
    assert!(cols[2].contains("Permission required"));
}

#[test]
fn preview_prints_multi_line_detail_for_known_session() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (_, _, code) = run(
        &state_dir,
        &["set", "notify"],
        Some(r#"{"session_id":"sess-prev","message":"Hello from agent"}"#),
    );
    assert_eq!(code, 0);

    let (stdout, _, code) = run(&state_dir, &["preview", "sess-prev"], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("Project:"));
    assert!(stdout.contains("Agent:"));
    assert!(stdout.contains("claude-code"));
    assert!(stdout.contains("Event:"));
    assert!(stdout.contains("Message:"));
    assert!(stdout.contains("Hello from agent"));
}

#[test]
fn preview_unknown_session_id_exits_zero_with_empty_output() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    let (stdout, _, code) = run(&state_dir, &["preview", "no-such-session"], None);
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
}
```

- [ ] **Step 2: Run the integration suite to verify the tests pass**

Run: `cargo test --test cli`

Expected: all integration tests pass (existing three plus the three new ones).

- [ ] **Step 3: Run the full test suite as a final gate**

Run: `cargo test`

Expected: all tests pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add tests/cli.rs
git commit -m "test(cli): cover new list shape and preview subcommand"
```

---

### Task 12: Forward a best-effort `message` field from the pi bridge

The pi extension currently sends `{ session_id }`. The relevant lifecycle event for surfacing a message is `agent_end` — that's where pi has just produced a response the user will read after switching to the pane. We extract whatever last-response text is reachable through the event payload and `ctx`, falling back to omitting the field. We use optional chaining throughout so unknown shapes degrade silently.

**Files:**
- Modify: `extensions/pi-coding-agent.ts`

- [ ] **Step 1: Update the bridge to extract and forward `message`**

In `extensions/pi-coding-agent.ts`, replace the file contents:

```typescript
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { spawn } from "node:child_process";
import { basename } from "node:path";

/**
 * Bridges pi-coding-agent lifecycle events to the `agent-status` CLI so pi
 * sessions waiting on user input show up in tmux's status-right.
 *
 * Install: copy this file to `~/.pi/agent/extensions/pi-coding-agent.ts`.
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 * On Windows, `process.env.HOME` is undefined — set `AGENT_STATUS_BIN`
 * explicitly to an absolute path or the spawn will silently no-op.
 */
export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => fire(ctx, undefined, "clear"));
  pi.on("session_shutdown", async (_event, ctx) =>
    fire(ctx, undefined, "clear"),
  );
  pi.on("before_agent_start", async (_event, ctx) =>
    fire(ctx, undefined, "clear"),
  );
  pi.on("agent_end", async (event, ctx) =>
    fire(ctx, lastAgentMessage(event, ctx), "set", "done"),
  );
}

const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(
  ctx: any,
  message: string | undefined,
  action: Action,
  event?: SetEvent,
): void {
  const sessionId = sessionIdFromCtx(ctx);
  if (!sessionId) return;

  const args =
    action === "set"
      ? ["set", "--agent", "pi-coding-agent", event!]
      : ["clear", "--agent", "pi-coding-agent"];

  const child = spawn(BIN, args, {
    stdio: ["pipe", "ignore", "ignore"],
  });
  child.on("error", () => {
    // best-effort: agent-status may not be installed; never crash pi
  });
  const payload: Record<string, string> = { session_id: sessionId };
  if (message) payload.message = message;
  child.stdin?.end(JSON.stringify(payload));
}

function sessionIdFromCtx(ctx: any): string | null {
  const file: string | null | undefined =
    ctx?.sessionManager?.getSessionFile?.();
  if (!file) return null;
  // pi session filenames are "<timestamp>_<uuid>.jsonl" — pull the UUID out.
  const match = basename(file, ".jsonl").match(/_([0-9a-f-]{36})$/i);
  return match ? match[1] : null;
}

/**
 * Best-effort extraction of the last assistant text from pi's `agent_end`
 * payload. The exact field name depends on pi's runtime shape; we probe a
 * handful of plausible spots and silently fall through when nothing is
 * present, in which case the JSON sent to `agent-status set` simply omits
 * the `message` field and the Rust side stores `message: None`.
 */
function lastAgentMessage(event: any, ctx: any): string | undefined {
  const candidates: unknown[] = [
    event?.response?.text,
    event?.message?.text,
    event?.lastMessage?.text,
    ctx?.lastAgentResponse?.text,
    ctx?.lastMessage?.text,
  ];
  for (const c of candidates) {
    if (typeof c === "string" && c.trim().length > 0) return c;
  }
  return undefined;
}
```

- [ ] **Step 2: Type-check the file**

Run: `npx tsc --noEmit extensions/pi-coding-agent.ts 2>&1 | head -50`

Expected: no errors related to this file. (The file uses `any` heavily by design — the pi SDK types don't cover the lifecycle context fully, and that's already the pattern in the existing code.)

- [ ] **Step 3: Commit**

```bash
git add extensions/pi-coding-agent.ts
git commit -m "feat(extensions/pi): forward best-effort agent message on agent_end"
```

---

### Task 13: Forward a best-effort `message` field from the opencode bridge

The opencode plugin currently sends `{ session_id }`. We extract the most relevant text from each event type: for `permission.updated`, a short synthetic label describing the permission; for `session.idle`, any title/summary text the event surfaces. Best-effort with optional chaining; absent fields just yield no `message` in the payload.

**Files:**
- Modify: `extensions/opencode.ts`

- [ ] **Step 1: Update the bridge to extract and forward `message`**

In `extensions/opencode.ts`, replace the file contents:

```typescript
import { spawnSync } from "node:child_process";

/**
 * Bridges opencode lifecycle events to the `agent-status` CLI so opencode
 * sessions waiting on user input show up in tmux's status-right.
 *
 * Install: copy this file to `~/.config/opencode/plugins/opencode.ts`
 * (or `.opencode/plugins/opencode.ts` for per-project install).
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 * On Windows, `process.env.HOME` is undefined — set `AGENT_STATUS_BIN`
 * explicitly to an absolute path or the spawn will silently no-op.
 */
export const AgentStatusPlugin = async () => {
  return {
    event: async ({ event }: { event: any }) => {
      switch (event?.type) {
        case "session.idle":
          fire(
            event.properties?.sessionID,
            "set",
            "done",
            sessionIdleMessage(event),
          );
          return;
        case "permission.updated":
          fire(
            event.properties?.sessionID,
            "set",
            "notify",
            permissionMessage(event),
          );
          return;
        case "session.created":
        case "session.deleted":
          fire(event.properties?.info?.id, "clear");
          return;
      }
    },
  };
};

const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(
  sessionId: string | undefined,
  action: Action,
  event?: SetEvent,
  message?: string,
): void {
  if (!sessionId) return;

  const args =
    action === "set"
      ? ["set", "--agent", "opencode", event!]
      : ["clear", "--agent", "opencode"];

  const payload: Record<string, string> = { session_id: sessionId };
  if (message) payload.message = message;

  // spawnSync rather than spawn: in `opencode run` headless mode the parent
  // exits immediately after `session.idle` and an async child has no time to
  // execute. Blocking ~5-50ms here is invisible in practice and works in TUI
  // mode too. `error` (e.g. ENOENT when agent-status isn't installed) is
  // returned on the result object, not thrown — so we ignore it for
  // best-effort behavior.
  spawnSync(BIN, args, {
    input: JSON.stringify(payload),
    stdio: ["pipe", "ignore", "ignore"],
    timeout: 1000,
  });
}

/**
 * Best-effort extraction of a human-readable label from a `session.idle`
 * event. Probes commonly-used fields; returns `undefined` if nothing
 * suitable is present, in which case `message` is omitted from the payload.
 */
function sessionIdleMessage(event: any): string | undefined {
  const candidates: unknown[] = [
    event?.properties?.info?.title,
    event?.properties?.info?.summary,
    event?.properties?.title,
    event?.properties?.summary,
  ];
  for (const c of candidates) {
    if (typeof c === "string" && c.trim().length > 0) return c;
  }
  return undefined;
}

/**
 * Synthesize a short label for a `permission.updated` event. Falls back to
 * a generic "Permission requested" string when no specific action text is
 * reachable on the event.
 */
function permissionMessage(event: any): string {
  const action: unknown =
    event?.properties?.action ??
    event?.properties?.tool ??
    event?.properties?.title;
  if (typeof action === "string" && action.trim().length > 0) {
    return `Permission requested: ${action}`;
  }
  return "Permission requested";
}
```

- [ ] **Step 2: Type-check the file**

Run: `npx tsc --noEmit extensions/opencode.ts 2>&1 | head -50`

Expected: no errors related to this file.

- [ ] **Step 3: Commit**

```bash
git add extensions/opencode.ts
git commit -m "feat(extensions/opencode): forward best-effort agent message on idle/permission"
```

---

### Task 14: Update the README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the tmux popup-picker snippet**

In `README.md`, replace the existing tmux popup picker block (currently around lines 105-110, the `bind-key C-a display-popup ...` snippet) with:

```tmux
bind-key C-a display-popup -E -w 80% -h 50% \
  "$HOME/.claude/bin/agent-status list | fzf \
     --delimiter='\\t' \
     --with-nth=3 \
     --preview='$HOME/.claude/bin/agent-status preview {1}' \
     --preview-window=right:50%:wrap \
     --prompt='Jump to> ' \
   | cut -f2 | xargs -r -I{} tmux switch-client -t {}"
```

Then directly under the snippet, add a short paragraph explaining the new format:

```markdown
`agent-status list` emits `session_id<TAB>pane<TAB>display` per waiting session.
fzf shows only the third column (`--with-nth=3`), uses the first column as the
preview key (`{1}` → `agent-status preview <session_id>`), and the post-selection
`cut -f2` extracts the pane to feed `tmux switch-client`. The display column
encodes the event as `[!]` (notify) or `[*]` (done) so fuzzy-find matches the
project, agent, and message snippet rather than the bare event word.
```

- [ ] **Step 2: Update the state-file example**

In `README.md`, replace the `## State location` JSON example to include the new optional `message` field:

```json
{"agent":"claude-code","project":"agent-status","cwd":"/path/to/project","event":"notify","tmux_pane":"%17","ts":1778163565,"message":"Permission required"}
```

And add a sentence after the example:

```markdown
The `message` field is optional and only present when the agent's hook payload
supplies one (e.g. Claude Code's `Notification` event). Older state files written
before this field existed still load — `message` defaults to absent.
```

- [ ] **Step 3: Update the `## Usage` section**

In `README.md`, in the `## Usage` code block (currently around lines 116-122), replace it with:

```sh
agent-status --help                       # top-level help
agent-status set [EVENT] [--agent NAME]   # mark this session as waiting (reads JSON on stdin)
agent-status clear [--agent NAME]         # clear this session's state (reads JSON on stdin)
agent-status status                       # print the status-right line, empty if nothing waiting
agent-status list                         # print TSV (session_id, pane, display) per waiting session
agent-status preview <SESSION_ID>         # multi-line detail for one session (used by fzf --preview)
```

- [ ] **Step 4: Run the test suite once more for the final gate**

Run: `cargo test && cargo clippy --all-targets --all-features --locked -- -D warnings`

Expected: all tests pass; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs(readme): document new list/preview shape and updated tmux snippet"
```

---

### Task 15: Manual smoke test of the picker

This is a hands-on verification step the engineer runs interactively — the unit + integration tests cover correctness of the binary, but the fzf preview behavior and visual alignment have to be eyeballed.

**Files:**
- None modified.

- [ ] **Step 1: Reinstall the freshly built binary**

Per `CLAUDE.md`, hooks invoke the binary at `~/.claude/bin/agent-status`, not `target/release/agent-status`. Reinstall:

Run:
```sh
cargo build --release && install -m 0755 target/release/agent-status ~/.claude/bin/agent-status
```

Expected: build succeeds; install copies the binary.

- [ ] **Step 2: Seed two state entries by hand**

Run:
```sh
mkdir -p "${XDG_RUNTIME_DIR:-/tmp}/agent-status"
printf '%s' '{"agent":"claude-code","project":"alpha","cwd":"/tmp/alpha","event":"notify","tmux_pane":"%1","ts":'$(date +%s)',"message":"Permission required to read /etc/passwd"}' > "${XDG_RUNTIME_DIR:-/tmp}/agent-status/sess-alpha"
printf '%s' '{"agent":"opencode","project":"a-much-longer-project","cwd":"/tmp/long","event":"done","tmux_pane":"%2","ts":'$(date +%s)'}' > "${XDG_RUNTIME_DIR:-/tmp}/agent-status/sess-long"
```

Expected: two files created.

- [ ] **Step 3: Eyeball `list` output**

Run: `~/.claude/bin/agent-status list`

Expected output shape (widths will adapt to the data):

```
sess-alpha	%1	[!] alpha                    claude-code       Permission required to read /etc/passwd
sess-long	%2	[*] a-much-longer-project    opencode
```

Verify: three TSV columns; `[!]` and `[*]` markers; `alpha` and `a-much-longer-project` left-pad so `claude-code` and `opencode` line up vertically when columns are visualized as a table.

- [ ] **Step 4: Eyeball `preview`**

Run: `~/.claude/bin/agent-status preview sess-alpha`

Expected: a multi-line block with `Project:`, `Agent:`, `Event:`, `CWD:`, `Pane:`, `Age:` (a small number of seconds), then a blank line, then `Message:` and the message body. Run again with `sess-long` and confirm the `Message:` section is absent (since that fixture has no message).

- [ ] **Step 5: Drive the picker inside tmux**

In a tmux session, source the updated `~/.tmux.conf` and press `<prefix> C-a` (or whichever binding you set). Verify:
  - The list shows aligned columns (no raw tab whitespace).
  - The preview pane on the right (per `--preview-window=right:50%:wrap`) updates as you move between rows.
  - Typing `alpha` matches the alpha row; typing `notify` does NOT match (the bare event word is gone from the display).
  - Selecting a row runs `tmux switch-client -t %<pane>` and you land on that pane.

- [ ] **Step 6: Clean up the seed data**

Run: `rm -f "${XDG_RUNTIME_DIR:-/tmp}/agent-status/sess-alpha" "${XDG_RUNTIME_DIR:-/tmp}/agent-status/sess-long"`

- [ ] **Step 7: No commit**

This task only verifies — nothing changed.

---

## Self-review notes

- **Spec coverage.** All three improvement axes from the prompt are covered: layout cleanup (Task 8 — padded display column, marker instead of event word), additional context (Tasks 1–7, 12–13 — `message` field plumbed end-to-end through state, agents, and bridges), and fzf preview (Tasks 9–11, 14 — `preview` subcommand and tmux snippet update).
- **No placeholders.** Every step contains the exact code or command. Padding caps and age formatting are concrete numbers, not "TBD".
- **Type/name consistency.** `format_preview` is referenced consistently across `commands.rs`, `main.rs` import, and `run_preview` call site. `extract_message` signature `(&self, &str) -> Option<String>` matches across trait, all three agent impls, and the call site in `run_set`. The new TSV column order `session_id\tpane\tdisplay` is matched by the fzf invocation (`--with-nth=3`, `cut -f2`, `{1}`) in the README and by the integration tests.
- **Wire compat.** The `entry_matches_bash_plan_field_names` guard (mentioned in `CLAUDE.md` as load-bearing) keeps passing because the new `message` field is additive and `serde(skip_serializing_if = "Option::is_none")` keeps it absent in the JSON when there's no message — the bash-precursor field set is unchanged.
- **Module split.** Pure helpers (`format_list`, `format_preview`, `format_age`, `truncate_chars`, `one_line`) all live in `commands.rs`. Filesystem I/O stays in `state.rs`. Clap glue and impure `now()` calls stay in `main.rs`. The agent abstraction grows by one method (`extract_message`) with a default — the architecture invariant from `CLAUDE.md` ("No changes to `state.rs`, `commands.rs`, or `main.rs` should be needed for a typical new agent") is preserved going forward, since adding a new agent still doesn't have to touch the new method unless that agent has a message to surface.
