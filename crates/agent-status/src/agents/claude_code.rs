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
        let m = v.get("message")?.as_str()?;
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    }
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
}
