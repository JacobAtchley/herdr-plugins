use herdr_client::PluginContext;
use herdr_client::models::{Tab, Workspace};

use crate::item::{Action, Item, Kind};

pub fn items(tabs: &[Tab], workspaces: &[Workspace], ctx: &PluginContext) -> Vec<Item> {
    tabs.iter()
        .map(|tab| {
            let title = format!(
                "{} › {}",
                super::workspace_label(workspaces, &tab.workspace_id),
                super::tab_label(tabs, &tab.tab_id)
            );
            Item::new(Kind::Tab, format!("tab:{}", tab.tab_id), title, Action::FocusTab(tab.tab_id.clone()))
                .status(tab.agent_status)
                .current(ctx.tab_id.as_deref() == Some(tab.tab_id.as_str()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Kind};
    use serde_json::json;

    #[test]
    fn builds_tab_items_titled_with_workspace() {
        let workspaces: Vec<Workspace> =
            serde_json::from_value(json!([{"workspace_id": "w1", "number": 1, "label": "CT"}])).unwrap();
        let tabs: Vec<Tab> = serde_json::from_value(json!([
            {"tab_id": "w1:t3", "workspace_id": "w1", "number": 3, "label": "Claude", "agent_status": "working"},
            {"tab_id": "w9:t1", "workspace_id": "w9", "number": 1, "label": ""}
        ]))
        .unwrap();
        let ctx = PluginContext { tab_id: Some("w1:t3".into()), ..Default::default() };
        let items = items(&tabs, &workspaces, &ctx);
        assert_eq!(items[0].kind, Kind::Tab);
        assert_eq!(items[0].id, "tab:w1:t3");
        assert_eq!(items[0].title, "CT › Claude");
        assert_eq!(items[0].action, Action::FocusTab("w1:t3".into()));
        assert!(items[0].current);
        assert_eq!(items[1].title, "w9 › Tab 1");
        assert!(!items[1].current);
    }
}
