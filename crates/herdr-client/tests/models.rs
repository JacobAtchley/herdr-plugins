use herdr_client::PluginContext;
use herdr_client::models::{Agent, AgentStatus, PluginAction, Tab, Workspace};
use serde_json::Value;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn result_field(name: &str, field: &str) -> Value {
    let value: Value = serde_json::from_str(&fixture(name)).unwrap();
    value["result"][field].clone()
}

#[test]
fn parses_workspaces() {
    let workspaces: Vec<Workspace> = serde_json::from_value(result_field("workspace_list.json", "workspaces")).unwrap();
    assert_eq!(workspaces.len(), 3);
    assert_eq!(workspaces[2].workspace_id, "w5");
    assert_eq!(workspaces[2].label, "V9 Orchestrator");
    assert_eq!(workspaces[2].number, 4);
    assert!(workspaces[2].focused);
    assert_eq!(workspaces[2].agent_status, AgentStatus::Idle);
}

#[test]
fn parses_tabs() {
    let tabs: Vec<Tab> = serde_json::from_value(result_field("tab_list.json", "tabs")).unwrap();
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs[1].tab_id, "w4:t1");
    assert_eq!(tabs[1].workspace_id, "w4");
    assert_eq!(tabs[1].label, "Hermes");
}

#[test]
fn parses_agents_with_optional_fields() {
    let agents: Vec<Agent> = serde_json::from_value(result_field("agent_list.json", "agents")).unwrap();
    assert_eq!(agents[0].agent.as_deref(), Some("hermes"));
    assert_eq!(agents[0].name, None);
    assert_eq!(agents[0].display_agent, None);
    assert_eq!(agents[1].name.as_deref(), Some("reviewer"));
    assert_eq!(agents[1].display_agent.as_deref(), Some("Claude Code"));
    assert_eq!(agents[1].agent_status, AgentStatus::Working);
    assert_eq!(agents[1].pane_id, "w5:p2");
}

#[test]
fn unknown_agent_status_maps_to_unknown() {
    let tab: Tab = serde_json::from_str(
        r#"{"tab_id":"w1:t1","workspace_id":"w1","number":1,"label":"x","agent_status":"sleeping"}"#,
    )
    .unwrap();
    assert_eq!(tab.agent_status, AgentStatus::Unknown);
    assert!(!tab.focused);
}

#[test]
fn missing_label_defaults_to_empty() {
    let ws: Workspace = serde_json::from_str(r#"{"workspace_id":"w9","number":9}"#).unwrap();
    assert_eq!(ws.label, "");
    assert_eq!(ws.agent_status, AgentStatus::Unknown);
}

#[test]
fn plugin_action_qualified_id() {
    let actions: Vec<PluginAction> =
        serde_json::from_value(result_field("plugin_action_list.json", "actions")).unwrap();
    assert_eq!(actions[1].qualified_id(), "hhdebb.herdr-radar.refresh");
    assert_eq!(actions[1].description.as_deref(), Some("Rescan agents"));
    assert_eq!(actions[0].description, None);
}

#[test]
fn agent_status_strings() {
    assert_eq!(AgentStatus::Idle.as_str(), "idle");
    assert_eq!(AgentStatus::Working.as_str(), "working");
    assert_eq!(AgentStatus::Blocked.as_str(), "blocked");
    assert_eq!(AgentStatus::Done.as_str(), "done");
    assert_eq!(AgentStatus::Unknown.as_str(), "unknown");
}

#[test]
fn parses_plugin_context() {
    let ctx = PluginContext::from_json(&fixture("context.json")).unwrap();
    assert_eq!(ctx.workspace_id.as_deref(), Some("w6"));
    assert_eq!(ctx.workspace_label.as_deref(), Some("herdr-plugins"));
    assert_eq!(ctx.tab_id.as_deref(), Some("w6:t1"));
    assert_eq!(ctx.tab_label.as_deref(), Some("1"));
    assert_eq!(ctx.focused_pane_id.as_deref(), Some("w6:p1"));
    assert_eq!(ctx.focused_pane_cwd.as_deref(), Some("/Users/me/herdr-plugins"));
}

#[test]
fn context_from_missing_or_invalid_env_is_default() {
    assert_eq!(PluginContext::from_env_value(None), PluginContext::default());
    assert_eq!(PluginContext::from_env_value(Some("not json")), PluginContext::default());
    let ctx = PluginContext::from_env_value(Some(r#"{"tab_id":"w1:t1"}"#));
    assert_eq!(ctx.tab_id.as_deref(), Some("w1:t1"));
    assert_eq!(ctx.workspace_id, None);
}
