//! Toggle zen on/off: resolve the shortlist, toast gaps, focus a tab.

use herdr_client::Api;
use serde_json::json;

use crate::resolve::{self, Resolution};
use crate::state::ZenState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleOutcome {
    pub active: bool,
    pub resolution: Resolution,
}

/// Flip `active`, persist, and when turning on focus the best available tab.
pub fn toggle(api: &impl Api, state: &mut ZenState, current_tab_id: Option<&str>) -> Result<ToggleOutcome, String> {
    if state.active {
        state.active = false;
        return Ok(ToggleOutcome {
            active: false,
            resolution: Resolution { available_tab_ids: Vec::new(), missing: Vec::new(), focus_tab_id: None },
        });
    }

    state.active = true;
    let workspaces = api.workspace_list().map_err(|err| err.to_string())?;
    let tabs = api.tab_list().map_err(|err| err.to_string())?;
    let resolution = resolve::resolve(state, &workspaces, &tabs, current_tab_id);

    for message in &resolution.missing {
        notify(api, "Zen mode", message)?;
    }
    if resolution.available_tab_ids.is_empty() {
        notify(api, "Zen mode", "no selected tabs available")?;
    } else if let Some(tab_id) = &resolution.focus_tab_id {
        api.request("tab.focus", json!({ "tab_id": tab_id })).map_err(|err| err.to_string())?;
    }

    Ok(ToggleOutcome { active: true, resolution })
}

pub fn notify(api: &impl Api, title: &str, body: &str) -> Result<(), String> {
    api.request(
        "notification.show",
        json!({
            "title": title,
            "body": body,
            "sound": "none",
        }),
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WorkspaceSlot;
    use crate::testing::FakeApi;
    use serde_json::json;

    fn live_lists() -> FakeApi {
        FakeApi::new()
            .ok(
                "workspace.list",
                json!({
                    "workspaces": [
                        {"workspace_id": "w1", "number": 1, "label": "Core", "focused": true, "agent_status": "idle"},
                        {"workspace_id": "w2", "number": 2, "label": "Docs", "focused": false, "agent_status": "idle"}
                    ]
                }),
            )
            .ok(
                "tab.list",
                json!({
                    "tabs": [
                        {"tab_id": "w1:t1", "workspace_id": "w1", "number": 1, "label": "Claude", "focused": true, "agent_status": "idle"},
                        {"tab_id": "w1:t2", "workspace_id": "w1", "number": 2, "label": "Logs", "focused": false, "agent_status": "idle"},
                        {"tab_id": "w2:t1", "workspace_id": "w2", "number": 1, "label": "Write", "focused": false, "agent_status": "idle"}
                    ]
                }),
            )
            .ok("tab.focus", json!({}))
            .ok("notification.show", json!({}))
    }

    #[test]
    fn activate_focuses_and_toasts_missing_without_closing() {
        let api = live_lists();
        let mut state = ZenState {
            active: false,
            workspaces: vec![
                WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t1".into(), "w1:t9".into()] },
                WorkspaceSlot { workspace_id: "w9".into(), tab_ids: vec!["w9:t1".into()] },
            ],
        };
        let outcome = toggle(&api, &mut state, Some("w1:t1")).unwrap();
        assert!(outcome.active);
        assert!(state.active);
        assert_eq!(outcome.resolution.focus_tab_id.as_deref(), Some("w1:t1"));
        assert_eq!(state.workspaces[0].tab_ids, ["w1:t1", "w1:t9"]);

        let methods: Vec<_> = api.calls().into_iter().map(|(method, _)| method).collect();
        assert!(methods.contains(&"tab.focus".to_string()));
        assert!(methods.contains(&"notification.show".to_string()));
        assert!(!methods.iter().any(|method| method.contains("close") || method.contains("move")));
    }

    #[test]
    fn deactivate_clears_active_and_keeps_selection() {
        let api = FakeApi::new();
        let mut state = ZenState {
            active: true,
            workspaces: vec![WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t1".into()] }],
        };
        let outcome = toggle(&api, &mut state, None).unwrap();
        assert!(!outcome.active);
        assert!(!state.active);
        assert_eq!(state.workspaces[0].tab_ids, ["w1:t1"]);
        assert!(api.calls().is_empty());
    }

    #[test]
    fn activate_with_empty_shortlist_notifies_and_skips_focus() {
        let api = live_lists();
        let mut state = ZenState::default();
        let outcome = toggle(&api, &mut state, None).unwrap();
        assert!(outcome.active);
        assert!(outcome.resolution.focus_tab_id.is_none());
        let methods: Vec<_> = api.calls().into_iter().map(|(method, _)| method).collect();
        assert!(methods.contains(&"notification.show".to_string()));
        assert!(!methods.contains(&"tab.focus".to_string()));
    }
}
