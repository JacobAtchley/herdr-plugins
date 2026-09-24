//! Curated herdr commands that map to socket API calls. Each one acts on the
//! context captured when the palette opened (the pane under the popup).

use herdr_client::PluginContext;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    NewTab,
    RenameTab,
    CloseTab,
    SplitRight,
    SplitDown,
    ToggleZoom,
    RenamePane,
    ClosePane,
    MovePaneToNewTab,
    MovePaneToNewWorkspace,
    CreateWorktree,
    OpenWorktree,
    ReloadConfig,
}

/// What the palette does after the user picks a built-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Run,
    Prompt { label: String, initial: String },
    Confirm { question: String },
}

impl Builtin {
    pub const ALL: [Builtin; 16] = [
        Builtin::NewWorkspace,
        Builtin::RenameWorkspace,
        Builtin::CloseWorkspace,
        Builtin::NewTab,
        Builtin::RenameTab,
        Builtin::CloseTab,
        Builtin::SplitRight,
        Builtin::SplitDown,
        Builtin::ToggleZoom,
        Builtin::RenamePane,
        Builtin::ClosePane,
        Builtin::MovePaneToNewTab,
        Builtin::MovePaneToNewWorkspace,
        Builtin::CreateWorktree,
        Builtin::OpenWorktree,
        Builtin::ReloadConfig,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Builtin::NewWorkspace => "New workspace",
            Builtin::RenameWorkspace => "Rename workspace",
            Builtin::CloseWorkspace => "Close workspace",
            Builtin::NewTab => "New tab",
            Builtin::RenameTab => "Rename tab",
            Builtin::CloseTab => "Close tab",
            Builtin::SplitRight => "Split pane right",
            Builtin::SplitDown => "Split pane down",
            Builtin::ToggleZoom => "Toggle pane zoom",
            Builtin::RenamePane => "Rename pane",
            Builtin::ClosePane => "Close pane",
            Builtin::MovePaneToNewTab => "Move pane to new tab",
            Builtin::MovePaneToNewWorkspace => "Move pane to new workspace",
            Builtin::CreateWorktree => "Create worktree",
            Builtin::OpenWorktree => "Open worktree",
            Builtin::ReloadConfig => "Reload herdr config",
        }
    }

    /// Stable id used as the frecency key (`cmd:<slug>`).
    pub fn slug(self) -> &'static str {
        match self {
            Builtin::NewWorkspace => "new-workspace",
            Builtin::RenameWorkspace => "rename-workspace",
            Builtin::CloseWorkspace => "close-workspace",
            Builtin::NewTab => "new-tab",
            Builtin::RenameTab => "rename-tab",
            Builtin::CloseTab => "close-tab",
            Builtin::SplitRight => "split-right",
            Builtin::SplitDown => "split-down",
            Builtin::ToggleZoom => "toggle-zoom",
            Builtin::RenamePane => "rename-pane",
            Builtin::ClosePane => "close-pane",
            Builtin::MovePaneToNewTab => "move-pane-new-tab",
            Builtin::MovePaneToNewWorkspace => "move-pane-new-workspace",
            Builtin::CreateWorktree => "create-worktree",
            Builtin::OpenWorktree => "open-worktree",
            Builtin::ReloadConfig => "reload-config",
        }
    }

    /// Extra words that match this command but are not shown.
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            Builtin::NewWorkspace | Builtin::NewTab | Builtin::CreateWorktree => &["create", "add"],
            Builtin::CloseWorkspace | Builtin::CloseTab | Builtin::ClosePane => &["kill", "remove"],
            Builtin::SplitRight => &["vertical"],
            Builtin::SplitDown => &["horizontal"],
            Builtin::ToggleZoom => &["maximize", "fullscreen"],
            Builtin::ReloadConfig => &["refresh", "settings"],
            _ => &[],
        }
    }

    pub fn step(self, ctx: &PluginContext) -> Step {
        match self {
            Builtin::NewWorkspace => prompt("New workspace label", None),
            Builtin::RenameWorkspace => prompt("Rename workspace", ctx.workspace_label.as_deref()),
            Builtin::CloseWorkspace => confirm("workspace", ctx.workspace_label.as_deref()),
            Builtin::NewTab => prompt("New tab label", None),
            Builtin::RenameTab => prompt("Rename tab", ctx.tab_label.as_deref()),
            Builtin::CloseTab => confirm("tab", ctx.tab_label.as_deref()),
            Builtin::RenamePane => prompt("Rename pane", None),
            Builtin::ClosePane => Step::Confirm { question: "Close the focused pane?".into() },
            Builtin::CreateWorktree => prompt("New worktree branch", None),
            Builtin::OpenWorktree => prompt("Open worktree branch", None),
            Builtin::SplitRight
            | Builtin::SplitDown
            | Builtin::ToggleZoom
            | Builtin::MovePaneToNewTab
            | Builtin::MovePaneToNewWorkspace
            | Builtin::ReloadConfig => Step::Run,
        }
    }

    /// The socket method and params for this command. `input` is the prompt
    /// text (empty for commands without a prompt).
    pub fn request(self, ctx: &PluginContext, input: &str) -> Result<(&'static str, Value), String> {
        let workspace = || need(&ctx.workspace_id, "workspace");
        let tab = || need(&ctx.tab_id, "tab");
        let pane = || need(&ctx.focused_pane_id, "pane");
        let cwd = &ctx.focused_pane_cwd;
        let label = optional(input);

        Ok(match self {
            Builtin::NewWorkspace => {
                ("workspace.create", json!({"cwd": cwd, "label": label, "focus": true}))
            }
            Builtin::RenameWorkspace => (
                "workspace.rename",
                json!({"workspace_id": workspace()?, "label": required(input, "label")?}),
            ),
            Builtin::CloseWorkspace => ("workspace.close", json!({"workspace_id": workspace()?})),
            Builtin::NewTab => (
                "tab.create",
                json!({"workspace_id": workspace()?, "cwd": cwd, "label": label, "focus": true}),
            ),
            Builtin::RenameTab => {
                ("tab.rename", json!({"tab_id": tab()?, "label": required(input, "label")?}))
            }
            Builtin::CloseTab => ("tab.close", json!({"tab_id": tab()?})),
            Builtin::SplitRight => split(pane()?, "right", cwd),
            Builtin::SplitDown => split(pane()?, "down", cwd),
            Builtin::ToggleZoom => ("pane.zoom", json!({"pane_id": pane()?, "mode": "toggle"})),
            Builtin::RenamePane => ("pane.rename", json!({"pane_id": pane()?, "label": label})),
            Builtin::ClosePane => ("pane.close", json!({"pane_id": pane()?})),
            Builtin::MovePaneToNewTab => (
                "pane.move",
                json!({
                    "pane_id": pane()?,
                    "destination": {"type": "new_tab", "workspace_id": workspace()?},
                    "focus": true
                }),
            ),
            Builtin::MovePaneToNewWorkspace => (
                "pane.move",
                json!({"pane_id": pane()?, "destination": {"type": "new_workspace"}, "focus": true}),
            ),
            Builtin::CreateWorktree => (
                "worktree.create",
                json!({"workspace_id": workspace()?, "branch": required(input, "branch")?, "focus": true}),
            ),
            Builtin::OpenWorktree => (
                "worktree.open",
                json!({"workspace_id": workspace()?, "branch": required(input, "branch")?, "focus": true}),
            ),
            Builtin::ReloadConfig => ("server.reload_config", json!({})),
        })
    }
}

