//! Ratatui rendering. Pure function of `&App`; called from the event loop.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::app::App;

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
    Paragraph::new(Line::from(vec![
        Span::styled(
            "> ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(filter),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Filter"))
}

fn sessions_table(app: &App) -> Table<'_> {
    let visible = app.visible_indices();
    let spinner_idx = usize::try_from(app.tick).unwrap_or(0) % SPINNER_FRAMES.len();
    let spinner = SPINNER_FRAMES[spinner_idx];

    let rows: Vec<Row<'_>> = visible
        .iter()
        .enumerate()
        .map(|(view_idx, &entries_idx)| {
            let (sid, e) = &app.entries[entries_idx];
            let (status_text, status_color) = match e.event.as_str() {
                "working" => (spinner.to_string(), Color::Cyan),
                "notify" => ("!".to_string(), Color::Yellow),
                "done" => ("✓".to_string(), Color::Green),
                "idle" => ("·".to_string(), Color::DarkGray),
                other => (other.chars().next().unwrap_or('?').to_string(), Color::Gray),
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
                row = row.style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            }
            row
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(28),
        Constraint::Length(16),
        Constraint::Min(0),
    ];

    Table::new(rows, widths)
        .header(
            Row::new(vec!["", "Session", "Agent", "Activity"]).style(
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(Block::default().borders(Borders::ALL).title("Sessions"))
}

fn help_widget() -> Paragraph<'static> {
    Paragraph::new("Ctrl-N/P or ↓/↑: navigate · Enter: switch pane · Esc / Ctrl-C: cancel")
        .style(Style::default().fg(Color::DarkGray))
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

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MESSAGE_CAP: usize = 80;

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
}
