//! Palette state and key handling, independent of the terminal.

use std::collections::HashMap;

use herdr_client::PluginContext;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::item::{Action, Item};
use crate::matcher::{self, Ranked};
use crate::sources::builtins::Step;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    List,
    Prompt { item: usize, label: String, input: String },
    Confirm { item: usize, question: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Continue,
    Quit,
    /// Run `items[item]` with the given prompt input.
    Run { item: usize, input: String },
}

pub struct App {
    pub items: Vec<Item>,
    pub query: String,
    pub ranked: Vec<Ranked>,
    /// Index into `ranked`.
    pub selected: usize,
    pub mode: Mode,
    pub status: Option<Status>,
    scores: HashMap<String, f64>,
}

impl App {
    pub fn new(items: Vec<Item>, scores: HashMap<String, f64>, status: Option<Status>) -> Self {
        let mut app = Self {
            items,
            query: String::new(),
            ranked: Vec::new(),
            selected: 0,
            mode: Mode::List,
            status,
            scores,
        };
        app.refilter();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent, ctx: &PluginContext) -> Command {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Command::Quit;
        }
        match self.mode {
            Mode::List => self.list_key(key, ctrl, ctx),
            Mode::Prompt { .. } => self.prompt_key(key, ctrl),
            Mode::Confirm { .. } => self.confirm_key(key),
        }
    }

    /// Shows an action failure. A confirm step is abandoned; a prompt stays
    /// open so the input can be corrected.
    pub fn fail(&mut self, message: String) {
        if matches!(self.mode, Mode::Confirm { .. }) {
            self.mode = Mode::List;
        }
        self.status = Some(Status::Error(message));
    }

    fn list_key(&mut self, key: KeyEvent, ctrl: bool, ctx: &PluginContext) -> Command {
        match key.code {
            KeyCode::Esc => return Command::Quit,
            KeyCode::Enter => return self.activate(ctx),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('p') if ctrl => self.move_selection(-1),
            KeyCode::Char('n') if ctrl => self.move_selection(1),
            KeyCode::Char('u') if ctrl => {
                self.query.clear();
                self.refilter();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.refilter();
            }
            _ => {}
        }
        Command::Continue
    }

    fn prompt_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let Mode::Prompt { item, input, .. } = &mut self.mode else {
            return Command::Continue;
        };
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                self.status = None;
            }
            KeyCode::Enter => return Command::Run { item: *item, input: input.clone() },
            KeyCode::Char('u') if ctrl => input.clear(),
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) if !ctrl => input.push(c),
            _ => {}
        }
        Command::Continue
    }

    fn confirm_key(&mut self, key: KeyEvent) -> Command {
        let Mode::Confirm { item, .. } = self.mode else {
            return Command::Continue;
        };
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => Command::Run { item, input: String::new() },
            KeyCode::Esc | KeyCode::Char('n') => {
                self.mode = Mode::List;
                Command::Continue
            }
            _ => Command::Continue,
        }
    }

    fn activate(&mut self, ctx: &PluginContext) -> Command {
        let Some(index) = self.ranked.get(self.selected).map(|r| r.index) else {
            return Command::Continue;
        };
        let run = Command::Run { item: index, input: String::new() };
        let Action::Builtin(builtin) = &self.items[index].action else {
            return run;
        };
        match builtin.step(ctx) {
            Step::Run => run,
            Step::Prompt { label, initial } => {
                self.mode = Mode::Prompt { item: index, label, input: initial };
                self.status = None;
                Command::Continue
            }
            Step::Confirm { question } => {
                self.mode = Mode::Confirm { item: index, question };
                self.status = None;
                Command::Continue
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let last = self.ranked.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    fn refilter(&mut self) {
        let scores = &self.scores;
        self.ranked = matcher::rank(&self.query, &self.items, |id| scores.get(id).copied().unwrap_or(0.0));
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Item, Kind};
    use crate::sources::builtins::Builtin;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    fn ctx() -> PluginContext {
        PluginContext { tab_id: Some("w1:t1".into()), tab_label: Some("Claude".into()), ..Default::default() }
    }

    fn app() -> App {
        let items = vec![
            Item::new(Kind::Workspace, "ws:w1", "CT", Action::FocusWorkspace("w1".into())),
            Item::new(Kind::Workspace, "ws:w2", "boardwalk", Action::FocusWorkspace("w2".into())),
            Item::new(Kind::Command, "cmd:rename-tab", "Rename tab", Action::Builtin(Builtin::RenameTab)),
            Item::new(Kind::Command, "cmd:close-tab", "Close tab", Action::Builtin(Builtin::CloseTab)),
            Item::new(Kind::Command, "cmd:split-right", "Split pane right", Action::Builtin(Builtin::SplitRight)),
        ];
        App::new(items, HashMap::new(), None)
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            assert_eq!(app.handle_key(ch(c), &ctx()), Command::Continue);
        }
    }

    fn selected_title(app: &App) -> &str {
        &app.items[app.ranked[app.selected].index].title
    }

    #[test]
    fn starts_with_all_items_ranked_and_first_selected() {
        let app = app();
        assert_eq!(app.ranked.len(), 5);
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::List);
    }

    #[test]
    fn typing_filters_and_resets_selection() {
        let mut app = app();
        app.handle_key(key(KeyCode::Down), &ctx());
        type_str(&mut app, "board");
        assert_eq!(app.query, "board");
        assert_eq!(app.ranked.len(), 1);
        assert_eq!(app.selected, 0);
        assert_eq!(selected_title(&app), "boardwalk");
        app.handle_key(key(KeyCode::Backspace), &ctx());
        assert_eq!(app.query, "boar");
        app.handle_key(ctrl('u'), &ctx());
        assert_eq!(app.query, "");
        assert_eq!(app.ranked.len(), 5);
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut app = app();
        app.handle_key(key(KeyCode::Up), &ctx());
        assert_eq!(app.selected, 0);
        app.handle_key(ctrl('n'), &ctx());
        app.handle_key(key(KeyCode::Down), &ctx());
        assert_eq!(app.selected, 2);
        for _ in 0..10 {
            app.handle_key(key(KeyCode::Down), &ctx());
        }
        assert_eq!(app.selected, 4);
        app.handle_key(ctrl('p'), &ctx());
        assert_eq!(app.selected, 3);
    }

    #[test]
    fn enter_on_plain_item_runs_it() {
        let mut app = app();
        type_str(&mut app, "boardwalk");
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 1, input: String::new() });
    }

    #[test]
    fn enter_on_run_step_builtin_runs_it() {
        let mut app = app();
        type_str(&mut app, "split");
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 4, input: String::new() });
    }

    #[test]
    fn enter_with_no_results_does_nothing() {
        let mut app = app();
        type_str(&mut app, "zzzzzz");
        assert!(app.ranked.is_empty());
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Continue);
    }

    #[test]
    fn prompt_flow_prefills_edits_and_submits() {
        let mut app = app();
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.mode, Mode::Prompt { item: 2, label: "Rename tab".into(), input: "Claude".into() });
        app.handle_key(ctrl('u'), &ctx());
        type_str(&mut app, "Logs");
        app.handle_key(key(KeyCode::Backspace), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 2, input: "Log".into() });
    }

    #[test]
    fn esc_in_prompt_returns_to_list_with_query_intact() {
        let mut app = app();
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Esc), &ctx()), Command::Continue);
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.query, "rename");
    }

    #[test]
    fn confirm_accepts_enter_or_y_and_rejects_esc_or_n() {
        let mut app = app();
        type_str(&mut app, "close");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.mode, Mode::Confirm { item: 3, question: "Close tab \"Claude\"?".into() });
        assert_eq!(app.handle_key(ch('x'), &ctx()), Command::Continue);
        assert_eq!(app.handle_key(ch('n'), &ctx()), Command::Continue);
        assert_eq!(app.mode, Mode::List);

        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(ch('y'), &ctx()), Command::Run { item: 3, input: String::new() });

        app.mode = Mode::List;
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Esc), &ctx()), Command::Continue);
        assert_eq!(app.mode, Mode::List);

        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 3, input: String::new() });
    }

    #[test]
    fn esc_in_list_and_ctrl_c_anywhere_quit() {
        let mut app = app();
        assert_eq!(app.handle_key(key(KeyCode::Esc), &ctx()), Command::Quit);
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(ctrl('c'), &ctx()), Command::Quit);
    }

    #[test]
    fn failure_in_list_keeps_list_and_shows_error() {
        let mut app = app();
        app.fail("tab.focus: tab w1:t9 not found (tab_not_found)".into());
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.status, Some(Status::Error("tab.focus: tab w1:t9 not found (tab_not_found)".into())));
    }

    #[test]
    fn failure_in_confirm_returns_to_list_and_in_prompt_stays() {
        let mut app = app();
        type_str(&mut app, "close");
        app.handle_key(key(KeyCode::Enter), &ctx());
        app.fail("boom".into());
        assert_eq!(app.mode, Mode::List);

        app.handle_key(ctrl('u'), &ctx());
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        app.fail("label cannot be empty".into());
        assert!(matches!(app.mode, Mode::Prompt { .. }));
        assert_eq!(app.status, Some(Status::Error("label cannot be empty".into())));
    }

    #[test]
    fn frecency_scores_order_the_empty_query() {
        let items = vec![
            Item::new(Kind::Workspace, "ws:a", "A", Action::FocusWorkspace("a".into())),
            Item::new(Kind::Workspace, "ws:b", "B", Action::FocusWorkspace("b".into())),
        ];
        let app = App::new(items, HashMap::from([("ws:b".to_string(), 4.0)]), Some(Status::Info("hi".into())));
        assert_eq!(selected_title(&app), "B");
        assert_eq!(app.status, Some(Status::Info("hi".into())));
    }
}
