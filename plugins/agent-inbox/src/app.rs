//! Inbox state and key handling, independent of the terminal.

use std::collections::HashMap;

use herdr_client::models::AgentStatus;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::inbox::Inbox;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    List,
    Reply { pane_id: String, input: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Continue,
    Quit,
    Focus {
        pane_id: String,
    },
    /// Key presses for a blocked agent's approval dialog.
    Keys {
        pane_id: String,
        keys: Vec<&'static str>,
    },
    /// A prompt for an agent that is ready for input.
    Prompt {
        pane_id: String,
        text: String,
    },
}

pub struct App {
    pub inbox: Inbox,
    /// Index into the inbox's display order.
    pub selected: usize,
    pub mode: Mode,
    pub status: Option<String>,
    /// Agents answered from the inbox, with the state they were answered in.
    /// They stay hidden until herdr reports a newer state for them.
    answered: HashMap<String, u64>,
}

impl App {
    pub fn new(inbox: Inbox) -> Self {
        let mut app =
            Self { inbox: Inbox::default(), selected: 0, mode: Mode::List, status: None, answered: HashMap::new() };
        app.set_inbox(inbox);
        app
    }

    /// Replaces the rows, keeping the cursor on the same agent when it is still listed.
    pub fn set_inbox(&mut self, mut inbox: Inbox) {
        let current = self.inbox.get(self.selected).map(|entry| entry.pane_id.clone());
        self.answered.retain(|pane_id, seq| inbox.needs.iter().any(|e| e.pane_id == *pane_id && e.seq == *seq));
        inbox.needs.retain(|entry| self.answered.get(&entry.pane_id) != Some(&entry.seq));
        self.inbox = inbox;
        self.selected = current
            .and_then(|pane_id| self.inbox.position(&pane_id))
            .unwrap_or(self.selected)
            .min(self.inbox.len().saturating_sub(1));
    }

    /// Hides an agent after a reply was sent, so the cursor moves on to the next one.
    pub fn answered(&mut self, pane_id: &str) {
        if let Some(entry) = self.inbox.needs.iter().find(|entry| entry.pane_id == pane_id) {
            self.answered.insert(pane_id.to_string(), entry.seq);
        }
        let inbox = std::mem::take(&mut self.inbox);
        let selected = self.selected;
        self.set_inbox(inbox);
        self.selected = selected.min(self.inbox.len().saturating_sub(1));
    }

    pub fn selected_status(&self) -> Option<AgentStatus> {
        self.inbox.get(self.selected).map(|entry| entry.status)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Command {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Command::Quit;
        }
        match self.mode {
            Mode::List => self.list_key(key, ctrl),
            Mode::Reply { .. } => self.reply_key(key, ctrl),
        }
    }

