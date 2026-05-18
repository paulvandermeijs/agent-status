use crate::agents::Agent;

/// pi-coding-agent ([pi.dev](https://pi.dev)).
///
/// Reads `session_id` from the JSON piped in by the bundled pi extension at
/// `extensions/pi-coding-agent.ts`, which fires on pi's `before_agent_start`,
/// `agent_end`, `session_start`, and `session_shutdown` events.
pub struct PiCodingAgent;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_pi_coding_agent() {
        assert_eq!(PiCodingAgent.name(), "pi-coding-agent");
    }

    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(
            PiCodingAgent.extract_session_id(json).as_deref(),
            Some("abc-123")
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_missing_field() {
        assert_eq!(
            PiCodingAgent.extract_session_id(r#"{"other":1}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(
            PiCodingAgent.extract_session_id(r#"{"session_id":""}"#),
            None
        );
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(PiCodingAgent.extract_session_id("not json"), None);
    }

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
}
