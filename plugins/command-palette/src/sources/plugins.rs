use herdr_client::models::PluginAction;

use crate::item::{Action, Item, Kind};

/// The palette's own `open` action; listing it would just reopen the palette.
pub const SELF_ACTION: &str = "jacob.command-palette.open";

pub fn items(actions: &[PluginAction]) -> Vec<Item> {
    actions
        .iter()
        .map(|action| (action, action.qualified_id()))
        .filter(|(_, qualified)| qualified != SELF_ACTION)
        .map(|(action, qualified)| {
            Item::new(Kind::Plugin, format!("plugin:{qualified}"), &action.title, Action::InvokePluginAction(qualified))
                .subtitle(&action.plugin_id)
                .keywords(action.description.iter().cloned().collect())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Kind};
    use serde_json::json;

    #[test]
    fn builds_plugin_items_excluding_the_palette_itself() {
        let actions: Vec<PluginAction> = serde_json::from_value(json!([
            {"plugin_id": "jacob.command-palette", "action_id": "open", "title": "Open command palette"},
            {"plugin_id": "hhdebb.herdr-radar", "action_id": "refresh", "title": "Refresh radar",
             "description": "Rescan agents"},
            {"plugin_id": "x.y", "action_id": "go", "title": "Go"}
        ]))
        .unwrap();
        let items = items(&actions);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, Kind::Plugin);
        assert_eq!(items[0].id, "plugin:hhdebb.herdr-radar.refresh");
        assert_eq!(items[0].title, "Refresh radar");
        assert_eq!(items[0].subtitle.as_deref(), Some("hhdebb.herdr-radar"));
        assert_eq!(items[0].keywords, ["Rescan agents"]);
        assert_eq!(items[0].action, Action::InvokePluginAction("hhdebb.herdr-radar.refresh".into()));
        assert!(items[1].keywords.is_empty());
    }
}
