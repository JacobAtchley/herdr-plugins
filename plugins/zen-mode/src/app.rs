//! Configure UI state and key handling, independent of the terminal.

use herdr_client::models::{Tab, Workspace};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::state::{MAX_TABS_PER_WORKSPACE, MAX_WORKSPACES, ZenState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Continue,
    Quit,
    /// Focus a shortlist tab and close the popup.
    FocusTab {
        tab_id: String,
    },
    /// Focus a shortlist workspace (no tabs picked yet) and close the popup.
    FocusWorkspace {
        workspace_id: String,
    },
    /// Activate or deactivate zen from the switcher.
    ToggleZen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Overview,
    PickWorkspace { target: WorkspaceTarget },
    PickTab { target: TabTarget },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Add,
    Replace { index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabTarget {
    Add { workspace_index: usize },
    Replace { workspace_index: usize, tab_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Workspace { index: usize },
    Tab { workspace_index: usize, tab_index: usize },
    AddWorkspace,
    AddTab { workspace_index: usize },
}

pub struct App {
    pub state: ZenState,
    pub workspaces: Vec<Workspace>,
    pub tabs: Vec<Tab>,
    pub mode: Mode,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub query: String,
    pub picker_indexes: Vec<usize>,
    pub status: Option<String>,
    dirty: bool,
}

impl App {
    pub fn new(state: ZenState, workspaces: Vec<Workspace>, tabs: Vec<Tab>) -> Self {
        let mut app = Self {
            state,
            workspaces,
            tabs,
            mode: Mode::Overview,
            rows: Vec::new(),
            selected: 0,
            query: String::new(),
            picker_indexes: Vec::new(),
            status: None,
            dirty: false,
        };
        app.rebuild_overview();
        app
    }

    pub fn take_dirty(&mut self) -> bool {
        let dirty = self.dirty;
        self.dirty = false;
        dirty
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Command {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Command::Quit;
        }
        match self.mode {
            Mode::Overview => self.overview_key(key, ctrl),
            Mode::PickWorkspace { .. } => self.pick_workspace_key(key, ctrl),
            Mode::PickTab { .. } => self.pick_tab_key(key, ctrl),
        }
    }

    fn overview_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        match key.code {
            KeyCode::Esc => return Command::Quit,
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('p') if ctrl => self.move_selection(-1),
            KeyCode::Char('n') if ctrl => self.move_selection(1),
            KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Enter => return self.activate_row(),
            KeyCode::Char('z') => return Command::ToggleZen,
            KeyCode::Char('r') => self.edit_row(),
            KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => self.delete_row(),
            KeyCode::Char('a') => {
                if self.state.workspaces.len() < MAX_WORKSPACES {
                    self.enter_pick_workspace(WorkspaceTarget::Add);
                } else {
                    self.status = Some(format!("at most {MAX_WORKSPACES} workspaces"));
                }
            }
            KeyCode::Char('t') => {
                if let Some(workspace_index) = self.selected_workspace_index() {
                    if self.state.workspaces[workspace_index].tab_ids.len() < MAX_TABS_PER_WORKSPACE {
                        self.enter_pick_tab(TabTarget::Add { workspace_index });
                    } else {
                        self.status = Some(format!("at most {MAX_TABS_PER_WORKSPACE} tabs per workspace"));
                    }
                } else {
                    self.status = Some("select a workspace first".into());
                }
            }
            _ => {}
        }
        Command::Continue
    }

    fn pick_workspace_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Overview;
                self.query.clear();
                self.status = None;
                self.rebuild_overview();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('p') if ctrl => self.move_selection(-1),
            KeyCode::Char('n') if ctrl => self.move_selection(1),
            KeyCode::Enter => self.confirm_workspace(),
            KeyCode::Char('u') if ctrl => {
                self.query.clear();
                self.refilter_workspaces();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter_workspaces();
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.refilter_workspaces();
            }
            _ => {}
        }
        Command::Continue
    }

    fn pick_tab_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Overview;
                self.query.clear();
                self.status = None;
                self.rebuild_overview();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('p') if ctrl => self.move_selection(-1),
            KeyCode::Char('n') if ctrl => self.move_selection(1),
            KeyCode::Enter => self.confirm_tab(),
            KeyCode::Char('u') if ctrl => {
                self.query.clear();
                self.refilter_tabs();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter_tabs();
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.refilter_tabs();
            }
            _ => {}
        }
        Command::Continue
    }

    /// Enter: navigate to a shortlist item, or open an add picker.
    fn activate_row(&mut self) -> Command {
        let Some(row) = self.rows.get(self.selected).cloned() else {
            return Command::Continue;
        };
        match row {
            Row::Tab { workspace_index, tab_index } => {
                let Some(tab_id) =
                    self.state.workspaces.get(workspace_index).and_then(|slot| slot.tab_ids.get(tab_index))
                else {
                    self.status = Some("tab missing from shortlist".into());
                    return Command::Continue;
                };
                if !self.tabs.iter().any(|tab| tab.tab_id == *tab_id) {
                    self.status = Some(format!("tab {tab_id} unavailable"));
                    return Command::Continue;
                }
                Command::FocusTab { tab_id: tab_id.clone() }
            }
            Row::Workspace { index } => {
                let Some(slot) = self.state.workspaces.get(index) else {
                    return Command::Continue;
                };
                if let Some(tab_id) =
                    slot.tab_ids.iter().find(|tab_id| self.tabs.iter().any(|tab| tab.tab_id == **tab_id))
                {
                    Command::FocusTab { tab_id: tab_id.clone() }
                } else if self.workspaces.iter().any(|workspace| workspace.workspace_id == slot.workspace_id) {
                    Command::FocusWorkspace { workspace_id: slot.workspace_id.clone() }
                } else {
                    self.status = Some(format!("workspace {} unavailable", slot.workspace_id));
                    Command::Continue
                }
            }
            Row::AddWorkspace => {
                self.enter_pick_workspace(WorkspaceTarget::Add);
                Command::Continue
            }
            Row::AddTab { workspace_index } => {
                self.enter_pick_tab(TabTarget::Add { workspace_index });
                Command::Continue
            }
        }
    }

    /// `r`: replace the selected workspace or tab in the shortlist.
    fn edit_row(&mut self) {
        let Some(row) = self.rows.get(self.selected).cloned() else {
            return;
        };
        match row {
            Row::Workspace { index } => self.enter_pick_workspace(WorkspaceTarget::Replace { index }),
            Row::Tab { workspace_index, tab_index } => {
                self.enter_pick_tab(TabTarget::Replace { workspace_index, tab_index });
            }
            Row::AddWorkspace => self.enter_pick_workspace(WorkspaceTarget::Add),
            Row::AddTab { workspace_index } => self.enter_pick_tab(TabTarget::Add { workspace_index }),
        }
    }

    fn delete_row(&mut self) {
        let Some(row) = self.rows.get(self.selected).cloned() else {
            return;
        };
        match row {
            Row::Workspace { index } => {
                self.state.remove_workspace(index);
                self.dirty = true;
                self.status = Some("workspace removed".into());
            }
            Row::Tab { workspace_index, tab_index } => {
                self.state.remove_tab(workspace_index, tab_index);
                self.dirty = true;
                self.status = Some("tab removed".into());
            }
            Row::AddWorkspace | Row::AddTab { .. } => {}
        }
        self.rebuild_overview();
    }

    fn enter_pick_workspace(&mut self, target: WorkspaceTarget) {
        self.mode = Mode::PickWorkspace { target };
        self.query.clear();
        self.status = None;
        self.refilter_workspaces();
    }

    fn enter_pick_tab(&mut self, target: TabTarget) {
        self.mode = Mode::PickTab { target };
        self.query.clear();
        self.status = None;
        self.refilter_tabs();
    }

    fn confirm_workspace(&mut self) {
        let Mode::PickWorkspace { target } = self.mode else {
            return;
        };
        let Some(&index) = self.picker_indexes.get(self.selected) else {
            self.status = Some("no workspace selected".into());
            return;
        };
        let workspace_id = self.workspaces[index].workspace_id.clone();
        let result = match target {
            WorkspaceTarget::Add => self.state.add_workspace(workspace_id),
            WorkspaceTarget::Replace { index } => {
                self.state.set_workspace(index, workspace_id);
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.dirty = true;
                self.mode = Mode::Overview;
                self.query.clear();
                self.status = Some("workspace updated".into());
                self.rebuild_overview();
            }
            Err(err) => self.status = Some(err),
        }
    }

    fn confirm_tab(&mut self) {
        let Mode::PickTab { target } = self.mode else {
            return;
        };
        let Some(&index) = self.picker_indexes.get(self.selected) else {
            self.status = Some("no tab selected".into());
            return;
        };
        let tab_id = self.tabs[index].tab_id.clone();
        let result = match target {
            TabTarget::Add { workspace_index } => self.state.add_tab(workspace_index, tab_id),
            TabTarget::Replace { workspace_index, tab_index } => self.state.set_tab(workspace_index, tab_index, tab_id),
        };
        match result {
            Ok(()) => {
                self.dirty = true;
                self.mode = Mode::Overview;
                self.query.clear();
                self.status = Some("tab updated".into());
                self.rebuild_overview();
            }
            Err(err) => self.status = Some(err),
        }
    }

    fn selected_workspace_index(&self) -> Option<usize> {
        match self.rows.get(self.selected)? {
            Row::Workspace { index } | Row::AddTab { workspace_index: index } => Some(*index),
            Row::Tab { workspace_index, .. } => Some(*workspace_index),
            Row::AddWorkspace => None,
        }
    }

    fn rebuild_overview(&mut self) {
        self.rows.clear();
        for (index, slot) in self.state.workspaces.iter().enumerate() {
            self.rows.push(Row::Workspace { index });
            for tab_index in 0..slot.tab_ids.len() {
                self.rows.push(Row::Tab { workspace_index: index, tab_index });
            }
            if slot.tab_ids.len() < MAX_TABS_PER_WORKSPACE {
                self.rows.push(Row::AddTab { workspace_index: index });
            }
        }
        if self.state.workspaces.len() < MAX_WORKSPACES {
            self.rows.push(Row::AddWorkspace);
        }
        if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
    }

    fn refilter_workspaces(&mut self) {
        let query = self.query.to_lowercase();
        self.picker_indexes = self
            .workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| {
                let already = self.state.workspaces.iter().any(|slot| slot.workspace_id == workspace.workspace_id);
                if already {
                    // Allow re-picking when replacing that same slot.
                    if let Mode::PickWorkspace { target: WorkspaceTarget::Replace { index } } = self.mode {
                        if self.state.workspaces.get(index).map(|slot| slot.workspace_id.as_str())
                            == Some(workspace.workspace_id.as_str())
                        {
                            // fall through
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                if query.is_empty() {
                    return true;
                }
                let label = workspace_label(workspace).to_lowercase();
                label.contains(&query) || workspace.workspace_id.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
    }

    fn refilter_tabs(&mut self) {
        let workspace_index = match self.mode {
            Mode::PickTab { target: TabTarget::Add { workspace_index } }
            | Mode::PickTab { target: TabTarget::Replace { workspace_index, .. } } => workspace_index,
            _ => 0,
        };
        let workspace_id =
            self.state.workspaces.get(workspace_index).map(|slot| slot.workspace_id.as_str()).unwrap_or("");
        let selected_tabs =
            self.state.workspaces.get(workspace_index).map(|slot| slot.tab_ids.as_slice()).unwrap_or(&[]);
        let replace_tab_id = match self.mode {
            Mode::PickTab { target: TabTarget::Replace { tab_index, .. } } => {
                selected_tabs.get(tab_index).map(String::as_str)
            }
            _ => None,
        };
        let query = self.query.to_lowercase();
        self.picker_indexes = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| {
                if tab.workspace_id != workspace_id {
                    return false;
                }
                if selected_tabs.iter().any(|id| id == &tab.tab_id) && replace_tab_id != Some(tab.tab_id.as_str()) {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                let label = tab_label(tab).to_lowercase();
                label.contains(&query) || tab.tab_id.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
    }

    fn move_selection(&mut self, delta: isize) {
        let len = match self.mode {
            Mode::Overview => self.rows.len(),
            Mode::PickWorkspace { .. } | Mode::PickTab { .. } => self.picker_indexes.len(),
        };
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
    }
}

pub fn workspace_label(workspace: &Workspace) -> String {
    if workspace.label.is_empty() { format!("Workspace {}", workspace.number) } else { workspace.label.clone() }
}

pub fn tab_label(tab: &Tab) -> String {
    if tab.label.is_empty() { format!("Tab {}", tab.number) } else { tab.label.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WorkspaceSlot;
    use herdr_client::models::AgentStatus;

    fn workspace(id: &str, number: u32, label: &str) -> Workspace {
        Workspace {
            workspace_id: id.into(),
            number,
            label: label.into(),
            focused: false,
            agent_status: AgentStatus::Unknown,
        }
    }

    fn tab(id: &str, workspace_id: &str, number: u32, label: &str) -> Tab {
        Tab {
            tab_id: id.into(),
            workspace_id: workspace_id.into(),
            number,
            label: label.into(),
            focused: false,
            agent_status: AgentStatus::Unknown,
        }
    }

    fn sample() -> App {
        App::new(
            ZenState::default(),
            vec![workspace("w1", 1, "Core"), workspace("w2", 2, "Docs")],
            vec![tab("w1:t1", "w1", 1, "Claude"), tab("w1:t2", "w1", 2, "Logs"), tab("w2:t1", "w2", 1, "Write")],
        )
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn adds_workspace_and_tabs_through_the_picker() {
        let mut app = sample();
        assert_eq!(app.rows, [Row::AddWorkspace]);
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.mode, Mode::PickWorkspace { target: WorkspaceTarget::Add }));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.state.workspaces[0].workspace_id, "w1");
        assert!(app.take_dirty());

        // Move to Add tab row and pick the first tab.
        app.selected = app.rows.iter().position(|row| matches!(row, Row::AddTab { .. })).unwrap();
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.state.workspaces[0].tab_ids, ["w1:t1"]);
    }

    #[test]
    fn replace_one_workspace_keeps_the_other() {
        let mut app = App::new(
            ZenState {
                active: false,
                workspaces: vec![
                    WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t1".into()] },
                    WorkspaceSlot { workspace_id: "w2".into(), tab_ids: vec!["w2:t1".into()] },
                ],
            },
            vec![workspace("w1", 1, "Core"), workspace("w2", 2, "Docs"), workspace("w3", 3, "Ops")],
            vec![tab("w1:t1", "w1", 1, "Claude"), tab("w2:t1", "w2", 1, "Write"), tab("w3:t1", "w3", 1, "Ship")],
        );
        app.selected = 0;
        app.handle_key(key(KeyCode::Char('r')));
        // Filtered: w1 (current replace slot), w3. Move to w3.
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.state.workspaces[0].workspace_id, "w3");
        assert!(app.state.workspaces[0].tab_ids.is_empty());
        assert_eq!(app.state.workspaces[1].workspace_id, "w2");
        assert_eq!(app.state.workspaces[1].tab_ids, ["w2:t1"]);
    }

    #[test]
    fn enter_on_a_tab_focuses_it() {
        let mut app = App::new(
            ZenState {
                active: false,
                workspaces: vec![WorkspaceSlot {
                    workspace_id: "w1".into(),
                    tab_ids: vec!["w1:t1".into(), "w1:t2".into()],
                }],
            },
            vec![workspace("w1", 1, "Core")],
            vec![tab("w1:t1", "w1", 1, "Claude"), tab("w1:t2", "w1", 2, "Logs")],
        );
        app.selected = 2; // second tab
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::FocusTab { tab_id: "w1:t2".into() });
    }

    #[test]
    fn enter_on_a_workspace_focuses_its_first_available_tab() {
        let mut app = App::new(
            ZenState {
                active: false,
                workspaces: vec![WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t1".into()] }],
            },
            vec![workspace("w1", 1, "Core")],
            vec![tab("w1:t1", "w1", 1, "Claude")],
        );
        app.selected = 0;
        assert_eq!(app.handle_key(key(KeyCode::Enter)), Command::FocusTab { tab_id: "w1:t1".into() });
    }

    #[test]
    fn delete_removes_selected_tab() {
        let mut app = App::new(
            ZenState {
                active: false,
                workspaces: vec![WorkspaceSlot {
                    workspace_id: "w1".into(),
                    tab_ids: vec!["w1:t1".into(), "w1:t2".into()],
                }],
            },
            vec![workspace("w1", 1, "Core")],
            vec![tab("w1:t1", "w1", 1, "Claude"), tab("w1:t2", "w1", 2, "Logs")],
        );
        app.selected = 1; // first tab
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.state.workspaces[0].tab_ids, ["w1:t2"]);
    }

    #[test]
    fn z_toggles_zen_from_overview() {
        let mut app = sample();
        assert_eq!(app.handle_key(key(KeyCode::Char('z'))), Command::ToggleZen);
    }
}
