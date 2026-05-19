# Switcher Activity & Permission Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each row in `agent-switcher` what the agent is currently doing — the active tool/operation for working entries (e.g. "Reading src/main.rs", "Running: git status"), and the notification message for waiting entries (e.g. "Claude needs your permission to use Bash") — so the user can pick the right session at a glance.

**Architecture:** The `notify` half is already wired — `Notification`'s `message` payload flows through `ClaudeCodeAgent::extract_message` → `AttentionEntry.message` → the switcher's snippet column. The gap is the `working` half: when the hook payload is a `PreToolUse` event it carries `tool_name`+`tool_input` instead of a top-level `message`, so today's `extract_message` returns `None`. We extend that one method to synthesize a one-line activity string from the tool fields. No schema change (we reuse the existing `message` field), no other agents touched, no new trait method.

**Tech Stack:** Rust 2021, `serde_json::Value` for payload probing, ratatui (label-only change), existing test infrastructure (`tempfile::TempDir`, `CARGO_BIN_EXE_agent-status`).

---

## File Structure

- **Modify:** `crates/agent-status/src/agents/claude_code.rs` — add a private `format_pre_tool_use_activity` helper and extend `extract_message` to fall back to it when the payload has no top-level `message`. Tests stay in the existing `#[cfg(test)] mod tests`.
- **Modify:** `crates/agent-switcher/src/ui.rs` — rename the snippet column header from `"Last response"` to `"Activity"` so it accurately describes both "what it's doing" and "what it's waiting for".
- **Modify:** `crates/agent-status/tests/cli.rs` — add an end-to-end test that `set working` with a real Claude Code PreToolUse payload produces a state file with a populated `message` field.
- **Modify:** `README.md` — one paragraph in the Claude Code section explaining what users will see in the switcher.
- **Modify:** `CLAUDE.md` — one line in the "Wire compatibility" / `working` paragraph explaining the new behavior.