fn prompt(label: &str, initial: Option<&str>) -> Step {
    Step::Prompt { label: label.to_string(), initial: initial.unwrap_or_default().to_string() }
}

fn confirm(what: &str, label: Option<&str>) -> Step {
    let question = match label {
        Some(label) => format!("Close {what} \"{label}\"?"),
        None => format!("Close the current {what}?"),
    };
    Step::Confirm { question }
}

fn split(pane: String, direction: &str, cwd: &Option<String>) -> (&'static str, Value) {
    (
        "pane.split",
        json!({"target_pane_id": pane, "direction": direction, "cwd": cwd, "focus": true}),
    )
}

fn need(value: &Option<String>, what: &str) -> Result<String, String> {
    value.clone().ok_or_else(|| format!("no focused {what}"))
}

fn optional(input: &str) -> Option<String> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn required(input: &str, what: &str) -> Result<String, String> {
    optional(input).ok_or_else(|| format!("{what} cannot be empty"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_client::PluginContext;
    use serde_json::json;

    fn ctx() -> PluginContext {
        PluginContext {
            workspace_id: Some("w6".into()),
            workspace_label: Some("herdr-plugins".into()),
            tab_id: Some("w6:t1".into()),
            tab_label: Some("Claude".into()),
            focused_pane_id: Some("w6:p1".into()),
            focused_pane_cwd: Some("/repo".into()),
            ..PluginContext::default()
        }
    }

    #[test]
    fn all_lists_every_builtin_once_with_unique_slugs() {
        let mut slugs: Vec<_> = Builtin::ALL.iter().map(|b| b.slug()).collect();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), 16);
    }

    #[test]
    fn requests_map_to_socket_methods() {
        let c = ctx();
        let cases: Vec<(Builtin, &str, &str, serde_json::Value)> = vec![
            (Builtin::NewWorkspace, "api", "workspace.create", json!({"cwd": "/repo", "label": "api", "focus": true})),
            (Builtin::NewWorkspace, "", "workspace.create", json!({"cwd": "/repo", "label": null, "focus": true})),
            (Builtin::RenameWorkspace, " core ", "workspace.rename", json!({"workspace_id": "w6", "label": "core"})),
            (Builtin::CloseWorkspace, "", "workspace.close", json!({"workspace_id": "w6"})),
            (Builtin::NewTab, "logs", "tab.create", json!({"workspace_id": "w6", "cwd": "/repo", "label": "logs", "focus": true})),
            (Builtin::RenameTab, "Review", "tab.rename", json!({"tab_id": "w6:t1", "label": "Review"})),
            (Builtin::CloseTab, "", "tab.close", json!({"tab_id": "w6:t1"})),
            (Builtin::SplitRight, "", "pane.split", json!({"target_pane_id": "w6:p1", "direction": "right", "cwd": "/repo", "focus": true})),
            (Builtin::SplitDown, "", "pane.split", json!({"target_pane_id": "w6:p1", "direction": "down", "cwd": "/repo", "focus": true})),
            (Builtin::ToggleZoom, "", "pane.zoom", json!({"pane_id": "w6:p1", "mode": "toggle"})),
            (Builtin::RenamePane, "server", "pane.rename", json!({"pane_id": "w6:p1", "label": "server"})),
            (Builtin::RenamePane, "", "pane.rename", json!({"pane_id": "w6:p1", "label": null})),
            (Builtin::ClosePane, "", "pane.close", json!({"pane_id": "w6:p1"})),
            (Builtin::MovePaneToNewTab, "", "pane.move", json!({"pane_id": "w6:p1", "destination": {"type": "new_tab", "workspace_id": "w6"}, "focus": true})),
            (Builtin::MovePaneToNewWorkspace, "", "pane.move", json!({"pane_id": "w6:p1", "destination": {"type": "new_workspace"}, "focus": true})),
            (Builtin::CreateWorktree, "feat/x", "worktree.create", json!({"workspace_id": "w6", "branch": "feat/x", "focus": true})),
            (Builtin::OpenWorktree, "main", "worktree.open", json!({"workspace_id": "w6", "branch": "main", "focus": true})),
            (Builtin::ReloadConfig, "", "server.reload_config", json!({})),
        ];
        for (builtin, input, method, params) in cases {
            assert_eq!(builtin.request(&c, input), Ok((method, params)), "{builtin:?} with {input:?}");
        }
    }

    #[test]
    fn required_input_must_not_be_blank() {
        let c = ctx();
        assert_eq!(Builtin::RenameTab.request(&c, "   "), Err("label cannot be empty".to_string()));
        assert_eq!(Builtin::RenameWorkspace.request(&c, ""), Err("label cannot be empty".to_string()));
        assert_eq!(Builtin::CreateWorktree.request(&c, ""), Err("branch cannot be empty".to_string()));
    }

    #[test]
    fn missing_context_is_an_error_not_a_panic() {
        let empty = PluginContext::default();
        assert_eq!(Builtin::CloseTab.request(&empty, ""), Err("no focused tab".to_string()));
        assert_eq!(Builtin::SplitRight.request(&empty, ""), Err("no focused pane".to_string()));
        assert_eq!(Builtin::CloseWorkspace.request(&empty, ""), Err("no focused workspace".to_string()));
        assert!(Builtin::ReloadConfig.request(&empty, "").is_ok());
        assert!(Builtin::NewWorkspace.request(&empty, "").is_ok());
    }

    #[test]
    fn steps_prompt_confirm_or_run() {
        let c = ctx();
        assert_eq!(Builtin::SplitRight.step(&c), Step::Run);
        assert_eq!(Builtin::ReloadConfig.step(&c), Step::Run);
        assert_eq!(
            Builtin::RenameTab.step(&c),
            Step::Prompt { label: "Rename tab".into(), initial: "Claude".into() }
        );
        assert_eq!(
            Builtin::RenameWorkspace.step(&c),
            Step::Prompt { label: "Rename workspace".into(), initial: "herdr-plugins".into() }
        );
        assert_eq!(
            Builtin::NewWorkspace.step(&c),
            Step::Prompt { label: "New workspace label".into(), initial: String::new() }
        );
        assert_eq!(
            Builtin::CreateWorktree.step(&c),
            Step::Prompt { label: "New worktree branch".into(), initial: String::new() }
        );
        assert_eq!(Builtin::CloseTab.step(&c), Step::Confirm { question: "Close tab \"Claude\"?".into() });
        assert_eq!(
            Builtin::CloseWorkspace.step(&c),
            Step::Confirm { question: "Close workspace \"herdr-plugins\"?".into() }
        );
        assert_eq!(Builtin::ClosePane.step(&c), Step::Confirm { question: "Close the focused pane?".into() });
    }

    #[test]
    fn confirm_without_labels_uses_generic_wording() {
        let empty = PluginContext::default();
        assert_eq!(
            Builtin::CloseTab.step(&empty),
            Step::Confirm { question: "Close the current tab?".into() }
        );
    }
}
