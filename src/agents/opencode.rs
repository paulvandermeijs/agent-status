use crate::agents::Agent;

/// opencode ([opencode.ai](https://opencode.ai)).
///
/// Reads `session_id` from the JSON piped in by the bundled opencode plugin at
/// `extensions/opencode.ts`, which fires on opencode's `session.idle`,
/// `permission.updated`, `session.created`, and `session.deleted` events.
pub struct OpencodeAgent;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_opencode() {
        assert_eq!(OpencodeAgent.name(), "opencode");
    }

    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(
            OpencodeAgent.extract_session_id(json).as_deref(),
            Some("abc-123")
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_missing_field() {
        assert_eq!(
            OpencodeAgent.extract_session_id(r#"{"other":1}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(
            OpencodeAgent.extract_session_id(r#"{"session_id":""}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(OpencodeAgent.extract_session_id("not json"), None);
    }

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
}
