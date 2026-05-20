use crate::state::AttentionEntry;
use std::path::Path;

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
    pid: Option<u32>,
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
        pid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("claude-code", "notify", "/Users/me/work/claude-status", "%5", 42, None, None);
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
        let e = build_entry("claude-code", "notify", "/", "", 0, None, None);
        assert_eq!(e.project, "/");
        assert_eq!(e.agent, "claude-code");
    }

    #[test]
    fn build_entry_stores_message_when_some() {
        let e = build_entry(
            "claude-code",
            "notify",
            "/Users/me/work/app",
            "%5",
            42,
            Some("Permission required"),
            None,
        );
        assert_eq!(e.message.as_deref(), Some("Permission required"));
    }

    #[test]
    fn build_entry_leaves_message_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", "%1", 1, None, None);
        assert!(e.message.is_none());
    }

    #[test]
    fn build_entry_stores_pid_when_some() {
        let e = build_entry(
            "claude-code", "notify", "/Users/me/work/app", "%5", 42,
            Some("Permission required"), Some(12345),
        );
        assert_eq!(e.pid, Some(12345));
    }

    #[test]
    fn build_entry_leaves_pid_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", "%1", 1, None, None);
        assert!(e.pid.is_none());
    }
}
