use herdr_client::PluginContext;
use herdr_client::models::{Agent, Tab, Workspace};

use crate::item::{Action, Item, Kind};

pub fn items(agents: &[Agent], workspaces: &[Workspace], tabs: &[Tab], ctx: &PluginContext) -> Vec<Item> {
    agents
        .iter()
        .map(|agent| {
            let name =
                agent.name.as_deref().or(agent.display_agent.as_deref()).or(agent.agent.as_deref()).unwrap_or("agent");
            let title = format!("{name} ({})", super::workspace_label(workspaces, &agent.workspace_id));
            let mut keywords = vec![agent.agent_status.as_str().to_string()];
            keywords.extend(agent.agent.clone());
            Item::new(Kind::Agent, format!("agent:{}", agent.pane_id), title, Action::FocusPane(agent.pane_id.clone()))
                .subtitle(super::tab_label(tabs, &agent.tab_id))
                .keywords(keywords)
                .status(agent.agent_status)
                .current(ctx.focused_pane_id.as_deref() == Some(agent.pane_id.as_str()))
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
    fn builds_agent_items_preferring_name_then_display_then_kind() {
        let workspaces: Vec<Workspace> =
            serde_json::from_value(json!([{"workspace_id": "w5", "number": 4, "label": "V9"}])).unwrap();
        let tabs: Vec<Tab> = serde_json::from_value(json!([
            {"tab_id": "w5:t2", "workspace_id": "w5", "number": 2, "label": "Claude"}
        ]))
        .unwrap();
        let agents: Vec<Agent> = serde_json::from_value(json!([
            {"pane_id": "w5:p2", "workspace_id": "w5", "tab_id": "w5:t2", "agent": "claude",
             "display_agent": "Claude Code", "name": "reviewer", "agent_status": "working"},
            {"pane_id": "w5:p3", "workspace_id": "w5", "tab_id": "w5:t2", "agent": "codex",
             "display_agent": "Codex", "agent_status": "blocked"},
            {"pane_id": "w5:p4", "workspace_id": "w5", "tab_id": "w5:t2", "agent": "pi"},
            {"pane_id": "w5:p5", "workspace_id": "w5", "tab_id": "w5:t2"}
        ]))
        .unwrap();
        let ctx = PluginContext { focused_pane_id: Some("w5:p3".into()), ..Default::default() };
        let items = items(&agents, &workspaces, &tabs, &ctx);
        let titles: Vec<_> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["reviewer (V9)", "Codex (V9)", "pi (V9)", "agent (V9)"]);
        assert_eq!(items[0].kind, Kind::Agent);
        assert_eq!(items[0].id, "agent:w5:p2");
        assert_eq!(items[0].subtitle.as_deref(), Some("Claude"));
        assert_eq!(items[0].status, Some(AgentStatus::Working));
        assert_eq!(items[0].keywords, ["working", "claude"]);
        assert_eq!(items[0].action, Action::FocusPane("w5:p2".into()));
        assert!(items[1].current);
    }
}
