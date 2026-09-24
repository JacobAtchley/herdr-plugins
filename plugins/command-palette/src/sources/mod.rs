pub mod agents;
pub mod builtins;
pub mod plugins;
pub mod tabs;
pub mod user;
pub mod workspaces;

use std::path::Path;

use herdr_client::models::{Tab, Workspace};
use herdr_client::{Api, Error, PluginContext};

use crate::app::Status;
use crate::item::Item;

pub struct Loaded {
    pub items: Vec<Item>,
    /// One line per source that failed to load, for the footer and the log.
    pub notices: Vec<String>,
    /// How many of the four remote sources (workspaces, tabs, agents, plugin
    /// actions) failed to load.
    pub remote_failures: usize,
}

/// Fetches every remote source in parallel, then builds items. A failing
/// source adds a notice instead of failing the palette.
pub fn load_all(api: &dyn Api, ctx: &PluginContext, config_dir: &Path) -> Loaded {
    let (workspace_list, tab_list, agent_list, action_list) = std::thread::scope(|scope| {
        let workspaces = scope.spawn(|| api.workspace_list());
        let tabs = scope.spawn(|| api.tab_list());
        let agents = scope.spawn(|| api.agent_list());
        let actions = scope.spawn(|| api.plugin_action_list());
        (
            workspaces.join().expect("workspace source panicked"),
            tabs.join().expect("tab source panicked"),
            agents.join().expect("agent source panicked"),
            actions.join().expect("plugin source panicked"),
        )
    });

    let mut notices = Vec::new();
    let mut remote_failures = 0;
    let ws = or_notice(workspace_list, "workspaces", &mut notices, &mut remote_failures);
    let tabs = or_notice(tab_list, "tabs", &mut notices, &mut remote_failures);
    let agents = or_notice(agent_list, "agents", &mut notices, &mut remote_failures);
    let actions = or_notice(action_list, "plugin actions", &mut notices, &mut remote_failures);

    let mut items = Vec::new();
    items.extend(workspaces::items(&ws, ctx));
    items.extend(tabs::items(&tabs, &ws, ctx));
    items.extend(agents::items(&agents, &ws, &tabs, ctx));
    items.extend(builtins::items());
    items.extend(plugins::items(&actions));
    match user::load(&config_dir.join("commands.toml")) {
        Ok(commands) => items.extend(user::items(&commands)),
        Err(err) => notices.push(err),
    }
    Loaded { items, notices, remote_failures }
}

fn or_notice<T>(
    result: Result<Vec<T>, Error>,
    source: &str,
    notices: &mut Vec<String>,
    failures: &mut usize,
) -> Vec<T> {
    result.unwrap_or_else(|err| {
        notices.push(format!("{source} unavailable: {err}"));
        *failures += 1;
        Vec::new()
    })
}

/// Summarizes source-load notices for the footer. A single notice shows as
/// Info, unchanged; several collapse into a count so the footer stays one
/// line. If every remote source failed (herdr itself is unreachable), that
/// takes priority and shows as an Error.
pub fn notice_status(notices: &[String], remote_failures: usize) -> Option<Status> {
    const REMOTE_SOURCES: usize = 4;
    if remote_failures >= REMOTE_SOURCES {
        return Some(Status::Error("herdr unreachable — see palette.log".to_string()));
    }
    match notices.len() {
        0 => None,
        1 => Some(Status::Info(notices[0].clone())),
        n => Some(Status::Info(format!("{n} sources unavailable — see palette.log"))),
    }
}

/// A workspace's label, `Workspace <n>` when blank, or its id when unknown.
pub fn workspace_label(workspaces: &[Workspace], id: &str) -> String {
    match workspaces.iter().find(|ws| ws.workspace_id == id) {
        Some(ws) if !ws.label.is_empty() => ws.label.clone(),
        Some(ws) => format!("Workspace {}", ws.number),
        None => id.to_string(),
    }
}

