//! Ratatui rendering. Pure function of `&App`; called from the event loop.

use agent_status::Event;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
};

use crate::app::{App, event_rank};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    f.render_widget(filter_widget(&app.filter), chunks[0]);
    f.render_widget(sessions_table(app), chunks[1]);
    f.render_widget(help_widget(), chunks[2]);
}

fn filter_widget(filter: &str) -> Paragraph<'_> {
    Paragraph::new(Line::from(vec![Span::raw("❯ "), Span::raw(filter)])).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(border_type())
            .title(" Filter "),
    )
}

fn sessions_table(app: &App) -> Table<'_> {
    let visible = app.visible_indices();
    let spinner_idx = usize::try_from(app.tick).unwrap_or(0) % SPINNER_FRAMES.len();
    let spinner = SPINNER_FRAMES[spinner_idx];

    let mut rows: Vec<Row<'_>> = Vec::with_capacity(visible.len() + 4);
    let mut current_group: Option<u8> = None;
    for (view_idx, &entries_idx) in visible.iter().enumerate() {
        let (sid, e) = &app.entries[entries_idx];
        let rank = event_rank(&e.event);
        if Some(rank) != current_group {
            rows.push(section_header_row(&e.event));
            current_group = Some(rank);
        }
        let (status_text, status_color) = match &e.event {
            Event::Working => (spinner.to_string(), Color::Cyan),
            Event::Notify => ("!".to_string(), Color::Yellow),
            Event::Done => ("✓".to_string(), Color::Green),
            Event::Idle => ("·".to_string(), Color::Gray),
            Event::Unknown(s) => (s.chars().next().unwrap_or('?').to_string(), Color::Gray),
        };
        let session = display_session(sid, &e.project);
        let snippet = e.message.as_deref().map(one_line).unwrap_or_default();
        let mut row = Row::new(vec![
            Cell::from(status_text).style(Style::default().fg(status_color)),
            Cell::from(session),
            Cell::from(e.agent.clone()),
            Cell::from(snippet),
        ]);
        if view_idx == app.selected {
            // Bright-black (ANSI 8) fill — the neutral counterpart to
            // Idle's bright-white banner. Distinct from every section
            // background (Yellow / Green / White / Cyan / Gray) and
            // hueless so the selection doesn't compete with the
            // group-color story. Ratatui composes row bg with each
            // cell's own fg, so the status glyph keeps its event
            // color while the rest of the row gets white-on-darkgray.
            row = row.style(
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );
        }
        rows.push(row);
    }

    let widths = [
        Constraint::Length(2),
        Constraint::Length(28),
        Constraint::Length(16),
        Constraint::Min(0),
    ];

    Table::new(rows, widths)
        .header(
            Row::new(vec!["", "Session", "Agent", "Activity"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(border_type())
                .title(" Sessions "),
        )
}

fn help_widget() -> Paragraph<'static> {
    Paragraph::new("Ctrl-N/P or ↓/↑: navigate · Enter: switch pane · Esc / Ctrl-C: cancel")
}

fn display_session(session_id: &str, project: &str) -> String {
    // Project name is the primary handle; session_id is the disambiguator on
    // the rare event that two sessions share a project.
    let short_sid: String = session_id.chars().take(8).collect();
    format!("{project} ({short_sid})")
}

fn one_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out.chars()
        .take(MESSAGE_CAP)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Build a non-selectable row that labels the start of an event group
/// (Needs your attention / Done / Idle / Working / Other). The whole row is painted
/// with the group's color as a background so the section break reads
/// as a banner; the foreground is picked for contrast against it.
fn section_header_row(event: &Event) -> Row<'static> {
    let (label, bg, fg) = section_header_style(event);
    Row::new(vec![
        Cell::from(""),
        Cell::from(label),
        Cell::from(""),
        Cell::from(""),
    ])
    .style(
        Style::default()
            .bg(bg)
            .fg(fg)
            .add_modifier(Modifier::BOLD),
    )
}

