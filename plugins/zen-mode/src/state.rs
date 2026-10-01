//! Persisted zen shortlist under `HERDR_PLUGIN_STATE_DIR/zen.toml`.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MAX_WORKSPACES: usize = 2;
pub const MAX_TABS_PER_WORKSPACE: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ZenState {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSlot {
    pub workspace_id: String,
    #[serde(default)]
    pub tab_ids: Vec<String>,
}

impl ZenState {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("zen.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
        let mut state: Self = toml::from_str(&text).map_err(|err| format!("parse {}: {err}", path.display()))?;
        state.clamp();
        Ok(state)
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let mut state = self.clone();
        state.clamp();
        std::fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
        let path = dir.join("zen.toml");
        let text = toml::to_string_pretty(&state).map_err(|err| format!("serialize zen.toml: {err}"))?;
        std::fs::write(&path, text).map_err(|err| format!("write {}: {err}", path.display()))
    }

    /// Enforces the shortlist limits and drops empty/duplicate ids.
    pub fn clamp(&mut self) {
        let mut seen_workspaces = Vec::new();
        self.workspaces.retain(|slot| {
            if slot.workspace_id.is_empty() || seen_workspaces.contains(&slot.workspace_id) {
                return false;
            }
            seen_workspaces.push(slot.workspace_id.clone());
            true
        });
        self.workspaces.truncate(MAX_WORKSPACES);
        for slot in &mut self.workspaces {
            let mut seen_tabs = Vec::new();
            slot.tab_ids.retain(|tab_id| {
                if tab_id.is_empty() || seen_tabs.contains(tab_id) {
                    return false;
                }
                seen_tabs.push(tab_id.clone());
                true
            });
            slot.tab_ids.truncate(MAX_TABS_PER_WORKSPACE);
        }
    }

    pub fn set_workspace(&mut self, index: usize, workspace_id: String) {
        if index >= MAX_WORKSPACES {
            return;
        }
        while self.workspaces.len() <= index {
            self.workspaces.push(WorkspaceSlot { workspace_id: String::new(), tab_ids: Vec::new() });
        }
        self.workspaces[index] = WorkspaceSlot { workspace_id: workspace_id.clone(), tab_ids: Vec::new() };
        // Keep the slot we just wrote; drop any other copy of the same id or empties.
        let mut kept_target = false;
        self.workspaces.retain(|slot| {
            if slot.workspace_id.is_empty() {
                return false;
            }
            if slot.workspace_id != workspace_id {
                return true;
            }
            if kept_target {
                return false;
            }
            kept_target = true;
            true
        });
        self.clamp();
    }

    pub fn add_workspace(&mut self, workspace_id: String) -> Result<(), String> {
        if self.workspaces.iter().any(|slot| slot.workspace_id == workspace_id) {
            return Err("workspace already selected".into());
        }
        if self.workspaces.len() >= MAX_WORKSPACES {
            return Err(format!("at most {MAX_WORKSPACES} workspaces"));
        }
        self.workspaces.push(WorkspaceSlot { workspace_id, tab_ids: Vec::new() });
        Ok(())
    }

    pub fn remove_workspace(&mut self, index: usize) {
        if index < self.workspaces.len() {
            self.workspaces.remove(index);
        }
    }

    pub fn set_tab(&mut self, workspace_index: usize, tab_index: usize, tab_id: String) -> Result<(), String> {
        let slot = self.workspaces.get_mut(workspace_index).ok_or_else(|| "workspace slot missing".to_string())?;
        if tab_index > slot.tab_ids.len() || tab_index >= MAX_TABS_PER_WORKSPACE {
            return Err(format!("at most {MAX_TABS_PER_WORKSPACE} tabs per workspace"));
        }
        if tab_index < slot.tab_ids.len() {
            slot.tab_ids.remove(tab_index);
        }
        slot.tab_ids.retain(|id| id != &tab_id);
        if slot.tab_ids.len() >= MAX_TABS_PER_WORKSPACE {
            return Err(format!("at most {MAX_TABS_PER_WORKSPACE} tabs per workspace"));
        }
        let insert_at = tab_index.min(slot.tab_ids.len());
        slot.tab_ids.insert(insert_at, tab_id);
        Ok(())
    }

    pub fn add_tab(&mut self, workspace_index: usize, tab_id: String) -> Result<(), String> {
        let slot = self.workspaces.get_mut(workspace_index).ok_or_else(|| "workspace slot missing".to_string())?;
        if slot.tab_ids.iter().any(|id| id == &tab_id) {
            return Err("tab already selected".into());
        }
        if slot.tab_ids.len() >= MAX_TABS_PER_WORKSPACE {
            return Err(format!("at most {MAX_TABS_PER_WORKSPACE} tabs per workspace"));
        }
        slot.tab_ids.push(tab_id);
        Ok(())
    }

    pub fn remove_tab(&mut self, workspace_index: usize, tab_index: usize) {
        if let Some(slot) = self.workspaces.get_mut(workspace_index)
            && tab_index < slot.tab_ids.len()
        {
            slot.tab_ids.remove(tab_index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_as_empty_inactive() {
        let dir = tempfile::tempdir().unwrap();
        let state = ZenState::load(dir.path()).unwrap();
        assert_eq!(state, ZenState::default());
        assert!(!state.active);
    }

    #[test]
    fn round_trips_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let state = ZenState {
            active: true,
            workspaces: vec![
                WorkspaceSlot {
                    workspace_id: "w1".into(),
                    tab_ids: vec!["w1:t1".into(), "w1:t2".into(), "w1:t3".into()],
                },
                WorkspaceSlot { workspace_id: "w2".into(), tab_ids: vec!["w2:t1".into()] },
            ],
        };
        state.save(dir.path()).unwrap();
        assert_eq!(ZenState::load(dir.path()).unwrap(), state);
    }

    #[test]
    fn clamp_enforces_limits_and_dedupes() {
        let mut state = ZenState {
            active: false,
            workspaces: vec![
                WorkspaceSlot {
                    workspace_id: "w1".into(),
                    tab_ids: vec!["w1:t1".into(), "w1:t1".into(), "w1:t2".into(), "w1:t3".into(), "w1:t4".into()],
                },
                WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t9".into()] },
                WorkspaceSlot { workspace_id: "w3".into(), tab_ids: vec!["w3:t1".into()] },
            ],
        };
        state.clamp();
        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.workspaces[0].workspace_id, "w1");
        assert_eq!(state.workspaces[0].tab_ids, ["w1:t1", "w1:t2", "w1:t3"]);
        assert_eq!(state.workspaces[1].workspace_id, "w3");
    }

    #[test]
    fn replace_workspace_clears_its_tabs_and_keeps_the_other_slot() {
        let mut state = ZenState {
            active: false,
            workspaces: vec![
                WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t1".into()] },
                WorkspaceSlot { workspace_id: "w2".into(), tab_ids: vec!["w2:t1".into()] },
            ],
        };
        state.set_workspace(0, "w9".into());
        assert_eq!(state.workspaces[0].workspace_id, "w9");
        assert!(state.workspaces[0].tab_ids.is_empty());
        assert_eq!(state.workspaces[1].workspace_id, "w2");
        assert_eq!(state.workspaces[1].tab_ids, ["w2:t1"]);
    }

    #[test]
    fn add_workspace_and_tab_respect_caps() {
        let mut state = ZenState::default();
        state.add_workspace("w1".into()).unwrap();
        state.add_workspace("w2".into()).unwrap();
        assert_eq!(state.add_workspace("w3".into()).unwrap_err(), "at most 2 workspaces");
        state.add_tab(0, "w1:t1".into()).unwrap();
        state.add_tab(0, "w1:t2".into()).unwrap();
        state.add_tab(0, "w1:t3".into()).unwrap();
        assert_eq!(state.add_tab(0, "w1:t4".into()).unwrap_err(), "at most 3 tabs per workspace");
    }
}
