//! Switcher app state — entries, filter input, selection, spinner tick.
//!
//! The `tick` method reads `StateStore`, so it's exercised in integration
//! tests; everything else (key handling, filter, selection clamping) is pure.

use agent_status::{AttentionEntry, StateStore};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::filter::{FilterRow, matches};

/// Outcome of one key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Keep running.
    Continue,
    /// User pressed Esc or Ctrl-C — exit without switching.
    Cancel,
    /// User pressed Enter — switch to the selected session's pane, then exit.
    Activate,
}

pub struct App {
    store: StateStore,
    pub entries: Vec<(String, AttentionEntry)>,
    pub filter: String,
    pub selected: usize,
    pub tick: u64,
}

impl App {
    #[must_use]
    pub fn new(store: StateStore) -> Self {
        let entries = store.list().unwrap_or_default();
        Self {
            store,
            entries,
            filter: String::new(),
            selected: 0,
            tick: 0,
        }
    }

    /// Re-read the state directory and bump the spinner tick. Called from the
    /// event loop on each ~250ms timer.
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.entries = self.store.list().unwrap_or_default();
        self.clamp_selection();
    }

    /// Indices into `self.entries` that pass the filter, preserving order.
    #[must_use]
    pub fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, (sid, e))| {
                matches(
                    FilterRow {
                        session_id: sid,
                        project: &e.project,
                        agent: &e.agent,
                        message: e.message.as_deref(),
                    },
                    &self.filter,
                )
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The entry at the current selected row in the filtered view, if any.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&(String, AttentionEntry)> {
        let idx = *self.visible_indices().get(self.selected)?;
        self.entries.get(idx)
    }

    /// Reduce one key event into a state change. Pure-ish: the only side effect
    /// is mutating `self`.
    pub fn handle_key(&mut self, key: KeyEvent) -> KeyOutcome {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => KeyOutcome::Cancel,
            (KeyCode::Enter, _) => KeyOutcome::Activate,
            (KeyCode::Char('n'), KeyModifiers::CONTROL) | (KeyCode::Down, _) => {
                self.move_down();
                KeyOutcome::Continue
            }
            (KeyCode::Char('p'), KeyModifiers::CONTROL) | (KeyCode::Up, _) => {
                self.move_up();
                KeyOutcome::Continue
            }
            (KeyCode::Backspace, _) => {
                self.filter.pop();
                self.selected = 0;
                KeyOutcome::Continue
            }
            (KeyCode::Char(c), m)
                if !m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) =>
            {
                self.filter.push(c);
                self.selected = 0;
                KeyOutcome::Continue
            }
            _ => KeyOutcome::Continue,
        }
    }

    fn move_down(&mut self) {
        let n = self.visible_indices().len();
        if n == 0 {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1) % n;
        }
    }

    fn move_up(&mut self) {
        let n = self.visible_indices().len();
        if n == 0 {
            self.selected = 0;
        } else {
            self.selected = if self.selected == 0 {
                n - 1
            } else {
                self.selected - 1
            };
        }
    }

    fn clamp_selection(&mut self) {
        let n = self.visible_indices().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_status::AttentionEntry;
    use tempfile::TempDir;

    fn sample(project: &str, event: &str) -> AttentionEntry {
        AttentionEntry {
            agent: "claude-code".into(),
            project: project.into(),
            cwd: format!("/x/{project}"),
            event: event.into(),
            tmux_pane: "%1".into(),
            ts: 1,
            message: None,
            pid: None,
        }
    }

    fn app_with_entries(entries: &[(&str, &str)]) -> App {
        let dir = TempDir::new().unwrap();
        let store = StateStore::new(dir.path().to_path_buf());
        for (sid, project) in entries {
            store.write(sid, &sample(project, "notify")).unwrap();
        }
        // Leak the tempdir so the store keeps working for the test. The OS
        // cleans it up at process exit anyway.
        let _ = Box::leak(Box::new(dir));
        App::new(store)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn empty_app_has_no_visible_entries() {
        let app = app_with_entries(&[]);
        assert_eq!(app.visible_indices().len(), 0);
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn ctrl_n_wraps_to_the_top_at_the_bottom() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta"), ("s3", "gamma")]);
        assert_eq!(app.selected, 0);
        assert_eq!(app.handle_key(ctrl('n')), KeyOutcome::Continue);
        assert_eq!(app.selected, 1);
        assert_eq!(app.handle_key(ctrl('n')), KeyOutcome::Continue);
        assert_eq!(app.selected, 2);
        assert_eq!(app.handle_key(ctrl('n')), KeyOutcome::Continue);
        assert_eq!(app.selected, 0, "should wrap");
    }

    #[test]
    fn ctrl_p_wraps_to_the_bottom_at_the_top() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta"), ("s3", "gamma")]);
        assert_eq!(app.handle_key(ctrl('p')), KeyOutcome::Continue);
        assert_eq!(app.selected, 2, "should wrap to bottom");
    }

    #[test]
    fn typing_chars_appends_to_filter_and_resets_selection() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta"), ("s3", "gamma")]);
        app.selected = 2;
        app.handle_key(key(KeyCode::Char('b')));
        assert_eq!(app.filter, "b");
        assert_eq!(app.selected, 0);
        // Only one entry matches "b" → beta.
        let visible = app.visible_indices();
        assert_eq!(visible.len(), 1);
        let (sid, _) = &app.entries[visible[0]];
        assert_eq!(sid, "s2");
    }

    #[test]
    fn backspace_pops_one_filter_char() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        app.filter.push_str("xyz");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.filter, "xy");
    }

    #[test]
    fn esc_returns_cancel() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), KeyOutcome::Cancel);
    }

    #[test]
    fn ctrl_c_returns_cancel() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        assert_eq!(app.handle_key(ctrl('c')), KeyOutcome::Cancel);
    }

    #[test]
    fn enter_returns_activate() {
        let mut app = app_with_entries(&[("s1", "alpha")]);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), KeyOutcome::Activate);
    }

    #[test]
    fn selection_clamps_when_filter_shrinks_visible_set() {
        let mut app = app_with_entries(&[("s1", "alpha"), ("s2", "beta")]);
        app.selected = 1;
        // Type "alp" — only "alpha" matches, so selected should reset.
        for c in "alp".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn tick_increments_and_does_not_panic_on_empty_store() {
        let mut app = app_with_entries(&[]);
        let before = app.tick;
        app.tick();
        assert_eq!(app.tick, before.wrapping_add(1));
    }
}
