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
        let tool_input = v
            .get("tool_input")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        Some(format_pre_tool_use_activity(tool_name, &tool_input))
    }
}

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
        "Read" => format_file_path_activity(tool_input, "Reading", "file"),
        "Edit" | "MultiEdit" => format_file_path_activity(tool_input, "Editing", "file"),
        "Write" => format_file_path_activity(tool_input, "Writing", "file"),
        "Grep" => format_field_activity(tool_input, "pattern", "Searching", "Searching"),
        "Glob" => format_field_activity(tool_input, "pattern", "Globbing", "Globbing files"),
        "Task" => format_field_activity(tool_input, "description", "Subagent", "Running subagent"),
        "WebFetch" => {
            // URLs are already self-descriptive, so we use a space separator
            // rather than the "verb: value" form the other field helpers use.
            let url = tool_input
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if url.is_empty() {
                "Fetching URL".to_string()
            } else {
                format!("Fetching {url}")
            }
        }
        "WebSearch" => format_field_activity(tool_input, "query", "Searching web", "Searching web"),
        "TodoWrite" => "Updating tasks".to_string(),
        "NotebookEdit" => "Editing notebook".to_string(),
        "ExitPlanMode" => "Exiting plan mode".to_string(),
        other => format!("Using {other}"),
    }
}

/// Format a "<verb> <short-path>" activity for tools whose `tool_input`
/// carries a single `file_path`. Returns the verb plus a short display
/// form of the path; falls back to `"<verb> <fallback>"` when the field
/// is missing or empty.
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

/// Format a "<verb>: <field-value>" activity for tools whose `tool_input`
/// carries a single string field. Falls back to `<empty_fallback>` (no
/// colon) when the field is missing or empty.
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

/// Return a short display form of `path`: the basename, except for
/// generic basenames (`main.rs`, `mod.rs`, `lib.rs`, `index.*`) where the
/// parent directory is prepended so the result still identifies the file.
/// The parent is only prepended when the path has at least three
/// components (so `/x/lib.rs` stays `lib.rs` — the lone parent dir adds
/// no useful context).
///
/// `path` is treated as a POSIX-style path (`/`) — Claude Code's hooks
/// always pass forward slashes.
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
            let base = rest[n - 1];
            if n >= 3 && is_generic_basename(base) {
                format!("{}/{}", rest[n - 2], base)
            } else {
                base.to_string()
            }
        }
    }
}

fn is_generic_basename(name: &str) -> bool {
    matches!(name, "main.rs" | "mod.rs" | "lib.rs") || name.starts_with("index.")
}

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
        // /repo/src/lib.rs: 3 components, generic basename → "src/lib.rs"
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
        assert_eq!(format_pre_tool_use_activity("Bash", &input), "Running command");
        assert_eq!(format_pre_tool_use_activity("Read", &input), "Reading file");
        assert_eq!(format_pre_tool_use_activity("Grep", &input), "Searching");
    }
}