/// Label, background color, and contrasting foreground for each event
/// group's header banner. Every banner uses a light/saturated background
/// with black foreground; `DarkGray` is reserved for the selection
/// indicator so the two roles can't be confused.
fn section_header_style(event: &Event) -> (&'static str, Color, Color) {
    match event {
        Event::Notify => ("Needs your attention", Color::Yellow, Color::Black),
        Event::Done => ("Done", Color::Green, Color::Black),
        Event::Idle => ("Idle", Color::Gray, Color::Black),
        Event::Working => ("Working", Color::Cyan, Color::Black),
        Event::Unknown(_) => ("Other", Color::White, Color::Black),
    }
}

/// Resolve the active border style for all bordered blocks. Reads
/// `AGENT_SWITCHER_BORDER_TYPE` once and caches it; unrecognized or
/// unset values fall back to `Plain`.
fn border_type() -> BorderType {
    use std::sync::OnceLock;
    static CACHE: OnceLock<BorderType> = OnceLock::new();
    *CACHE.get_or_init(|| {
        std::env::var(BORDER_TYPE_ENV)
            .ok()
            .as_deref()
            .and_then(parse_border_type)
            .unwrap_or(BorderType::Plain)
    })
}

fn parse_border_type(s: &str) -> Option<BorderType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "plain" => Some(BorderType::Plain),
        "rounded" => Some(BorderType::Rounded),
        "double" => Some(BorderType::Double),
        "thick" => Some(BorderType::Thick),
        "quadrantinside" | "quadrant-inside" | "quadrant_inside" => {
            Some(BorderType::QuadrantInside)
        }
        "quadrantoutside" | "quadrant-outside" | "quadrant_outside" => {
            Some(BorderType::QuadrantOutside)
        }
        _ => None,
    }
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MESSAGE_CAP: usize = 80;
const BORDER_TYPE_ENV: &str = "AGENT_SWITCHER_BORDER_TYPE";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_collapses_newlines_and_tabs_to_single_spaces() {
        assert_eq!(one_line("a\nb\n\nc"), "a b c");
        assert_eq!(one_line("a\tb\rc"), "a b c");
    }

    #[test]
    fn one_line_caps_long_input() {
        let long = "x".repeat(500);
        let result = one_line(&long);
        assert!(result.chars().count() <= MESSAGE_CAP);
    }

    #[test]
    fn display_session_truncates_session_id_to_eight_chars() {
        let out = display_session("abcdef-1234-very-long-session-id", "alpha");
        assert_eq!(out, "alpha (abcdef-1)");
    }

    #[test]
    fn section_header_label_matches_event_variant() {
        assert_eq!(section_header_style(&Event::Notify).0, "Needs your attention");
        assert_eq!(section_header_style(&Event::Done).0, "Done");
        assert_eq!(section_header_style(&Event::Idle).0, "Idle");
        assert_eq!(section_header_style(&Event::Working).0, "Working");
        assert_eq!(
            section_header_style(&Event::Unknown("future".into())).0,
            "Other"
        );
    }

    #[test]
    fn parse_border_type_known_values() {
        assert_eq!(parse_border_type("plain"), Some(BorderType::Plain));
        assert_eq!(parse_border_type("rounded"), Some(BorderType::Rounded));
        assert_eq!(parse_border_type("double"), Some(BorderType::Double));
        assert_eq!(parse_border_type("thick"), Some(BorderType::Thick));
    }

    #[test]
    fn parse_border_type_is_case_insensitive_and_trims() {
        assert_eq!(parse_border_type("  Rounded\n"), Some(BorderType::Rounded));
        assert_eq!(parse_border_type("THICK"), Some(BorderType::Thick));
    }

    #[test]
    fn parse_border_type_accepts_quadrant_synonyms() {
        assert_eq!(
            parse_border_type("quadrant_inside"),
            Some(BorderType::QuadrantInside),
        );
        assert_eq!(
            parse_border_type("quadrant-outside"),
            Some(BorderType::QuadrantOutside),
        );
        assert_eq!(
            parse_border_type("quadrantinside"),
            Some(BorderType::QuadrantInside),
        );
    }

    #[test]
    fn parse_border_type_rejects_garbage() {
        assert_eq!(parse_border_type(""), None);
        assert_eq!(parse_border_type("nope"), None);
    }
}
