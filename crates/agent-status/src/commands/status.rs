use super::needs_attention;
use crate::state::AttentionEntry;

/// Format the tmux `status-right` line for the given entries.
///
/// Returns `None` when there are no entries so the caller can omit the line entirely.
/// One entry shows the project name; multiple entries show a count. The output is
/// plain text — styling is left to the tmux config so users can pick their own colors.
#[must_use]
pub fn format_status(entries: &[(String, AttentionEntry)]) -> Option<String> {
    let waiting: Vec<&AttentionEntry> = entries
        .iter()
        .filter(|(_, e)| needs_attention(&e.event))
        .map(|(_, e)| e)
        .collect();
    match waiting.len() {
        0 => None,
        1 => Some(format!("[!] {}", waiting[0].project)),
        n => Some(format!("[!] {n} projects waiting")),
    }
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
            message: None,
            pid: None,
        }
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
    fn format_status_ignores_working_entries() {
        let e = entry("alpha", "%1", "working");
        assert_eq!(format_status(&[("s1".into(), e)]), None);
    }

    #[test]
    fn format_status_counts_only_non_working_entries() {
        let working = entry("alpha", "%1", "working");
        let waiting = entry("beta", "%2", "notify");
        let entries = vec![
            ("s1".into(), working),
            ("s2".into(), waiting),
        ];
        assert_eq!(format_status(&entries).as_deref(), Some("[!] beta"));
    }

    #[test]
    fn format_status_ignores_idle_entries() {
        let e = entry("alpha", "%1", "idle");
        assert_eq!(format_status(&[("s1".into(), e)]), None);
    }

    #[test]
    fn format_status_counts_only_attention_needing_entries() {
        let idle = entry("alpha", "%1", "idle");
        let working = entry("beta", "%2", "working");
        let waiting = entry("gamma", "%3", "notify");
        let entries = vec![
            ("s1".into(), idle),
            ("s2".into(), working),
            ("s3".into(), waiting),
        ];
        assert_eq!(format_status(&entries).as_deref(), Some("[!] gamma"));
    }
}
