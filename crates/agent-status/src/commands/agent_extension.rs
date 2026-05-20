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
