use crate::state::AttentionEntry;
use std::path::Path;

/// Build the Claude Code `--settings` JSON wiring the agent-status hooks to `bin_path`.
///
/// Returns `Some(json)` for agents that support `--settings`-style injection
/// (currently only `claude-code`); returns `None` for any other agent name. The
/// JSON wires the same six hooks documented in the README's manual settings
/// snippet — `Notification`/`Stop` → `set`, `UserPromptSubmit`/`PreToolUse`/
/// `SessionStart`/`SessionEnd` → `clear`.
///
/// `bin_path` is embedded into the `command` strings via `serde_json::json!`
/// so quotes/backslashes in the path are escaped safely without manual work.
pub fn build_settings_json(bin_path: &str, agent_name: &str) -> Option<String> {
    if agent_name != "claude-code" {
        return None;
    }
    let set_notify = format!("{bin_path} set --agent claude-code notify");
    let set_done = format!("{bin_path} set --agent claude-code done");
    let clear = format!("{bin_path} clear --agent claude-code");

    let value = serde_json::json!({
        "hooks": {
            "Notification":     [{"hooks": [{"type": "command", "command": set_notify}]}],
            "Stop":             [{"hooks": [{"type": "command", "command": set_done}]}],
            "UserPromptSubmit": [{"hooks": [{"type": "command", "command": clear}]}],
            "PreToolUse":       [{"hooks": [{"type": "command", "command": clear}]}],
            "SessionStart":     [{"hooks": [{"type": "command", "command": clear}]}],
            "SessionEnd":       [{"hooks": [{"type": "command", "command": clear}]}],
        }
    });
    Some(serde_json::to_string_pretty(&value).expect("serde_json::Value always serializes"))
}

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

/// Format the popup picker output: `session_id<TAB>pane<TAB>display\n` per entry.
///
/// The first two columns are machine-consumed (`session_id` is the preview key, pane is
/// the `tmux switch-client` target). The third column is a single space-padded display
/// string safe for fzf's `--with-nth=3`: a `[!]`/`[*]` marker (so fzf cannot fuzzy-match
/// the raw event word `notify`/`done`), then the project and agent names padded to the
/// max width in this list, then a one-line snippet of the agent's message if any.
pub fn format_list(entries: &[(String, AttentionEntry)]) -> String {
    const PROJECT_CAP: usize = 30;
    const AGENT_CAP: usize = 16;
    const MESSAGE_CAP: usize = 80;

    if entries.is_empty() {
        return String::new();
    }

    let project_width = entries
        .iter()
        .map(|(_, e)| e.project.chars().count().min(PROJECT_CAP))
        .max()
        .unwrap_or(0);
    let agent_width = entries
        .iter()
        .map(|(_, e)| e.agent.chars().count().min(AGENT_CAP))
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for (sid, e) in entries {
        let marker = if e.event == "notify" { "[!]" } else { "[*]" };
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
        out.push_str(&e.tmux_pane);
        out.push('\t');
        out.push_str(display.trim_end());
        out.push('\n');
    }
    out
}

