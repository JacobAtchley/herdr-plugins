//! Glance state and key handling, independent of the terminal and of git:
//! keys produce an `Op`, and the op's `Outcome` is fed back through `apply`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::git::{Branch, Stash, StashVerb};
use crate::status::{Entry, Section, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Status,
    Diff { title: String, lines: Vec<String>, scroll: usize },
    Branches { branches: Vec<Branch>, query: String, matches: Vec<usize>, selected: usize },
    Stashes { stashes: Vec<Stash>, selected: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Commit,
    Stash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Prompt { purpose: Purpose, input: String },
    Confirm { question: String, op: Op },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Info(String),
    Error(String),
}

/// Work for git, run by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Refresh,
    Stage(String),
    Unstage { path: String, unborn: bool },
    StageAll,
    UnstageAll { unborn: bool },
    Commit(String),
    Diff(Entry),
    ListBranches,
    Switch { name: String, create: bool },
    ListStashes,
    StashPush(String),
    Stash(StashVerb, usize),
}

/// What an `Op` produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Snapshot(Snapshot),
    Diff { title: String, text: String },
    Branches(Vec<Branch>),
    Stashes(Vec<Stash>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Continue,
    Quit,
    Run(Op),
}

pub struct App {
    pub repo_name: String,
    pub snapshot: Snapshot,
    /// Index into `snapshot.entries`.
    pub selected: usize,
    pub view: View,
    pub mode: Mode,
    pub status: Option<Status>,
    /// Rows available for the scrolling body; set by the caller on resize.
    pub page: usize,
}

impl App {
    pub fn new(repo_name: String, snapshot: Snapshot) -> Self {
        Self { repo_name, snapshot, selected: 0, view: View::Status, mode: Mode::Normal, status: None, page: 10 }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.snapshot.entries.get(self.selected)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Command {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Command::Quit;
        }
        // A notice describes the last action; the next key moves on from it.
        self.status = None;
        match self.mode {
            Mode::Prompt { .. } => return self.prompt_key(key, ctrl),
            Mode::Confirm { .. } => return self.confirm_key(key),
            Mode::Normal => {}
        }
        match self.view {
            View::Status => self.status_key(key, ctrl),
            View::Diff { .. } => self.diff_key(key, ctrl),
            View::Branches { .. } => self.branches_key(key, ctrl),
            View::Stashes { .. } => self.stashes_key(key, ctrl),
        }
    }

