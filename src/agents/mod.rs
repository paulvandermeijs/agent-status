pub mod claude_code;

/// An agent implementation: knows how to extract a session ID from the JSON payload that
/// agent's hook delivers on stdin.
pub trait Agent {
    /// Stable, lowercase, hyphenated identifier (e.g. `"claude-code"`). Used for the
    /// `--agent` CLI flag and the `agent` field on persisted entries.
    fn name(&self) -> &'static str;

    /// Extract the session ID from the agent's hook event JSON. Returns `None` for
    /// invalid JSON, missing field, non-string value, or empty string.
    fn extract_session_id(&self, stdin_json: &str) -> Option<String>;
}