/// Format the multi-line preview shown in fzf's `--preview` pane for one entry.
///
/// `now_ts` is the caller-supplied current Unix time in seconds, used to render `Age:`.
/// The output is plain ASCII: a label-aligned key/value block, optionally followed by
/// a `Message:` section when `entry.message` is `Some`. The `Message:` body preserves
/// embedded newlines verbatim so multi-line agent responses read naturally.
pub fn format_preview(entry: &AttentionEntry, now_ts: u64) -> String {
    use std::fmt::Write as _;
    let age = now_ts.saturating_sub(entry.ts);
    let pane = if entry.tmux_pane.is_empty() {
        "-"
    } else {
        entry.tmux_pane.as_str()
    };
    let mut out = String::new();
    writeln!(out, "Project:    {}", entry.project).unwrap();
    writeln!(out, "Agent:      {}", entry.agent).unwrap();
    writeln!(out, "Event:      {}", entry.event).unwrap();
    writeln!(out, "CWD:        {}", entry.cwd).unwrap();
    writeln!(out, "Pane:       {pane}").unwrap();
    writeln!(out, "Age:        {}", format_age(age)).unwrap();
    if let Some(msg) = entry.message.as_deref() {
        out.push('\n');
        out.push_str("Message:\n");
        out.push_str(msg);
        if !msg.ends_with('\n') {
            out.push('\n');
        }
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

fn format_age(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m {s:02}s")
    } else if secs < 86_400 {
        let h = secs / 3_600;
        let m = (secs % 3_600) / 60;
        format!("{h}h {m:02}m")
    } else {
        let d = secs / 86_400;
        let h = (secs % 86_400) / 3_600;
        format!("{d}d {h:02}h")
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
    fn format_list_emits_session_id_pane_display_columns() {
        let entries = vec![
            ("sess-1".into(), entry("alpha", "%1", "notify")),
            ("sess-2".into(), entry("beta", "%2", "done")),
        ];
        let out = format_list(&entries);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        // Each line has exactly two tabs: session_id<TAB>pane<TAB>display
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
        // The bare event word must not appear in the display column,
        // so fzf doesn't fuzzy-match against "notify"/"done".
        for line in out.lines() {
            let display = line.split('\t').nth(2).unwrap();
            assert!(!display.contains("notify"), "display: {display:?}");
            assert!(!display.contains("done"), "display: {display:?}");
        }
        // Notify rows get [!]; other rows get [*].
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
        // The project column slot occupies max(len("short"), len("a-much-longer-project-name")) chars.
        // Easiest invariant to check: the position of the agent token is the same on both lines.
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
        // The display column must be exactly one line (no embedded newlines).
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
        // Cap at 80 chars of the message body itself.
        assert!(display.len() < 200, "display too long: {} chars", display.len());
    }

    #[test]
    fn format_list_empty_input_returns_empty_string() {
        assert_eq!(format_list(&[]), "");
    }

    #[test]
    fn format_preview_includes_core_fields() {
        let mut e = entry("alpha", "%17", "notify");
        e.cwd = "/Users/x/work/alpha".into();
        e.ts = 1_000;
        let out = format_preview(&e, 1_000 + 134); // 134 seconds later
        assert!(out.contains("Project:"));
        assert!(out.contains("alpha"));
        assert!(out.contains("Agent:"));
        assert!(out.contains("claude-code"));
        assert!(out.contains("Event:"));
        assert!(out.contains("notify"));
        assert!(out.contains("CWD:"));
        assert!(out.contains("/Users/x/work/alpha"));
        assert!(out.contains("Pane:"));
        assert!(out.contains("%17"));
        assert!(out.contains("Age:"));
        assert!(out.contains("2m"), "expected 2m in: {out}");
    }

    #[test]
    fn format_preview_omits_message_section_when_none() {
        let e = entry("alpha", "%17", "done");
        let out = format_preview(&e, e.ts);
        assert!(!out.contains("Message:"), "got: {out}");
    }

    #[test]
    fn format_preview_includes_message_section_when_some() {
        let mut e = entry("alpha", "%17", "notify");
        e.message = Some("Permission required\nfor /etc/passwd".into());
        let out = format_preview(&e, e.ts);
        assert!(out.contains("Message:"));
        assert!(out.contains("Permission required"));
        // Multi-line messages should be preserved (unlike the list snippet).
        assert!(out.contains("for /etc/passwd"));
    }

    #[test]
    fn format_preview_age_handles_seconds_minutes_hours_days() {
        let e = entry("p", "%1", "done"); // ts = 1 from helper
        assert!(format_preview(&e, e.ts).contains("Age:        0s"));
        assert!(format_preview(&e, e.ts + 9).contains("Age:        9s"));
        assert!(format_preview(&e, e.ts + 75).contains("Age:        1m 15s"));
        assert!(format_preview(&e, e.ts + 3_600 + 120).contains("Age:        1h 02m"));
        assert!(format_preview(&e, e.ts + 3 * 86_400 + 4 * 3_600).contains("Age:        3d 04h"));
    }

    #[test]
    fn format_preview_age_clamps_when_now_before_ts() {
        // Defense against clock skew: if now < ts, render as 0s rather than panicking.
        let mut e = entry("p", "%1", "done");
        e.ts = 100;
        let out = format_preview(&e, 50);
        assert!(out.contains("Age:        0s"), "got: {out}");
    }

    #[test]
    fn build_settings_json_returns_none_for_unknown_agent() {
        assert!(build_settings_json("/x/agent-status", "pi-coding-agent").is_none());
        assert!(build_settings_json("/x/agent-status", "opencode").is_none());
        assert!(build_settings_json("/x/agent-status", "frobnicator").is_none());
    }

    #[test]
    fn build_settings_json_returns_some_for_claude_code() {
        let json = build_settings_json("/x/agent-status", "claude-code")
            .expect("claude-code is supported");
        // Parse-back roundtrip — output must be valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("hooks").is_some(), "missing top-level hooks key");
    }

    #[test]
    fn build_settings_json_wires_all_six_hook_events() {
        let json = build_settings_json("/x/agent-status", "claude-code").unwrap();
        for event in [
            "Notification",
            "Stop",
            "UserPromptSubmit",
            "PreToolUse",
            "SessionStart",
            "SessionEnd",
        ] {
            assert!(json.contains(event), "missing hook event {event} in: {json}");
        }
    }

    #[test]
    fn build_settings_json_uses_set_and_clear_correctly() {
        let json = build_settings_json("/path/to/agent-status", "claude-code").unwrap();
        // Notification → notify, Stop → done.
        assert!(json.contains("set --agent claude-code notify"));
        assert!(json.contains("set --agent claude-code done"));
        // The four clear events all share one command string.
        assert!(json.contains("clear --agent claude-code"));
        // Sanity: the binary path is embedded verbatim.
        assert!(json.contains("/path/to/agent-status"));
    }

    #[test]
    fn build_settings_json_escapes_unsafe_chars_in_bin_path() {
        // A path with a quote and a backslash would corrupt JSON if interpolated raw.
        // serde_json::json! handles the escaping for us; verify the output round-trips.
        let json = build_settings_json(r#"/x/has"quote\and-backslash/agent-status"#, "claude-code")
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let command = parsed
            .pointer("/hooks/Notification/0/hooks/0/command")
            .and_then(serde_json::Value::as_str)
            .expect("notification command string");
        assert!(command.contains(r#"has"quote\and-backslash"#), "got: {command}");
    }
}