    /// Records a finished op. Snapshots return to the status view (keeping the
    /// cursor position, so repeated staging walks down the list); lists and
    /// diffs open their view.
    pub fn apply(&mut self, outcome: Outcome, notice: Option<String>) {
        self.mode = Mode::Normal;
        self.status = notice.map(Status::Info);
        match outcome {
            Outcome::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                self.selected = self.selected.min(self.snapshot.entries.len().saturating_sub(1));
                self.view = View::Status;
            }
            Outcome::Diff { title, text } => {
                let lines = if text.is_empty() {
                    vec!["(no textual changes)".to_string()]
                } else {
                    text.lines().map(str::to_string).collect()
                };
                self.view = View::Diff { title, lines, scroll: 0 };
            }
            Outcome::Branches(branches) => {
                let matches = (0..branches.len()).collect();
                self.view = View::Branches { branches, query: String::new(), matches, selected: 0 };
            }
            Outcome::Stashes(stashes) => {
                let selected = match &self.view {
                    View::Stashes { selected, .. } => (*selected).min(stashes.len().saturating_sub(1)),
                    _ => 0,
                };
                self.view = View::Stashes { stashes, selected };
            }
        }
    }

    /// Shows an op failure. A confirm step is abandoned; a prompt stays open
    /// so its input (e.g. a commit message) isn't lost.
    pub fn fail(&mut self, message: String) {
        if matches!(self.mode, Mode::Confirm { .. }) {
            self.mode = Mode::Normal;
        }
        self.status = Some(Status::Error(message));
    }

    fn status_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let unborn = self.snapshot.is_unborn();
        let last = self.snapshot.entries.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Command::Quit,
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('p') if ctrl => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
            KeyCode::Char('n') if ctrl => self.selected = (self.selected + 1).min(last),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.selected = last,
            KeyCode::Char('r') => return Command::Run(Op::Refresh),
            KeyCode::Char(' ') => {
                if let Some(entry) = self.selected_entry() {
                    let path = entry.path.clone();
                    return Command::Run(match entry.section {
                        Section::Staged => Op::Unstage { path, unborn },
                        _ => Op::Stage(path),
                    });
                }
            }
            KeyCode::Char('a') if !self.snapshot.entries.is_empty() => return Command::Run(Op::StageAll),
            KeyCode::Char('u') if self.snapshot.has_staged() => return Command::Run(Op::UnstageAll { unborn }),
            KeyCode::Enter | KeyCode::Char('d') => {
                if let Some(entry) = self.selected_entry() {
                    return Command::Run(Op::Diff(entry.clone()));
                }
            }
            KeyCode::Char('c') => {
                if self.snapshot.has_staged() {
                    self.mode = Mode::Prompt { purpose: Purpose::Commit, input: String::new() };
                } else {
                    self.status = Some(Status::Error("nothing staged to commit".into()));
                }
            }
            KeyCode::Char('b') => return Command::Run(Op::ListBranches),
            KeyCode::Char('z') => return Command::Run(Op::ListStashes),
            KeyCode::Char('S') => {
                if self.snapshot.entries.is_empty() {
                    self.status = Some(Status::Error("no local changes to stash".into()));
                } else {
                    self.mode = Mode::Prompt { purpose: Purpose::Stash, input: String::new() };
                }
            }
            _ => {}
        }
        Command::Continue
    }

    fn diff_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let page = self.page.max(1);
        let View::Diff { lines, scroll, .. } = &mut self.view else { return Command::Continue };
        let last = lines.len().saturating_sub(page);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter | KeyCode::Char('d') => self.view = View::Status,
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::Char('p') if ctrl => *scroll = scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *scroll = (*scroll + 1).min(last),
            KeyCode::Char('n') if ctrl => *scroll = (*scroll + 1).min(last),
            KeyCode::PageUp | KeyCode::Char('b') => *scroll = scroll.saturating_sub(page),
            KeyCode::PageDown | KeyCode::Char(' ') => *scroll = (*scroll + page).min(last),
            KeyCode::Home | KeyCode::Char('g') => *scroll = 0,
            KeyCode::End | KeyCode::Char('G') => *scroll = last,
            _ => {}
        }
        Command::Continue
    }

    fn branches_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let View::Branches { branches, query, matches, selected } = &mut self.view else { return Command::Continue };
        let last = matches.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => self.view = View::Status,
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Char('p') if ctrl => *selected = selected.saturating_sub(1),
            KeyCode::Down => *selected = (*selected + 1).min(last),
            KeyCode::Char('n') if ctrl => *selected = (*selected + 1).min(last),
            KeyCode::Enter => {
                if let Some(&index) = matches.get(*selected) {
                    let branch = &branches[index];
                    if branch.current {
                        self.view = View::Status;
                        return Command::Continue;
                    }
                    return Command::Run(Op::Switch { name: branch.name.clone(), create: false });
                }
                let name = query.trim();
                if !name.is_empty() {
                    return Command::Run(Op::Switch { name: name.to_string(), create: true });
                }
            }
            KeyCode::Char('u') if ctrl => {
                query.clear();
                refilter(branches, query, matches, selected);
            }
            KeyCode::Backspace => {
                query.pop();
                refilter(branches, query, matches, selected);
            }
            KeyCode::Char(c) if !ctrl => {
                query.push(c);
                refilter(branches, query, matches, selected);
            }
            _ => {}
        }
        Command::Continue
    }

    fn stashes_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let View::Stashes { stashes, selected } = &mut self.view else { return Command::Continue };
        let last = stashes.len().saturating_sub(1);
        let target = stashes.get(*selected).map(|s| s.index);
        match (key.code, target) {
            (KeyCode::Esc | KeyCode::Char('q'), _) => self.view = View::Status,
            (KeyCode::Up | KeyCode::Char('k'), _) => *selected = selected.saturating_sub(1),
            (KeyCode::Char('p'), _) if ctrl => *selected = selected.saturating_sub(1),
            (KeyCode::Down | KeyCode::Char('j'), _) => *selected = (*selected + 1).min(last),
            (KeyCode::Char('n'), _) if ctrl => *selected = (*selected + 1).min(last),
            (KeyCode::Enter | KeyCode::Char('p'), Some(index)) => {
                return Command::Run(Op::Stash(StashVerb::Pop, index));
            }
            (KeyCode::Char('a'), Some(index)) => return Command::Run(Op::Stash(StashVerb::Apply, index)),
            (KeyCode::Char('x'), Some(index)) => {
                self.mode = Mode::Confirm {
                    question: format!("Drop stash@{{{index}}}? It cannot be recovered from here."),
                    op: Op::Stash(StashVerb::Drop, index),
                };
            }
            _ => {}
        }
        Command::Continue
    }

    fn prompt_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let Mode::Prompt { purpose, input } = &mut self.mode else { return Command::Continue };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let text = input.trim().to_string();
                return match purpose {
                    Purpose::Commit if text.is_empty() => {
                        self.status = Some(Status::Error("commit message cannot be empty".into()));
                        Command::Continue
                    }
                    Purpose::Commit => Command::Run(Op::Commit(text)),
                    Purpose::Stash => Command::Run(Op::StashPush(text)),
                };
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

    fn confirm_key(&mut self, key: KeyEvent) -> Command {
        let Mode::Confirm { op, .. } = &self.mode else { return Command::Continue };
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => Command::Run(op.clone()),
            KeyCode::Esc | KeyCode::Char('n') => {
                self.mode = Mode::Normal;
                Command::Continue
            }
            _ => Command::Continue,
        }
    }
}