Files left untouched on purpose: `state.rs` (no schema change), `commands.rs` (formatting and filtering are unchanged — `working` entries are still hidden from `format_status`/`format_list`), `agents/mod.rs` (no new trait method), `agents/pi_coding_agent.rs` and `agents/opencode.rs` (those agents don't have a PreToolUse-equivalent hook today; their `extract_message` keeps probing the `message` field).

---

## Task 1: Helper that turns a PreToolUse payload into a one-line activity string

**Files:**
- Modify: `crates/agent-status/src/agents/claude_code.rs`

Add a private free function in this file:

```rust
fn format_pre_tool_use_activity(tool_name: &str, tool_input: &serde_json::Value) -> String { ... }
```

It is *pure*: takes the parsed `tool_name` string and the `tool_input` JSON value, returns a `String`. It never touches `std::fs`, `std::env`, or the network. It must always return a non-empty string (the caller treats an empty string as "no message" and we don't want to confuse the two — if we can't extract a useful detail we still want `"Using <tool>"` as the visible activity).

- [ ] **Step 1: Write the failing test for Bash → "Running: <command>"**

Add this test inside the existing `#[cfg(test)] mod tests` block in `crates/agent-status/src/agents/claude_code.rs`:

```rust
#[test]
fn format_pre_tool_use_activity_bash_uses_command() {
    let input = serde_json::json!({"command": "git status", "description": "Show status"});
    assert_eq!(
        format_pre_tool_use_activity("Bash", &input),
        "Running: git status"
    );
}

#[test]
fn format_pre_tool_use_activity_bash_collapses_multiline_command() {
    let input = serde_json::json!({"command": "set -e\nmake build\nmake test"});
    // Multi-line commands collapse to the first non-empty line so the
    // snippet stays on one row of the table.
    assert_eq!(
        format_pre_tool_use_activity("Bash", &input),
        "Running: set -e"
    );
}
```

- [ ] **Step 2: Run those tests to verify they fail with "not in scope"**

Run: `cargo test -p agent-status agents::claude_code::tests::format_pre_tool_use_activity_bash -- --nocapture`
Expected: compile error / FAIL — `cannot find function format_pre_tool_use_activity` (or similar).

- [ ] **Step 3: Add the function skeleton with only the Bash arm**

Insert this *after* the `impl Agent for ClaudeCodeAgent` block, *before* the `#[cfg(test)]` block in `crates/agent-status/src/agents/claude_code.rs`:

```rust
/// Build a one-line, human-readable description of a Claude Code
/// `PreToolUse` payload's tool call. Used as the entry `message` for
/// `working` entries so the switcher can show *what* the agent is doing.
///
/// `tool_input` is the raw `tool_input` field from the hook payload — a JSON
/// object whose shape depends on `tool_name`. We probe defensively: a
/// missing or wrong-typed field falls back to a generic `"Using <tool>"`
/// string rather than panicking, since the hook payload is external input.
///
/// Always returns a non-empty string. Length capping is the UI's job
/// (`crates/agent-switcher/src/ui.rs` truncates to `MESSAGE_CAP`).
fn format_pre_tool_use_activity(tool_name: &str, tool_input: &serde_json::Value) -> String {
    match tool_name {
        "Bash" => {
            let cmd = tool_input
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let first = cmd.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            if first.is_empty() {
                "Running command".to_string()
            } else {
                format!("Running: {first}")
            }
        }
        other => format!("Using {other}"),
    }
}
```

- [ ] **Step 4: Run the two Bash tests to verify pass**

Run: `cargo test -p agent-status agents::claude_code::tests::format_pre_tool_use_activity_bash`
Expected: 2 passed.

- [ ] **Step 5: Add tests for the file-path tools (Read/Edit/Write/MultiEdit)**

Append to the same test module:

```rust
#[test]
fn format_pre_tool_use_activity_read_uses_basename() {
    let input = serde_json::json!({"file_path": "/Users/me/work/repo/src/main.rs"});
    assert_eq!(
        format_pre_tool_use_activity("Read", &input),
        "Reading src/main.rs"
    );
}

#[test]
fn format_pre_tool_use_activity_edit_uses_basename() {
    let input = serde_json::json!({"file_path": "/x/lib.rs", "old_string": "a", "new_string": "b"});
    assert_eq!(
        format_pre_tool_use_activity("Edit", &input),
        "Editing lib.rs"
    );
}

#[test]
fn format_pre_tool_use_activity_multiedit_uses_basename() {
    let input = serde_json::json!({"file_path": "/x/a/b/c.rs"});
    assert_eq!(
        format_pre_tool_use_activity("MultiEdit", &input),
        "Editing c.rs"
    );
}

#[test]
fn format_pre_tool_use_activity_write_uses_basename() {
    let input = serde_json::json!({"file_path": "/x/new.rs", "content": "fn main() {}"});
    assert_eq!(
        format_pre_tool_use_activity("Write", &input),
        "Writing new.rs"
    );
}

#[test]
fn format_pre_tool_use_activity_read_falls_back_when_path_missing() {
    let input = serde_json::json!({});
    assert_eq!(format_pre_tool_use_activity("Read", &input), "Reading file");
}
```

(The `Reading src/main.rs` expectation uses the *last two* path components when the basename is generic like `main.rs`; if you'd rather just emit `Reading main.rs` for every Read, simplify the helper and these tests in lockstep. See Step 6 for the chosen strategy.)

- [ ] **Step 6: Run the file-path tests to verify fail, then extend the helper**

Run: `cargo test -p agent-status agents::claude_code::tests::format_pre_tool_use_activity_read`
Expected: FAIL — only the Bash and `Using <tool>` arms exist.

Replace the `match tool_name` block in `format_pre_tool_use_activity` with this expanded version:

```rust
    match tool_name {
        "Bash" => {
            let cmd = tool_input
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let first = cmd.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            if first.is_empty() {
                "Running command".to_string()
            } else {
                format!("Running: {first}")
            }
        }
        "Read" => format_file_path_activity(tool_input, "Reading", "file"),
        "Edit" | "MultiEdit" => format_file_path_activity(tool_input, "Editing", "file"),
        "Write" => format_file_path_activity(tool_input, "Writing", "file"),
        other => format!("Using {other}"),
    }
```

…and add this private helper directly below `format_pre_tool_use_activity` in the same file:

```rust
/// Format a "<verb> <short-path>" activity for tools whose `tool_input`
/// carries a single `file_path`. Returns the verb plus the path's last
/// component(s); falls back to `"<verb> <fallback>"` when the field is
/// missing or empty.
///
/// We show *up to two* path components (e.g. `src/main.rs` rather than
/// `main.rs`) because generic basenames like `mod.rs` or `lib.rs` carry no
/// signal on their own. Two components is enough to disambiguate across
/// typical Rust/TS/Python module layouts without bloating the column.
fn format_file_path_activity(
    tool_input: &serde_json::Value,
    verb: &str,
    fallback_noun: &str,
) -> String {
    let path = tool_input
        .get("file_path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if path.is_empty() {
        return format!("{verb} {fallback_noun}");
    }
    let short = short_path(path);
    format!("{verb} {short}")
}

/// Return the last *two* path components of `path` joined with `/`, or the
/// whole input if it has fewer than two components. `path` is treated as a
/// POSIX-style path (`/`) — Claude Code's hooks always pass forward
/// slashes, even on Windows.
fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path
        .trim_end_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    match parts.as_slice() {
        [] => path.to_string(),
        [only] => (*only).to_string(),
        rest => {
            let n = rest.len();
            format!("{}/{}", rest[n - 2], rest[n - 1])
        }
    }
}
```

- [ ] **Step 7: Run all claude_code tests to confirm pass**

Run: `cargo test -p agent-status agents::claude_code::tests`
Expected: all tests (the original ones + the 7 new ones from Steps 1, 5) pass.

- [ ] **Step 8: Add tests for the search and meta tools (Grep, Glob, Task, WebFetch, WebSearch, TodoWrite)**

Append to the same test module:

```rust
#[test]
fn format_pre_tool_use_activity_grep_uses_pattern() {
    let input = serde_json::json!({"pattern": "fn main", "path": "src"});
    assert_eq!(
        format_pre_tool_use_activity("Grep", &input),
        "Searching: fn main"
    );
}

#[test]
fn format_pre_tool_use_activity_glob_uses_pattern() {
    let input = serde_json::json!({"pattern": "**/*.rs"});
    assert_eq!(
        format_pre_tool_use_activity("Glob", &input),
        "Globbing: **/*.rs"
    );
}

#[test]
fn format_pre_tool_use_activity_task_uses_description() {
    let input = serde_json::json!({
        "description": "Audit auth middleware",
        "subagent_type": "general-purpose",
    });
    assert_eq!(
        format_pre_tool_use_activity("Task", &input),
        "Subagent: Audit auth middleware"
    );
}

#[test]
fn format_pre_tool_use_activity_task_falls_back_when_description_missing() {
    let input = serde_json::json!({"subagent_type": "general-purpose"});
    assert_eq!(
        format_pre_tool_use_activity("Task", &input),
        "Running subagent"
    );
}

#[test]
fn format_pre_tool_use_activity_webfetch_uses_url() {
    let input = serde_json::json!({"url": "https://example.com/docs", "prompt": "summarize"});
    assert_eq!(
        format_pre_tool_use_activity("WebFetch", &input),
        "Fetching https://example.com/docs"
    );
}

#[test]
fn format_pre_tool_use_activity_websearch_uses_query() {
    let input = serde_json::json!({"query": "ratatui table widget"});
    assert_eq!(
        format_pre_tool_use_activity("WebSearch", &input),
        "Searching web: ratatui table widget"
    );
}

#[test]
fn format_pre_tool_use_activity_todowrite_is_generic() {
    let input = serde_json::json!({"todos": []});
    assert_eq!(
        format_pre_tool_use_activity("TodoWrite", &input),
        "Updating tasks"
    );
}

#[test]
fn format_pre_tool_use_activity_unknown_tool_falls_back() {
    let input = serde_json::json!({});
    assert_eq!(
        format_pre_tool_use_activity("Frobnicator", &input),
        "Using Frobnicator"
    );
}

#[test]
fn format_pre_tool_use_activity_handles_missing_input_object() {
    // `tool_input` is sometimes null, never an object — defend against it.
    let input = serde_json::Value::Null;
    assert_eq!(
        format_pre_tool_use_activity("Bash", &input),
        "Running command"
    );
    assert_eq!(
        format_pre_tool_use_activity("Read", &input),
        "Reading file"
    );
    assert_eq!(
        format_pre_tool_use_activity("Grep", &input),
        "Searching"
    );
}
```

- [ ] **Step 9: Run the new tests to verify fail**

Run: `cargo test -p agent-status agents::claude_code::tests::format_pre_tool_use_activity`
Expected: the new tests (Grep/Glob/Task/WebFetch/WebSearch/TodoWrite/unknown/null) FAIL; only Bash / file-path tests pass.

- [ ] **Step 10: Add the remaining match arms to the helper**

Replace `format_pre_tool_use_activity` with the full implementation:

```rust
fn format_pre_tool_use_activity(tool_name: &str, tool_input: &serde_json::Value) -> String {
    match tool_name {
        "Bash" => {
            let cmd = tool_input
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let first = cmd.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            if first.is_empty() {
                "Running command".to_string()
            } else {
                format!("Running: {first}")
            }
        }
        "Read" => format_file_path_activity(tool_input, "Reading", "file"),
        "Edit" | "MultiEdit" => format_file_path_activity(tool_input, "Editing", "file"),
        "Write" => format_file_path_activity(tool_input, "Writing", "file"),
        "Grep" => format_field_activity(tool_input, "pattern", "Searching", "Searching"),
        "Glob" => format_field_activity(tool_input, "pattern", "Globbing", "Globbing files"),
        "Task" => format_field_activity(tool_input, "description", "Subagent", "Running subagent"),
        "WebFetch" => format_field_activity(tool_input, "url", "Fetching", "Fetching URL"),
        "WebSearch" => format_field_activity(tool_input, "query", "Searching web", "Searching web"),
        "TodoWrite" => "Updating tasks".to_string(),
        "NotebookEdit" => "Editing notebook".to_string(),
        "ExitPlanMode" => "Exiting plan mode".to_string(),
        other => format!("Using {other}"),
    }
}

/// Format a "<verb>: <field-value>" activity for tools whose `tool_input`
/// carries a single string field. Falls back to `<empty_fallback>` (no
/// colon) when the field is missing or empty — that's why callers pass the
/// raw verb (`"Searching"`) and the fallback (`"Searching"` or
/// `"Searching web"`) separately rather than computing one from the other.
fn format_field_activity(
    tool_input: &serde_json::Value,
    field: &str,
    verb: &str,
    empty_fallback: &str,
) -> String {
    let value = tool_input
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if value.is_empty() {
        empty_fallback.to_string()
    } else {
        format!("{verb}: {value}")
    }
}
```

- [ ] **Step 11: Run all claude_code tests to confirm pass**

Run: `cargo test -p agent-status agents::claude_code`
Expected: every test in the module passes (original + 17 new ones).

- [ ] **Step 12: Run the workspace clippy gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: clean exit (no warnings — the workspace lints deny `clippy::all` and warn on `clippy::pedantic`).

If clippy complains about `unused` private helpers, that's expected: nothing calls `format_pre_tool_use_activity` from `extract_message` yet — Task 2 wires it up. Use `#[allow(dead_code)]` on the helper as a *temporary* annotation if needed; remove it in Task 2.

- [ ] **Step 13: Commit**

```bash
git add crates/agent-status/src/agents/claude_code.rs
git commit -m "feat(agent-status): add format_pre_tool_use_activity helper

Pure function that maps a Claude Code PreToolUse payload's tool_name and
tool_input to a one-line human-readable description (\"Running: git status\",
\"Reading src/main.rs\", \"Searching: fn main\", ...). Used in the next
commit to populate AttentionEntry.message for working entries so the
switcher can show what the agent is currently doing."
```

---

## Task 2: Wire the helper into `ClaudeCodeAgent::extract_message`

**Files:**
- Modify: `crates/agent-status/src/agents/claude_code.rs`

`extract_message` currently probes only the top-level `message` field. Extend it: if `message` is absent or empty, try `tool_name` + `tool_input` and format via `format_pre_tool_use_activity`. Order matters — Notification payloads can carry both `message` *and* (rarely) tool fields; we always prefer the explicit `message`.

- [ ] **Step 1: Write the failing test for PreToolUse → activity string**

Append to the existing test module in `crates/agent-status/src/agents/claude_code.rs`:

```rust
#[test]
fn extract_message_returns_activity_for_pre_tool_use_payload() {
    let json = r#"{
        "session_id": "abc-123",
        "transcript_path": "/x/y.jsonl",
        "tool_name": "Bash",
        "tool_input": {"command": "git status", "description": "Show status"}
    }"#;
    assert_eq!(
        ClaudeCodeAgent.extract_message(json).as_deref(),
        Some("Running: git status"),
    );
}

#[test]
fn extract_message_returns_activity_for_read_pre_tool_use_payload() {
    let json = r#"{
        "session_id": "abc",
        "tool_name": "Read",
        "tool_input": {"file_path": "/repo/src/lib.rs"}
    }"#;
    assert_eq!(
        ClaudeCodeAgent.extract_message(json).as_deref(),
        Some("Reading src/lib.rs"),
    );
}

#[test]
fn extract_message_prefers_message_field_over_tool_fields() {
    // If both are present (defensive — shouldn't happen in practice), the
    // explicit message wins. Notification payloads sometimes carry extra
    // fields and we don't want them to override the user-facing message.
    let json = r#"{
        "session_id": "abc",
        "message": "Permission required",
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf /"}
    }"#;
    assert_eq!(
        ClaudeCodeAgent.extract_message(json).as_deref(),
        Some("Permission required"),
    );
}

#[test]
fn extract_message_returns_none_when_neither_message_nor_tool_name_present() {
    // UserPromptSubmit, Stop, SessionStart, SessionEnd payloads don't have
    // either field — we must keep returning None so the entry stores no
    // message (the spinner alone communicates "working" in that case).
    let json = r#"{"session_id":"abc","prompt":"hello"}"#;
    assert!(ClaudeCodeAgent.extract_message(json).is_none());
}

#[test]
fn extract_message_returns_none_when_tool_name_is_empty() {
    let json = r#"{"session_id":"abc","tool_name":"","tool_input":{}}"#;
    assert!(ClaudeCodeAgent.extract_message(json).is_none());
}
```

- [ ] **Step 2: Run the new tests to verify fail**

Run: `cargo test -p agent-status agents::claude_code::tests::extract_message_returns_activity`
Expected: FAIL — `extract_message` currently returns `None` for any payload without a top-level `message` field.

- [ ] **Step 3: Update `extract_message` to fall back to the activity formatter**

Replace the existing `extract_message` body in `crates/agent-status/src/agents/claude_code.rs`:

```rust
    fn extract_message(&self, stdin_json: &str) -> Option<String> {
        let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;

        // Prefer an explicit `message` field (Notification payloads) — it's
        // the agent's user-facing text and always more informative than a
        // derived activity description.
        if let Some(m) = v.get("message").and_then(serde_json::Value::as_str) {
            if !m.is_empty() {
                return Some(m.to_string());
            }
        }

        // Fall back to PreToolUse tool fields: synthesize an activity
        // string. `tool_input` is allowed to be missing / null / wrong-
        // typed — `format_pre_tool_use_activity` defends against that.
        let tool_name = v.get("tool_name").and_then(serde_json::Value::as_str)?;
        if tool_name.is_empty() {
            return None;
        }
        let tool_input = v.get("tool_input").cloned().unwrap_or(serde_json::Value::Null);
        Some(format_pre_tool_use_activity(tool_name, &tool_input))
    }
```

If you added `#[allow(dead_code)]` to the helper in Task 1, *remove it now* — the helper is called.

- [ ] **Step 4: Run all extract_message tests to verify pass**

Run: `cargo test -p agent-status agents::claude_code::tests::extract_message`
Expected: all `extract_message_*` tests pass (the existing 6 + 5 new ones from Step 1).

- [ ] **Step 5: Run the full test suite**

Run: `cargo test`
Expected: every test in the workspace passes. In particular, the existing `entry_message_field_roundtrips_when_set` (state.rs) and the `format_list_*` tests in commands.rs should be unaffected, and `working_status_is_recorded_but_hidden_from_indicator_and_list` (tests/cli.rs) still passes because `working` entries remain filtered from `format_status`/`format_list`.

- [ ] **Step 6: Run the workspace clippy gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: clean exit.

- [ ] **Step 7: Commit**

```bash
git add crates/agent-status/src/agents/claude_code.rs
git commit -m "feat(agent-status): populate message for PreToolUse working entries

extract_message now falls back to a derived activity string (via
format_pre_tool_use_activity) when the hook payload has no top-level
message but does carry tool_name/tool_input. Notification payloads still
win — the explicit message is always more informative than a synthesized
one. Net effect: working entries in agent-switcher now show what the
agent is currently doing (\"Reading src/main.rs\", \"Running: git status\",
...) instead of just a spinner."
```

---

## Task 3: Rename the switcher's snippet column from "Last response" to "Activity"

**Files:**
- Modify: `crates/agent-switcher/src/ui.rs`

The column now carries two distinct kinds of content (the agent's notification text for `notify` rows, the active tool/operation for `working` rows). "Last response" is misleading for working rows. "Activity" covers both.

- [ ] **Step 1: Update the header cell**

In `crates/agent-switcher/src/ui.rs`, locate this block inside `sessions_table`:

```rust
        .header(
            Row::new(vec!["", "Session", "Agent", "Last response"]).style(
```

Replace `"Last response"` with `"Activity"`:

```rust
        .header(
            Row::new(vec!["", "Session", "Agent", "Activity"]).style(
```

That is the only change in this file.

- [ ] **Step 2: Run the switcher tests**

Run: `cargo test -p agent-switcher`
Expected: all tests pass. The ui.rs tests cover only `one_line` and `display_session`; no test asserts on the header text, so no test should break or need updating.

- [ ] **Step 3: Run the workspace clippy gate**

Run: `cargo clippy --all-targets --all-features --locked -- -D warnings`
Expected: clean exit.

- [ ] **Step 4: Commit**

```bash
git add crates/agent-switcher/src/ui.rs
git commit -m "feat(agent-switcher): rename 'Last response' column to 'Activity'

The column now shows both the active tool/operation (for working entries,
populated in the previous commit) and the agent's notification message
(for notify entries). 'Activity' covers both; 'Last response' was
misleading for working rows."
```

---

## Task 4: End-to-end test pinning PreToolUse → state.message → list/status invariants

**Files:**
- Modify: `crates/agent-status/tests/cli.rs`

We need an integration test (process-level, not unit-level) that confirms three things at once:
1. `set working` with a Claude-Code-shaped PreToolUse payload writes a state file with a populated `message`.
2. The same state file is *still* hidden from `status` and `list` (working entries don't surface there — that invariant is sacred).
3. The message round-trips: the JSON on disk parses back to a `message` containing the formatted activity string.

This guards the wire-format and the cross-module integration in one shot.

- [ ] **Step 1: Write the failing integration test**

Append to `crates/agent-status/tests/cli.rs`:

```rust
#[test]
fn working_entry_with_pre_tool_use_payload_records_activity_message() {
    let tmp = TempDir::new().unwrap();
    let state_dir = tmp.path().join("agent-status");

    // Claude Code PreToolUse payload shape — what the real hook pipes.
    let payload = r#"{
        "session_id": "sess-work",
        "transcript_path": "/x/y.jsonl",
        "tool_name": "Read",
        "tool_input": {"file_path": "/repo/src/lib.rs"}
    }"#;
    let (_, stderr, code) = run(&state_dir, &["set", "working"], Some(payload));
    assert_eq!(code, 0, "stderr: {stderr}");

    // State file should exist.
    let state_file = state_dir.join("sess-work");
    assert!(state_file.exists(), "expected state file at {state_file:?}");

    // Parse it back and check the message field carries the activity.
    let raw = std::fs::read_to_string(&state_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["event"], "working");
    assert_eq!(
        parsed["message"].as_str(),
        Some("Reading src/lib.rs"),
        "expected derived activity in message; got: {raw}",
    );

    // Working entries must STILL be hidden from status and list.
    let (stdout, _, _) = run(&state_dir, &["status"], None);
    assert_eq!(stdout, "", "working must not appear in tmux status");
    let (stdout, _, _) = run(&state_dir, &["list"], None);
    assert_eq!(stdout, "", "working must not appear in switcher list");
}
```

- [ ] **Step 2: Run the new test to verify it would have failed without Tasks 1+2**

Run: `cargo test -p agent-status --test cli working_entry_with_pre_tool_use_payload_records_activity_message`
Expected: PASS (because Tasks 1+2 are merged). If it fails, the regression is real — re-check the `extract_message` ordering: `message` field wins, but when absent we must reach into `tool_name`/`tool_input`.

- [ ] **Step 3: Run the full workspace test suite + clippy**

```bash
cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Expected: green.

- [ ] **Step 4: Commit**

```bash
git add crates/agent-status/tests/cli.rs
git commit -m "test(agent-status): pin PreToolUse activity end-to-end

Integration test that drives a real Claude-Code-shaped PreToolUse payload
through the binary and checks (a) the on-disk state file's message field
carries the formatted activity, and (b) the working entry stays hidden
from status/list. Locks in the cross-module invariant that the snippet
column in agent-switcher is the only surface that reveals working state."
```

---

## Task 5: Documentation — README and CLAUDE.md

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

Both files mention the `working` event already. We just need to add a sentence noting that the switcher now shows the *active tool* for those rows.

- [ ] **Step 1: Locate the README section to update**

Run: `grep -n "working\|switcher\|status-right" README.md`

You want the paragraph that describes the switcher TUI's columns. If the README only describes `status-right` and the install instructions, add a fresh one-sentence paragraph under the Claude Code hook description that says:

> While Claude Code is working, the switcher's "Activity" column shows the active tool — e.g. `Reading src/main.rs`, `Running: git status`, `Searching: fn main`. When Claude Code is waiting on you, the same column shows the notification message (e.g. `Claude needs your permission to use Bash`).

Place that paragraph immediately after the bullet list of Claude Code hooks. Keep the prose tight — one paragraph, no headings, no emoji.

- [ ] **Step 2: Update CLAUDE.md's "working" paragraph**

Open `CLAUDE.md` and find this paragraph:

```
The `event` field accepts a third value `"working"` in addition to `"notify"`
and `"done"`. The Claude Code extension's `UserPromptSubmit` and `PreToolUse`
hooks emit `set working` so an in-flight session is recorded in the state
directory. `format_status` and `format_list` filter `working` entries out, so
the tmux indicator and the `list` TSV output are unchanged. `agent-switcher`
is the only consumer that surfaces working entries (with a spinner). pi and
opencode do not yet emit `working`; their hook semantics are unchanged.
```

Append one sentence to the end of that paragraph (still inside the same `<paragraph>`):

> For PreToolUse, the hook payload's `tool_name`/`tool_input` are turned into a one-line activity string (`format_pre_tool_use_activity` in `agents/claude_code.rs`) and stored as the entry's `message`, so the switcher's Activity column shows what the agent is doing in real time.

- [ ] **Step 3: Sanity-check the docs build/render**

Open both files in a markdown viewer or `glow README.md` / `glow CLAUDE.md` and skim — the paragraphs should flow naturally and not introduce stray formatting.

- [ ] **Step 4: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: document switcher activity column behavior

Note in README.md (user-facing) and CLAUDE.md (contributor-facing) that
the switcher's snippet column shows the active tool for working entries
(\"Reading src/main.rs\", \"Running: git status\") and the notification
message for waiting entries (\"Permission required to use Bash\")."
```

---

## Self-Review Checklist (already applied)

- **Spec coverage:**
  - "Show updates about what the agent is doing" → Tasks 1+2 derive a one-liner from PreToolUse `tool_name`+`tool_input` and store it as the entry's `message`. Covered.
  - "Show what the agent is waiting for" → Already wired (Notification's `message` field). Verified in Task 2 Step 1 with the `extract_message_prefers_message_field_over_tool_fields` test and in the existing `extract_message_returns_string_when_present` test. Covered without code change.
- **Placeholder scan:** No "TBD", no "Add validation as appropriate", no "Similar to Task N". Every code block is concrete and copy-paste runnable.
- **Type consistency:** `format_pre_tool_use_activity(tool_name: &str, tool_input: &serde_json::Value) -> String` — used identically in Task 1 Step 10 and Task 2 Step 3. `format_file_path_activity` and `format_field_activity` signatures match between their definitions and their callers in Task 1 Step 10. Test assertion strings (`"Reading src/main.rs"`, `"Running: git status"`) match the implementation arms.