/// A tab's label, `Tab <n>` when blank, or its id when unknown.
pub fn tab_label(tabs: &[Tab], id: &str) -> String {
    match tabs.iter().find(|tab| tab.tab_id == id) {
        Some(tab) if !tab.label.is_empty() => tab.label.clone(),
        Some(tab) => format!("Tab {}", tab.number),
        None => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Kind;
    use crate::testing::FakeApi;
    use serde_json::json;

    fn full_api() -> FakeApi {
        FakeApi::new()
            .ok("workspace.list", json!({"workspaces": [{"workspace_id": "w1", "number": 1, "label": "CT"}]}))
            .ok(
                "tab.list",
                json!({"tabs": [{"tab_id": "w1:t1", "workspace_id": "w1", "number": 1, "label": "Claude"}]}),
            )
            .ok(
                "agent.list",
                json!({"agents": [{"pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1", "agent": "claude"}]}),
            )
            .ok("plugin.action.list", json!({"actions": [{"plugin_id": "a.b", "action_id": "c", "title": "Do C"}]}))
    }

    fn kinds(loaded: &Loaded) -> Vec<Kind> {
        let mut kinds: Vec<Kind> = loaded.items.iter().map(|i| i.kind).collect();
        kinds.dedup();
        kinds
    }

    #[test]
    fn loads_every_source() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("commands.toml"), "[[commands]]\ntitle = \"U\"\nrun = \"true\"\n").unwrap();
        let loaded = load_all(&full_api(), &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices, Vec::<String>::new());
        assert_eq!(kinds(&loaded), [Kind::Workspace, Kind::Tab, Kind::Agent, Kind::Command, Kind::Plugin, Kind::User]);
        assert_eq!(loaded.items.iter().find(|i| i.kind == Kind::Tab).unwrap().title, "CT › Claude");
    }

    #[test]
    fn a_failing_source_becomes_a_notice_and_others_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let api = full_api().err("agent.list", "internal", "boom");
        let loaded = load_all(&api, &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices, ["agents unavailable: agent.list: boom (internal)"]);
        assert!(loaded.items.iter().any(|i| i.kind == Kind::Workspace));
        assert!(!loaded.items.iter().any(|i| i.kind == Kind::Agent));
        assert_eq!(loaded.remote_failures, 1);
    }

    #[test]
    fn missing_workspaces_fall_back_to_ids_in_titles() {
        let dir = tempfile::tempdir().unwrap();
        let api = full_api().err("workspace.list", "internal", "boom");
        let loaded = load_all(&api, &PluginContext::default(), dir.path());
        assert_eq!(loaded.items.iter().find(|i| i.kind == Kind::Tab).unwrap().title, "w1 › Claude");
        assert_eq!(loaded.notices.len(), 1);
        assert_eq!(loaded.remote_failures, 1);
    }

    #[test]
    fn invalid_commands_file_is_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("commands.toml"), "[[commands]]\ntitle =\n").unwrap();
        let loaded = load_all(&full_api(), &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices.len(), 1);
        assert!(loaded.notices[0].starts_with("commands.toml line 2"), "{}", loaded.notices[0]);
        assert!(loaded.items.iter().any(|i| i.kind == Kind::Command));
    }

    #[test]
    fn socket_down_leaves_only_local_sources() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_all(&FakeApi::new(), &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices.len(), 4);
        assert_eq!(loaded.remote_failures, 4);
        assert_eq!(kinds(&loaded), [Kind::Command]);
    }

    #[test]
    fn notice_status_is_none_for_no_notices() {
        assert_eq!(notice_status(&[], 0), None);
    }

    #[test]
    fn notice_status_shows_a_single_notice_as_info() {
        assert_eq!(
            notice_status(&["agents unavailable: boom".to_string()], 1),
            Some(Status::Info("agents unavailable: boom".to_string()))
        );
    }

    #[test]
    fn notice_status_collapses_several_notices_into_a_count() {
        let notices = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            notice_status(&notices, 2),
            Some(Status::Info("3 sources unavailable — see palette.log".to_string()))
        );
    }

    #[test]
    fn notice_status_is_an_error_when_every_remote_source_fails() {
        let notices = vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()];
        assert_eq!(notice_status(&notices, 4), Some(Status::Error("herdr unreachable — see palette.log".to_string())));
    }
}
