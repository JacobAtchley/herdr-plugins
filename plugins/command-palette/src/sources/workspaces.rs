use herdr_client::PluginContext;
use herdr_client::models::Workspace;

use crate::item::{Action, Item, Kind};

pub fn items(workspaces: &[Workspace], ctx: &PluginContext) -> Vec<Item> {
    workspaces
        .iter()
        .map(|ws| {
            Item::new(
                Kind::Workspace,
                format!("ws:{}", ws.workspace_id),
                super::workspace_label(workspaces, &ws.workspace_id),
                Action::FocusWorkspace(ws.workspace_id.clone()),
            )
            .subtitle(format!("#{}", ws.number))
            .status(ws.agent_status)
            .current(ctx.workspace_id.as_deref() == Some(ws.workspace_id.as_str()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Kind};
    use herdr_client::models::AgentStatus;
    use serde_json::json;

    #[test]
    fn builds_workspace_items() {
        let workspaces: Vec<Workspace> = serde_json::from_value(json!([
            {"workspace_id": "w1", "number": 1, "label": "CT", "agent_status": "unknown"},
            {"workspace_id": "w5", "number": 4, "label": "V9", "agent_status": "idle"},
            {"workspace_id": "w7", "number": 6, "label": ""}
        ]))
        .unwrap();
        let ctx = PluginContext { workspace_id: Some("w5".into()), ..Default::default() };
        let items = items(&workspaces, &ctx);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, Kind::Workspace);
        assert_eq!(items[0].id, "ws:w1");
        assert_eq!(items[0].title, "CT");
        assert_eq!(items[0].subtitle.as_deref(), Some("#1"));
        assert_eq!(items[0].action, Action::FocusWorkspace("w1".into()));
        assert!(!items[0].current);
        assert!(items[1].current);
        assert_eq!(items[1].status, Some(AgentStatus::Idle));
        assert_eq!(items[2].title, "Workspace 6");
    }
}