/// Case-insensitive substring match: branch lists are short and names are
/// typed exactly, so fuzzy ranking would add weight without helping.
fn refilter(branches: &[Branch], query: &str, matches: &mut Vec<usize>, selected: &mut usize) {
    let needle = query.to_lowercase();
    matches.clear();
    matches
        .extend(branches.iter().enumerate().filter(|(_, b)| b.name.to_lowercase().contains(&needle)).map(|(i, _)| i));
    *selected = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::BranchInfo;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn entry(section: Section, code: char, path: &str) -> Entry {
        Entry { section, code, path: path.into(), orig: None }
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            branch: BranchInfo { head: Some("main".into()), oid: Some("abc1234".into()), ..Default::default() },
            stashes: 0,
            entries: vec![
                entry(Section::Staged, 'M', "src/app.rs"),
                entry(Section::Unstaged, 'M', "README.md"),
                entry(Section::Untracked, '?', "notes.txt"),
            ],
        }
    }

    fn app() -> App {
        App::new("repo".into(), snapshot())
    }

    fn branch(name: &str, current: bool) -> Branch {
        Branch { name: name.into(), current, age: "1 day ago".into(), track: String::new() }
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle_key(ch(c));
        }
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut app = app();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
        app.handle_key(ch('j'));
        app.handle_key(ctrl('n'));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 2);
        app.handle_key(ch('k'));
        assert_eq!(app.selected, 1);
        app.handle_key(ch('g'));
        assert_eq!(app.selected, 0);
        app.handle_key(ch('G'));
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn space_stages_unstaged_and_unstages_staged() {
        let mut app = app();
        assert_eq!(app.handle_key(ch(' ')), Command::Run(Op::Unstage { path: "src/app.rs".into(), unborn: false }));
        app.handle_key(ch('j'));
        assert_eq!(app.handle_key(ch(' ')), Command::Run(Op::Stage("README.md".into())));
        app.handle_key(ch('j'));
        assert_eq!(app.handle_key(ch(' ')), Command::Run(Op::Stage("notes.txt".into())));
    }

    #[test]
    fn unborn_repo_unstages_by_removing_from_the_index() {
        let mut snap = snapshot();
        snap.branch.oid = None;
        let mut app = App::new("repo".into(), snap);
        assert_eq!(app.handle_key(ch(' ')), Command::Run(Op::Unstage { path: "src/app.rs".into(), unborn: true }));
        assert_eq!(app.handle_key(ch('u')), Command::Run(Op::UnstageAll { unborn: true }));
    }

    #[test]
    fn bulk_keys_need_something_to_act_on() {
        let mut app = App::new("repo".into(), Snapshot::default());
        assert_eq!(app.handle_key(ch('a')), Command::Continue);
        assert_eq!(app.handle_key(ch('u')), Command::Continue);
        assert_eq!(app.handle_key(ch(' ')), Command::Continue);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Continue);
        app.handle_key(ch('S'));
        assert_eq!(app.status, Some(Status::Error("no local changes to stash".into())));

        let mut app = self::app();
        assert_eq!(app.handle_key(ch('a')), Command::Run(Op::StageAll));
        assert_eq!(app.handle_key(ch('u')), Command::Run(Op::UnstageAll { unborn: false }));
    }

    #[test]
    fn enter_opens_diff_and_diff_view_scrolls_within_bounds() {
        let mut app = app();
        app.handle_key(ch('j'));
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Command::Run(Op::Diff(entry(Section::Unstaged, 'M', "README.md")))
        );

        app.page = 3;
        let text = (0..10).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        app.apply(Outcome::Diff { title: "README.md".into(), text }, None);
        let scroll = |app: &App| match &app.view {
            View::Diff { scroll, .. } => *scroll,
            other => panic!("{other:?}"),
        };
        assert_eq!(scroll(&app), 0);
        app.handle_key(ch('j'));
        assert_eq!(scroll(&app), 1);
        app.handle_key(ch(' '));
        assert_eq!(scroll(&app), 4);
        app.handle_key(ch('G'));
        assert_eq!(scroll(&app), 7);
        app.handle_key(ch('j'));
        assert_eq!(scroll(&app), 7);
        app.handle_key(ch('b'));
        assert_eq!(scroll(&app), 4);
        app.handle_key(ch('g'));
        assert_eq!(scroll(&app), 0);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.view, View::Status);
        assert_eq!(app.selected, 1, "returning from a diff keeps the cursor");
    }

    #[test]
    fn empty_diff_says_so() {
        let mut app = app();
        app.apply(Outcome::Diff { title: "x".into(), text: String::new() }, None);
        assert!(matches!(&app.view, View::Diff { lines, .. } if lines == &["(no textual changes)"]));
    }

    #[test]
    fn commit_prompt_requires_staged_changes_and_a_message() {
        let mut app =
            App::new("repo".into(), Snapshot { entries: vec![entry(Section::Unstaged, 'M', "a")], ..snapshot() });
        app.handle_key(ch('c'));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status, Some(Status::Error("nothing staged to commit".into())));

        let mut app = self::app();
        app.handle_key(ch('c'));
        assert_eq!(app.mode, Mode::Prompt { purpose: Purpose::Commit, input: String::new() });
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Continue);
        assert_eq!(app.status, Some(Status::Error("commit message cannot be empty".into())));

        type_str(&mut app, "fix: typoo");
        app.handle_key(key(KeyCode::Backspace));
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Run(Op::Commit("fix: typo".into())));
    }

    #[test]
    fn prompt_keys_are_text_not_commands() {
        let mut app = app();
        app.handle_key(ch('c'));
        type_str(&mut app, "qjk ");
        assert_eq!(app.mode, Mode::Prompt { purpose: Purpose::Commit, input: "qjk ".into() });
        app.handle_key(ctrl('u'));
        assert_eq!(app.mode, Mode::Prompt { purpose: Purpose::Commit, input: String::new() });
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn failed_commit_keeps_the_prompt_and_success_closes_it() {
        let mut app = app();
        app.handle_key(ch('c'));
        type_str(&mut app, "msg");
        app.handle_key(key(KeyCode::Enter));
        app.fail("pre-commit hook failed".into());
        assert_eq!(app.mode, Mode::Prompt { purpose: Purpose::Commit, input: "msg".into() });
        assert_eq!(app.status, Some(Status::Error("pre-commit hook failed".into())));

        app.apply(Outcome::Snapshot(Snapshot::default()), Some("committed abc1234".into()));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status, Some(Status::Info("committed abc1234".into())));
    }

    #[test]
    fn stash_prompt_allows_an_empty_message() {
        let mut app = app();
        app.handle_key(ch('S'));
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Run(Op::StashPush(String::new())));
    }

    #[test]
    fn new_snapshot_clamps_selection_and_returns_to_status() {
        let mut app = app();
        app.handle_key(ch('G'));
        app.apply(Outcome::Snapshot(Snapshot { entries: vec![entry(Section::Staged, 'M', "a")], ..snapshot() }), None);
        assert_eq!(app.selected, 0);
        app.apply(Outcome::Snapshot(Snapshot::default()), None);
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_entry(), None);
    }

    #[test]
    fn branch_view_filters_switches_and_creates() {
        let mut app = app();
        assert_eq!(app.handle_key(ch('b')), Command::Run(Op::ListBranches));
        app.apply(
            Outcome::Branches(vec![branch("main", true), branch("feature/Login", false), branch("fix-bug", false)]),
            None,
        );
        assert_eq!(app.handle_key(key(KeyCode::Down)), Command::Continue);
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Command::Run(Op::Switch { name: "feature/Login".into(), create: false })
        );

        type_str(&mut app, "LOG");
        let View::Branches { matches, selected, .. } = &app.view else { panic!() };
        assert_eq!((matches.as_slice(), *selected), ([1].as_slice(), 0));

        app.handle_key(ctrl('u'));
        type_str(&mut app, "new-thing ");
        assert_eq!(
            app.handle_key(key(KeyCode::Enter)),
            Command::Run(Op::Switch { name: "new-thing".into(), create: true })
        );

        app.handle_key(ctrl('u'));
        type_str(&mut app, "q");
        assert!(matches!(&app.view, View::Branches { query, .. } if query == "q"), "q is filter text here");
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.view, View::Status);
    }

    #[test]
    fn enter_on_the_current_branch_just_goes_back() {
        let mut app = app();
        app.apply(Outcome::Branches(vec![branch("main", true)]), None);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Continue);
        assert_eq!(app.view, View::Status);
    }

    #[test]
    fn stash_view_pops_applies_and_confirms_drop() {
        let stashes = vec![
            Stash { index: 0, age: "now".into(), subject: "WIP on main".into() },
            Stash { index: 1, age: "1 day ago".into(), subject: "On main: old".into() },
        ];
        let mut app = app();
        assert_eq!(app.handle_key(ch('z')), Command::Run(Op::ListStashes));
        app.apply(Outcome::Stashes(stashes.clone()), None);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Run(Op::Stash(StashVerb::Pop, 0)));
        app.handle_key(ch('j'));
        assert_eq!(app.handle_key(ch('a')), Command::Run(Op::Stash(StashVerb::Apply, 1)));

        app.handle_key(ch('x'));
        assert!(matches!(app.mode, Mode::Confirm { .. }));
        assert_eq!(app.handle_key(ch('n')), Command::Continue);
        assert_eq!(app.mode, Mode::Normal);
        app.handle_key(ch('x'));
        assert_eq!(app.handle_key(ch('y')), Command::Run(Op::Stash(StashVerb::Drop, 1)));

        // A refreshed list after dropping keeps the view and a valid cursor.
        app.apply(Outcome::Stashes(stashes[..1].to_vec()), Some("dropped stash@{1}".into()));
        assert!(matches!(app.view, View::Stashes { selected: 0, .. }));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn failed_confirm_returns_to_normal() {
        let mut app = app();
        app.apply(Outcome::Stashes(vec![Stash { index: 0, age: "now".into(), subject: "s".into() }]), None);
        app.handle_key(ch('x'));
        app.handle_key(ch('y'));
        app.fail("boom".into());
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status, Some(Status::Error("boom".into())));
    }

    #[test]
    fn empty_stash_list_ignores_actions() {
        let mut app = app();
        app.apply(Outcome::Stashes(vec![]), None);
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::Continue);
        assert_eq!(app.handle_key(ch('x')), Command::Continue);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn any_key_clears_the_last_notice_and_quit_keys_work() {
        let mut app = app();
        app.status = Some(Status::Info("committed".into()));
        app.handle_key(ch('j'));
        assert_eq!(app.status, None);
        assert_eq!(app.handle_key(ch('q')), Command::Quit);
        assert_eq!(app.handle_key(key(KeyCode::Esc)), Command::Quit);
        app.handle_key(ch('c'));
        assert_eq!(app.handle_key(ctrl('c')), Command::Quit);
    }

    #[test]
    fn refresh_key() {
        assert_eq!(app().handle_key(ch('r')), Command::Run(Op::Refresh));
    }
}
