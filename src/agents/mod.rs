pub mod claude_code;
pub mod opencode;
pub mod pi_coding_agent;

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
    #[allow(dead_code)]
    fn extract_message(&self, _stdin_json: &str) -> Option<String> {
        None
    }
}

/// Resolve an agent by its `--agent` flag value.
pub fn by_name(name: &str) -> Option<Box<dyn Agent>> {
    match name {
        "claude-code" => Some(Box::new(claude_code::ClaudeCodeAgent)),
        "opencode" => Some(Box::new(opencode::OpencodeAgent)),
        "pi-coding-agent" => Some(Box::new(pi_coding_agent::PiCodingAgent)),
        _ => None,
    }
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

    #[test]
    fn by_name_resolves_pi_coding_agent() {
        let agent = by_name("pi-coding-agent").expect("pi-coding-agent is a registered agent");
        assert_eq!(agent.name(), "pi-coding-agent");
    }

    #[test]
    fn by_name_resolves_opencode() {
        let agent = by_name("opencode").expect("opencode is a registered agent");
        assert_eq!(agent.name(), "opencode");
    }

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
}
