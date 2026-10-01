//! Resolve a saved shortlist against the live workspace/tab lists.

use herdr_client::models::{Tab, Workspace};

use crate::state::ZenState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// Tab ids that still exist, in shortlist order.
    pub available_tab_ids: Vec<String>,
    /// Human-readable notices for missing workspaces/tabs.
    pub missing: Vec<String>,
    /// Tab to focus on activate, if any.
    pub focus_tab_id: Option<String>,
}

/// Keeps saved ids intact in `ZenState`; only reports what is currently available.
pub fn resolve(state: &ZenState, workspaces: &[Workspace], tabs: &[Tab], current_tab_id: Option<&str>) -> Resolution {
    let mut available_tab_ids = Vec::new();
    let mut missing = Vec::new();

    for slot in &state.workspaces {
        let workspace = workspaces.iter().find(|workspace| workspace.workspace_id == slot.workspace_id);
        if workspace.is_none() {
            missing.push(format!("workspace {} unavailable", label_workspace(slot.workspace_id.as_str(), None)));
            for tab_id in &slot.tab_ids {
                missing.push(format!("tab {tab_id} unavailable"));
            }
            continue;
        }
        let workspace_label = label_workspace(slot.workspace_id.as_str(), workspace);
        for tab_id in &slot.tab_ids {
            match tabs.iter().find(|tab| tab.tab_id == *tab_id) {
                Some(tab) if tab.workspace_id == slot.workspace_id => {
                    available_tab_ids.push(tab_id.clone());
                }
                Some(_) => {
                    missing.push(format!("tab {} moved out of workspace {workspace_label}", label_tab(tab_id, tabs)));
                }
                None => {
                    missing.push(format!("tab {} unavailable", label_tab(tab_id, tabs)));
                }
            }
        }
    }

    let focus_tab_id = current_tab_id
        .filter(|tab_id| available_tab_ids.iter().any(|id| id == tab_id))
        .map(str::to_string)
        .or_else(|| available_tab_ids.first().cloned());

    Resolution { available_tab_ids, missing, focus_tab_id }
}

fn label_workspace(id: &str, workspace: Option<&Workspace>) -> String {
    match workspace {
        Some(workspace) if !workspace.label.is_empty() => workspace.label.clone(),
        Some(workspace) => format!("Workspace {}", workspace.number),
        None => id.to_string(),
    }
}

fn label_tab(tab_id: &str, tabs: &[Tab]) -> String {
    match tabs.iter().find(|tab| tab.tab_id == tab_id) {
        Some(tab) if !tab.label.is_empty() => tab.label.clone(),
        Some(tab) => format!("Tab {}", tab.number),
        None => tab_id.to_string(),
    }
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

    #[test]
    fn keeps_available_tabs_and_reports_missing() {
        let state = ZenState {
            active: true,
            workspaces: vec![
                WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t1".into(), "w1:t9".into()] },
                WorkspaceSlot { workspace_id: "w9".into(), tab_ids: vec!["w9:t1".into()] },
            ],
        };
        let workspaces = vec![workspace("w1", 1, "Core")];
        let tabs = vec![tab("w1:t1", "w1", 1, "Claude")];
        let resolution = resolve(&state, &workspaces, &tabs, Some("w2:t1"));
        assert_eq!(resolution.available_tab_ids, ["w1:t1"]);
        assert_eq!(resolution.focus_tab_id.as_deref(), Some("w1:t1"));
        assert!(resolution.missing.iter().any(|message| message.contains("w1:t9")));
        assert!(resolution.missing.iter().any(|message| message.contains("w9")));
    }

    #[test]
    fn prefers_current_tab_when_it_is_in_the_shortlist() {
        let state = ZenState {
            active: true,
            workspaces: vec![WorkspaceSlot {
                workspace_id: "w1".into(),
                tab_ids: vec!["w1:t1".into(), "w1:t2".into()],
            }],
        };
        let workspaces = vec![workspace("w1", 1, "Core")];
        let tabs = vec![tab("w1:t1", "w1", 1, "A"), tab("w1:t2", "w1", 2, "B")];
        let resolution = resolve(&state, &workspaces, &tabs, Some("w1:t2"));
        assert_eq!(resolution.focus_tab_id.as_deref(), Some("w1:t2"));
    }
}
