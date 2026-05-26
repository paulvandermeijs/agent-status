use crate::state::{AttentionEntry, Event};

/// Format the popup picker output: `session_id<TAB>pane<TAB>display\n` per entry.
///
/// The first two columns are machine-consumed (`session_id` is the preview key, pane is
/// the `tmux switch-client` target). The third column is a single space-padded display
/// string safe for fzf's `--with-nth=3`: a `[!]`/`[*]` marker (so fzf cannot fuzzy-match
/// the raw event word `notify`/`done`), then the project and agent names padded to the
/// max width in this list, then a one-line snippet of the agent's message if any.
#[must_use]
pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    const PROJECT_CAP: usize = 30;
    const AGENT_CAP: usize = 16;
    const MESSAGE_CAP: usize = 80;

    let visible: Vec<&(String, AttentionEntry)> = entries
        .iter()
        .filter(|(_, e)| e.event.needs_attention())
        .collect();
    if visible.is_empty() {
        return String::new();
    }

    let project_width = visible
        .iter()
        .map(|(_, e)| e.project.chars().count().min(PROJECT_CAP))
        .max()
        .unwrap_or(0);
    let agent_width = visible
        .iter()
        .map(|(_, e)| e.agent.chars().count().min(AGENT_CAP))
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for (sid, e) in &visible {
        let marker = if e.event == Event::Notify { "[!]" } else { "[*]" };
        let project = truncate_chars(&e.project, PROJECT_CAP);
        let agent = truncate_chars(&e.agent, AGENT_CAP);
        let snippet = e
            .message
            .as_deref()
            .map(|m| one_line(m, MESSAGE_CAP))
            .unwrap_or_default();

        let mut display = format!("{marker} {project:<project_width$}  {agent:<agent_width$}");
        if !snippet.is_empty() {
            display.push_str("  ");
            display.push_str(&snippet);
        }

        out.push_str(sid);
        out.push('\t');
        out.push_str(e.tmux_pane.as_deref().unwrap_or(""));
        out.push('\t');
        out.push_str(display.trim_end());
        out.push('\n');
    }
    out
}

fn truncate_chars(s: &str, cap: usize) -> String {
    s.chars().take(cap).collect()
}

fn one_line(s: &str, cap: usize) -> String {
    let mut flat = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if !flat.ends_with(' ') {
                flat.push(' ');
            }
        } else {
            flat.push(ch);
        }
    }
    truncate_chars(flat.trim(), cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, pane: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: Event::from(event),
            tmux_pane: Some(pane.into()),
            ts: 1,
            message: None,
            pid: None,
            tmux_session: None,
        }
    }

    #[test]
    fn format_list_emits_session_id_pane_display_columns() {
        let entries = vec![
            ("sess-1".into(), entry("alpha", "%1", "notify")),
            ("sess-2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert_eq!(line.matches('\t').count(), 2, "line: {line:?}");
        }
        let cols0: Vec<&str> = lines[0].split('\t').collect();
        assert_eq!(cols0[0], "sess-1");
        assert_eq!(cols0[1], "%1");
        assert!(cols0[2].contains("alpha"));
        assert!(cols0[2].contains("claude-code"));
    }

    #[test]
    fn format_list_uses_bracket_marker_not_event_word() {
        let entries = vec![
            ("s1".into(), entry("alpha", "%1", "notify")),
            ("s2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        for line in out.lines() {
            let display = line.split('\t').nth(2).unwrap();
            assert!(!display.contains("notify"), "display: {display:?}");
            assert!(!display.contains("done"), "display: {display:?}");
        }
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].split('\t').nth(2).unwrap().starts_with("[!] "));
        assert!(lines[1].split('\t').nth(2).unwrap().starts_with("[*] "));
    }

    #[test]
    fn format_list_pads_project_and_agent_columns_to_max_width() {
        let mut a = entry("short", "%1", "notify");
        a.agent = "claude-code".into();
        let mut b = entry("a-much-longer-project-name", "%2", "done");
        b.agent = "pi-coding-agent".into();
        let entries = vec![("s1".into(), a), ("s2".into(), b)];
        let out = format_list(&entries);
        let lines: Vec<&str> = out.lines().collect();
        let display0 = lines[0].split('\t').nth(2).unwrap();
        let display1 = lines[1].split('\t').nth(2).unwrap();
        let agent0 = display0.find("claude-code").expect("agent on line 0");
        let agent1 = display1.find("pi-coding-agent").expect("agent on line 1");
        assert_eq!(agent0, agent1, "agent column not aligned: {display0:?} vs {display1:?}");
    }

    #[test]
    fn format_list_appends_message_snippet_when_present() {
        let mut e = entry("alpha", "%1", "notify");
        e.message = Some("Permission required to read /etc/passwd".into());
        let out = format_list(&[("s1".into(), e)]);
        let display = out.lines().next().unwrap().split('\t').nth(2).unwrap();
        assert!(display.contains("Permission required"), "display: {display:?}");
    }

    #[test]
    fn format_list_collapses_newlines_in_message_snippet() {
        let mut e = entry("alpha", "%1", "notify");
        e.message = Some("line one\nline two\r\nline three".into());
        let out = format_list(&[("s1".into(), e)]);
        assert_eq!(out.matches('\n').count(), 1, "got: {out:?}");
        let display = out.lines().next().unwrap().split('\t').nth(2).unwrap();
        assert!(!display.contains('\n'));
        assert!(!display.contains('\r'));
    }

    #[test]
    fn format_list_truncates_long_message_snippet() {
        let mut e = entry("alpha", "%1", "notify");
        e.message = Some("x".repeat(500));
        let out = format_list(&[("s1".into(), e)]);
        let display = out.lines().next().unwrap().split('\t').nth(2).unwrap();
        assert!(display.len() < 200, "display too long: {} chars", display.len());
    }

    #[test]
    fn format_list_empty_input_returns_empty_string() {
        assert_eq!(format_list(&[]), "");
    }

    #[test]
    fn format_list_ignores_idle_entries() {
        let idle = entry("alpha", "%1", "idle");
        let waiting = entry("beta", "%2", "notify");
        let out = format_list(&[("s1".into(), idle), ("s2".into(), waiting)]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1, "got: {lines:?}");
        assert!(lines[0].contains("beta"));
        assert!(!lines[0].contains("alpha"));
    }

    #[test]
    fn format_list_ignores_working_entries() {
        let working = entry("alpha", "%1", "working");
        let waiting = entry("beta", "%2", "notify");
        let out = format_list(&[
            ("s1".into(), working),
            ("s2".into(), waiting),
        ]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1, "got: {lines:?}");
        assert!(lines[0].contains("beta"));
        assert!(!lines[0].contains("alpha"));
    }
}
