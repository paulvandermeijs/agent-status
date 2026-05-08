use crate::state::AttentionEntry;
use std::path::Path;

/// Extract `session_id` from a JSON object.
///
/// Returns `None` for any of: invalid JSON, missing `session_id` field, non-string value,
/// or empty string. The empty-string case is treated as missing because Claude Code hooks
/// occasionally fire with no session id and we want to silently no-op rather than fail.
pub fn extract_session_id(stdin_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(stdin_json).ok()?;
    let id = v.get("session_id")?.as_str()?;
    if id.is_empty() { None } else { Some(id.to_string()) }
}

/// Construct an [`AttentionEntry`] from raw inputs.
///
/// `project` is derived as the basename of `cwd`. When `cwd` has no basename (e.g. `/`
/// or empty string), `project` falls back to `cwd` itself.
pub fn build_entry(
    agent: &str,
    event: &str,
    cwd: &str,
    tmux_pane: &str,
    ts: u64,
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
    }
}

/// Format the tmux `status-right` line for the given entries.
///
/// Returns `None` when there are no entries so the caller can omit the line entirely.
/// One entry shows the project name; multiple entries show a count. The output is
/// plain text — styling is left to the tmux config so users can pick their own colors.
pub fn format_status(entries: &[(String, AttentionEntry)]) -> Option<String> {
    match entries.len() {
        0 => None,
        1 => Some(format!("[!] {}", entries[0].1.project)),
        n => Some(format!("[!] {n} projects waiting")),
    }
}

/// Format the popup picker output: tab-separated `pane\tproject\tevent\n` per entry.
///
/// Field order matches the bash version's `jq -r '[.tmux_pane, .project, .event] | @tsv'`.
/// The trailing newline is included on every line, including the last.
pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    let mut out = String::new();
    for (_, e) in entries {
        out.push_str(&e.tmux_pane);
        out.push('\t');
        out.push_str(&e.project);
        out.push('\t');
        out.push_str(&e.event);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: pane.into(),
            ts: 1,
        }
    }

    #[test]
    fn extract_session_id_returns_id() {
        let json = r#"{"session_id":"abc-123","other":"stuff"}"#;
        assert_eq!(extract_session_id(json).as_deref(), Some("abc-123"));
    }

    #[test]
    fn extract_session_id_returns_none_for_missing() {
        assert_eq!(extract_session_id(r#"{"other":1}"#), None);
    }

    #[test]
    fn extract_session_id_returns_none_for_empty_string() {
        assert_eq!(extract_session_id(r#"{"session_id":""}"#), None);
    }

    #[test]
    fn extract_session_id_returns_none_for_invalid_json() {
        assert_eq!(extract_session_id("not json"), None);
    }

    #[test]
    fn build_entry_uses_basename_of_cwd_as_project() {
        let e = build_entry("claude-code", "notify", "/Users/me/work/claude-status", "%5", 42);
        assert_eq!(e.agent, "claude-code");
        assert_eq!(e.project, "claude-status");
        assert_eq!(e.cwd, "/Users/me/work/claude-status");
        assert_eq!(e.event, "notify");
        assert_eq!(e.tmux_pane, "%5");
        assert_eq!(e.ts, 42);
    }

    #[test]
    fn build_entry_falls_back_to_cwd_when_no_basename() {
        let e = build_entry("claude-code", "notify", "/", "", 0);
        assert_eq!(e.project, "/");
        assert_eq!(e.agent, "claude-code");
    }

    #[test]
    fn format_status_empty_returns_none() {
        assert_eq!(format_status(&[]), None);
    }

    #[test]
    fn format_status_single_entry_shows_project_name() {
        let entries = vec![("s1".into(), entry("alpha", "%1", "notify"))];
        assert_eq!(format_status(&entries).as_deref(), Some("[!] alpha"));
    }

    #[test]
    fn format_status_multiple_entries_shows_count() {
        let entries = vec![
            ("s1".into(), entry("a", "%1", "notify")),
            ("s2".into(), entry("b", "%2", "done")),
            ("s3".into(), entry("c", "%3", "done")),
        ];
        assert_eq!(
            format_status(&entries).as_deref(),
            Some("[!] 3 projects waiting")
        );
    }

    #[test]
    fn format_list_emits_tab_separated_pane_project_event() {
        let entries = vec![
            ("s1".into(), entry("alpha", "%1", "notify")),
            ("s2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        assert_eq!(out, "%1\talpha\tnotify\n%2\tbeta\tdone\n");
    }
}
