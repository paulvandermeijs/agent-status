use crate::agents::Agent;
use crate::commands::extract_session_id;

/// Claude Code (`claude.ai/code`).
///
/// Reads `session_id` from the hook event payload that Claude Code pipes to stdin.
pub struct ClaudeCodeAgent;

impl Agent for ClaudeCodeAgent {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn extract_session_id(&self, stdin_json: &str) -> Option<String> {
        extract_session_id(stdin_json)
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
}
