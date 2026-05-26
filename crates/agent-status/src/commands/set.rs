use crate::state::{AttentionEntry, Event};
use std::path::Path;

/// Construct an [`AttentionEntry`] from raw inputs.
///
/// `project` is derived as the basename of `cwd`. When `cwd` has no basename (e.g. `/`
/// or empty string), `project` falls back to `cwd` itself. `event` is parsed via
/// [`Event::from`]: known names (`notify`/`done`/`working`/`idle`) become named
/// variants, anything else is preserved as [`Event::Unknown`]. `message` is the
/// agent's last-response text when the hook payload supplies one; pass `None`
/// otherwise. `tmux_pane` and `tmux_session` carry the enclosing tmux pane id
/// and session name respectively; pass `None` for either when the hook fires
/// outside tmux (or capture failed).
#[allow(clippy::too_many_arguments)]
pub fn build_entry(
    agent: &str,
    event: &str,
    cwd: &str,
    tmux_pane: Option<&str>,
    ts: u64,
    message: Option<&str>,
    pid: Option<u32>,
    tmux_session: Option<&str>,
) -> AttentionEntry {
    let project = Path::new(cwd)
        .file_name()
        .map_or_else(|| cwd.to_string(), |s| s.to_string_lossy().into_owned());
    AttentionEntry {
        agent: agent.to_string(),
        project,
        cwd: cwd.to_string(),
        event: Event::from(event),
        tmux_pane: tmux_pane.map(str::to_string),
        ts,
        message: message.map(str::to_string),
        pid,
        tmux_session: tmux_session.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("claude-code", "notify", "/Users/me/work/claude-status", Some("%5"), 42, None, None, None);
        assert_eq!(e.agent, "claude-code");
        assert_eq!(e.project, "claude-status");
        assert_eq!(e.cwd, "/Users/me/work/claude-status");
        assert_eq!(e.event, Event::Notify);
        assert_eq!(e.tmux_pane.as_deref(), Some("%5"));
        assert_eq!(e.ts, 42);
        assert!(e.message.is_none());
    }

    #[test]
    fn build_entry_parses_unknown_event_into_unknown_variant() {
        let e = build_entry("claude-code", "future-event", "/x", None, 0, None, None, None);
        assert_eq!(e.event, Event::Unknown("future-event".to_string()));
    }

    #[test]
    fn build_entry_falls_back_to_cwd_when_no_basename() {
        let e = build_entry("claude-code", "notify", "/", None, 0, None, None, None);
        assert_eq!(e.project, "/");
        assert_eq!(e.agent, "claude-code");
    }

    #[test]
    fn build_entry_stores_message_when_some() {
        let e = build_entry(
            "claude-code",
            "notify",
            "/Users/me/work/app",
            Some("%5"),
            42,
            Some("Permission required"),
            None,
            None,
        );
        assert_eq!(e.message.as_deref(), Some("Permission required"));
    }

    #[test]
    fn build_entry_leaves_message_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", Some("%1"), 1, None, None, None);
        assert!(e.message.is_none());
    }

    #[test]
    fn build_entry_stores_pid_when_some() {
        let e = build_entry(
            "claude-code", "notify", "/Users/me/work/app", Some("%5"), 42,
            Some("Permission required"), Some(12345), None,
        );
        assert_eq!(e.pid, Some(12345));
    }

    #[test]
    fn build_entry_leaves_pid_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", Some("%1"), 1, None, None, None);
        assert!(e.pid.is_none());
    }

    #[test]
    fn build_entry_stores_tmux_session_when_some() {
        let e = build_entry(
            "claude-code", "notify", "/x/p", Some("%5"), 42,
            None, None, Some("main-session"),
        );
        assert_eq!(e.tmux_session.as_deref(), Some("main-session"));
    }

    #[test]
    fn build_entry_leaves_tmux_session_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", Some("%1"), 1, None, None, None);
        assert!(e.tmux_session.is_none());
    }

    #[test]
    fn build_entry_leaves_tmux_pane_none_when_none() {
        let e = build_entry("claude-code", "done", "/x/p", None, 1, None, None, None);
        assert!(e.tmux_pane.is_none());
    }
}