    fn list_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let Some(entry) = self.inbox.get(self.selected) else {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('q') => Command::Quit,
                _ => Command::Continue,
            };
        };
        let pane_id = entry.pane_id.clone();
        match (key.code, entry.status) {
            (KeyCode::Esc | KeyCode::Char('q'), _) => return Command::Quit,
            (KeyCode::Enter, _) => return Command::Focus { pane_id },
            (KeyCode::Up | KeyCode::Char('k'), _) => self.move_selection(-1),
            (KeyCode::Down | KeyCode::Char('j'), _) => self.move_selection(1),
            (KeyCode::Char('p'), _) if ctrl => self.move_selection(-1),
            (KeyCode::Char('n'), _) if ctrl => self.move_selection(1),
            // Approval dialogs highlight "yes" by default and treat escape as "no".
            (KeyCode::Char('y'), AgentStatus::Blocked) => return Command::Keys { pane_id, keys: vec!["enter"] },
            (KeyCode::Char('n'), AgentStatus::Blocked) => return Command::Keys { pane_id, keys: vec!["esc"] },
            (KeyCode::Char('c'), AgentStatus::Done) => return Command::Prompt { pane_id, text: "continue".into() },
            (KeyCode::Char('r'), AgentStatus::Done) => {
                self.mode = Mode::Reply { pane_id, input: String::new() };
                self.status = None;
            }
            _ => {}
        }
        Command::Continue
    }

    fn reply_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let Mode::Reply { pane_id, input } = &mut self.mode else {
            return Command::Continue;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Enter if !input.trim().is_empty() => {
                let command = Command::Prompt { pane_id: pane_id.clone(), text: input.clone() };
                self.mode = Mode::List;
                return command;
            }
            KeyCode::Char('u') if ctrl => input.clear(),
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) if !ctrl => input.push(c),
            _ => {}
        }
        Command::Continue
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.inbox.len();
        if len > 0 {
            self.selected = self.selected.saturating_add_signed(delta).min(len - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::Entry;

    fn entry(pane: &str, status: AgentStatus, seq: u64) -> Entry {
        Entry {
            pane_id: pane.into(),
            agent: "claude".into(),
            status,
            location: "api › t1".into(),
            seq,
            title: None,
            preview: None,
        }
    }

    fn inbox() -> Inbox {
        Inbox {
            needs: vec![entry("p1", AgentStatus::Blocked, 1), entry("p2", AgentStatus::Done, 2)],
            working: vec![entry("p3", AgentStatus::Working, 3)],
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn quick_replies_match_the_selected_agent_state() {
        let mut app = App::new(inbox());
        assert_eq!(
            app.handle_key(key(KeyCode::Char('y'))),
            Command::Keys { pane_id: "p1".into(), keys: vec!["enter"] }
        );
        assert_eq!(app.handle_key(key(KeyCode::Char('n'))), Command::Keys { pane_id: "p1".into(), keys: vec!["esc"] });
        assert_eq!(app.handle_key(key(KeyCode::Char('c'))), Command::Continue);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.handle_key(key(KeyCode::Char('y'))), Command::Continue);
        assert_eq!(
            app.handle_key(key(KeyCode::Char('c'))),
            Command::Prompt { pane_id: "p2".into(), text: "continue".into() }
        );

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.handle_key(key(KeyCode::Char('c'))), Command::Continue);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Focus { pane_id: "p3".into() });
    }

    #[test]
    fn reply_mode_collects_text_and_sends_it() {
        let mut app = App::new(inbox());
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('r')));
        type_text(&mut app, "yes do it");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.mode, Mode::Reply { pane_id: "p2".into(), input: "yes do i".into() });
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Command::Prompt { pane_id: "p2".into(), text: "yes do i".into() }
        );
        assert_eq!(app.mode, Mode::List);
    }

    #[test]
    fn empty_reply_is_not_sent_and_esc_cancels() {
        let mut app = App::new(inbox());
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Continue);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Command::Quit);
    }

    #[test]
    fn answered_agent_hides_until_its_state_changes() {
        let mut app = App::new(inbox());
        app.answered("p1");
        assert_eq!(app.inbox.needs.len(), 1);
        assert_eq!(app.inbox.get(app.selected).unwrap().pane_id, "p2");

        app.set_inbox(inbox());
        assert_eq!(app.inbox.needs.len(), 1, "same state stays hidden");

        let mut changed = inbox();
        changed.needs[0].seq = 7;
        app.set_inbox(changed);
        assert_eq!(app.inbox.needs.len(), 2, "newer state shows again");
    }

    #[test]
    fn refresh_keeps_the_cursor_on_the_same_agent() {
        let mut app = App::new(inbox());
        app.handle_key(key(KeyCode::Down));
        let mut reordered = inbox();
        reordered.needs.reverse();
        app.set_inbox(reordered);
        assert_eq!(app.inbox.get(app.selected).unwrap().pane_id, "p2");
        assert_eq!(app.selected_status(), Some(AgentStatus::Done));
    }

    #[test]
    fn selection_stays_in_bounds_and_empty_inbox_quits() {
        let mut app = App::new(inbox());
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
        for _ in 0..5 {
            app.handle_key(key(KeyCode::Char('j')));
        }
        assert_eq!(app.selected, 2);

        let mut empty = App::new(Inbox::default());
        assert_eq!(empty.handle_key(key(KeyCode::Enter)), Command::Continue);
        assert_eq!(empty.handle_key(key(KeyCode::Char('q'))), Command::Quit);
    }
}
