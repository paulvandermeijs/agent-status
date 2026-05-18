//! Pure filter logic for the switcher's list. Lives outside `app.rs` so it can
//! be unit-tested without touching `AttentionEntry`/`StateStore`.

/// Subset of [`agent_status::AttentionEntry`] fields the filter cares about.
/// Borrowing-only so callers don't pay for clones during filter evaluation.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct FilterRow<'a> {
    pub session_id: &'a str,
    pub project: &'a str,
    pub agent: &'a str,
    pub message: Option<&'a str>,
}

/// Return `true` if `row` should be visible given the user's filter text.
///
/// Matching is case-insensitive substring across `project`, `agent`, `message`,
/// and `session_id`. An empty filter matches everything.
#[must_use]
#[allow(dead_code)]
pub fn matches(row: FilterRow<'_>, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let needle = filter.to_lowercase();
    let haystacks = [
        row.project,
        row.agent,
        row.session_id,
        row.message.unwrap_or(""),
    ];
    haystacks.iter().any(|h| h.to_lowercase().contains(&needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row<'a>(project: &'a str, agent: &'a str, message: Option<&'a str>) -> FilterRow<'a> {
        FilterRow {
            session_id: "sess-x",
            project,
            agent,
            message,
        }
    }

    #[test]
    fn empty_filter_matches_everything() {
        assert!(matches(row("alpha", "claude-code", None), ""));
        assert!(matches(row("beta", "opencode", Some("hi")), ""));
    }

    #[test]
    fn filter_matches_project_substring() {
        assert!(matches(row("alpha-project", "claude-code", None), "alpha"));
        assert!(matches(row("alpha-project", "claude-code", None), "PROJ"));
    }

    #[test]
    fn filter_matches_agent_substring() {
        assert!(matches(row("p", "pi-coding-agent", None), "pi"));
        assert!(matches(row("p", "pi-coding-agent", None), "CODING"));
    }

    #[test]
    fn filter_matches_message_substring() {
        assert!(matches(
            row("p", "claude-code", Some("Permission required")),
            "permission",
        ));
    }

    #[test]
    fn filter_matches_session_id_substring() {
        assert!(matches(
            FilterRow {
                session_id: "abc-123-def",
                project: "p",
                agent: "a",
                message: None,
            },
            "123",
        ));
    }

    #[test]
    fn filter_rejects_when_nothing_matches() {
        assert!(!matches(row("alpha", "claude-code", Some("hi")), "xyz"));
    }

    #[test]
    fn filter_handles_missing_message_as_empty() {
        // Missing message shouldn't match "" arms-length checks unless filter is empty.
        // We already test the empty-filter path; ensure a non-empty filter doesn't
        // false-match against the unwrap_or("") default.
        assert!(!matches(row("p", "a", None), "anything"));
    }
}
