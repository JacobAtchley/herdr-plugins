//! Runner state and key handling, independent of the terminal.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::launch::Target;
use crate::matcher::{Ranked, ScriptMatcher};
use crate::project::Project;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Continue,
    Quit,
    /// Run `project.scripts[script]` in `target`.
    Run {
        script: usize,
        target: Target,
    },
}

pub struct App {
    pub project: Project,
    pub query: String,
    pub ranked: Vec<Ranked>,
    /// Index into `ranked`.
    pub selected: usize,
    pub error: Option<String>,
    matcher: ScriptMatcher,
}

impl App {
    pub fn new(project: Project) -> Self {
        let mut app = Self {
            project,
            query: String::new(),
            ranked: Vec::new(),
            selected: 0,
            error: None,
            matcher: ScriptMatcher::default(),
        };
        app.refilter();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Command {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Command::Quit,
            KeyCode::Char('c') if ctrl => return Command::Quit,
            KeyCode::Enter => return self.run(Target::Tab),
            KeyCode::Char('v') if ctrl => return self.run(Target::SplitRight),
            KeyCode::Char('x') if ctrl => return self.run(Target::SplitDown),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('p') if ctrl => self.move_selection(-1),
            KeyCode::Char('n') if ctrl => self.move_selection(1),
            KeyCode::Char('u') if ctrl => self.edit(String::clear),
            KeyCode::Backspace => self.edit(|q| {
                q.pop();
            }),
            KeyCode::Char(c) if !ctrl => self.edit(|q| q.push(c)),
            _ => {}
        }
        Command::Continue
    }

    pub fn fail(&mut self, message: String) {
        self.error = Some(message);
    }

    fn run(&self, target: Target) -> Command {
        match self.ranked.get(self.selected) {
            Some(ranked) => Command::Run { script: ranked.index, target },
            None => Command::Continue,
        }
    }

    fn edit(&mut self, change: impl FnOnce(&mut String)) {
        change(&mut self.query);
        self.error = None;
        self.refilter();
    }

    fn refilter(&mut self) {
        self.ranked = self.matcher.rank(&self.query, &self.project.scripts);
        self.selected = 0;
    }

    /// Wraps around at either end.
    fn move_selection(&mut self, delta: isize) {
        let len = self.ranked.len();
        if len > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::project::{Manager, Script};
    use std::path::PathBuf;

    pub fn project() -> Project {
        let scripts = [("start:web", "nx serve web"), ("start:api", "nx serve api"), ("lint", "nx run-many -t lint")];
        Project {
            dir: PathBuf::from("/repo"),
            name: "mono".into(),
            manager: Manager::Pnpm,
            scripts: scripts.iter().map(|(n, c)| Script { name: n.to_string(), command: c.to_string() }).collect(),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn type_str(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn enter_runs_the_selected_script_in_a_tab() {
        let mut app = App::new(project());
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Run { script: 0, target: Target::Tab });
    }

    #[test]
    fn ctrl_v_and_ctrl_x_run_in_splits() {
        let mut app = App::new(project());
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.handle_key(ctrl('v')), Command::Run { script: 1, target: Target::SplitRight });
        assert_eq!(app.handle_key(ctrl('x')), Command::Run { script: 1, target: Target::SplitDown });
    }

    #[test]
    fn typing_filters_and_resets_selection() {
        let mut app = App::new(project());
        app.handle_key(key(KeyCode::Down));
        type_str(&mut app, "lint");
        assert_eq!(app.selected, 0);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Run { script: 2, target: Target::Tab });
    }

    #[test]
    fn selection_wraps() {
        let mut app = App::new(project());
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 2);
        app.handle_key(ctrl('n'));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn no_matches_makes_enter_a_no_op() {
        let mut app = App::new(project());
        type_str(&mut app, "zzzz");
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Continue);
    }

    #[test]
    fn editing_clears_the_error_and_ctrl_u_clears_the_query() {
        let mut app = App::new(project());
        type_str(&mut app, "ap");
        app.fail("boom".into());
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!((app.query.as_str(), app.error.as_deref()), ("a", None));
        app.handle_key(ctrl('u'));
        assert_eq!(app.ranked.len(), 3);
    }

    #[test]
    fn esc_and_ctrl_c_quit() {
        let mut app = App::new(project());
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Command::Quit);
        assert_eq!(app.handle_key(ctrl('c')), Command::Quit);
    }
}
