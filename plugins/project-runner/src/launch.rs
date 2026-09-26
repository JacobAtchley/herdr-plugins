//! Opens a pane for the chosen script and types its run line into it, so the
//! script runs in a normal shell: output stays visible, ctrl-c stops it, and
//! the shell history has the command for re-runs.

use std::path::Path;

use herdr_client::{Api, PluginContext};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Tab,
    SplitRight,
    SplitDown,
}

/// `label` names the new tab; `line` is what the shell runs.
pub fn launch(
    api: &dyn Api,
    ctx: &PluginContext,
    target: Target,
    dir: &Path,
    label: &str,
    line: &str,
) -> Result<(), String> {
    let call = |method: &str, params: Value| api.request(method, params).map_err(|e| e.to_string());
    let cwd = dir.to_string_lossy();
    let pane_id = match target {
        Target::Tab => {
            let workspace = need(&ctx.workspace_id, "workspace")?;
            let result =
                call("tab.create", json!({"workspace_id": workspace, "cwd": cwd, "label": label, "focus": true}))?;
            pane_id(&result, "/root_pane/pane_id", "tab.create")?
        }
        Target::SplitRight | Target::SplitDown => {
            let direction = if target == Target::SplitRight { "right" } else { "down" };
            let pane = need(&ctx.focused_pane_id, "pane")?;
            let result =
                call("pane.split", json!({"target_pane_id": pane, "direction": direction, "cwd": cwd, "focus": true}))?;
            pane_id(&result, "/pane/pane_id", "pane.split")?
        }
    };
    call("pane.send_input", json!({"pane_id": pane_id, "text": line, "keys": ["Enter"]})).map(drop)
}

fn need<'a>(value: &'a Option<String>, what: &str) -> Result<&'a str, String> {
    value.as_deref().ok_or_else(|| format!("no focused {what}"))
}

fn pane_id(result: &Value, pointer: &str, method: &str) -> Result<String, String> {
    result
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{method}: response has no {pointer}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeApi;

    fn ctx() -> PluginContext {
        PluginContext { workspace_id: Some("w1".into()), focused_pane_id: Some("w1:p1".into()), ..Default::default() }
    }

    fn send(pane: &str) -> (String, Value) {
        ("pane.send_input".into(), json!({"pane_id": pane, "text": "pnpm run dev", "keys": ["Enter"]}))
    }

    #[test]
    fn tab_target_creates_a_labelled_tab_and_types_the_line() {
        let api = FakeApi::new()
            .ok("tab.create", json!({"tab": {"tab_id": "w1:t4"}, "root_pane": {"pane_id": "w1:p9"}}))
            .ok("pane.send_input", json!({}));
        launch(&api, &ctx(), Target::Tab, Path::new("/repo"), "dev", "pnpm run dev").unwrap();
        assert_eq!(
            api.calls(),
            [
                ("tab.create".into(), json!({"workspace_id": "w1", "cwd": "/repo", "label": "dev", "focus": true})),
                send("w1:p9"),
            ]
        );
    }

    #[test]
    fn split_targets_split_the_focused_pane() {
        for (target, direction) in [(Target::SplitRight, "right"), (Target::SplitDown, "down")] {
            let api =
                FakeApi::new().ok("pane.split", json!({"pane": {"pane_id": "w1:p2"}})).ok("pane.send_input", json!({}));
            launch(&api, &ctx(), target, Path::new("/repo"), "dev", "pnpm run dev").unwrap();
            assert_eq!(
                api.calls(),
                [
                    (
                        "pane.split".into(),
                        json!({"target_pane_id": "w1:p1", "direction": direction, "cwd": "/repo", "focus": true})
                    ),
                    send("w1:p2"),
                ]
            );
        }
    }

    #[test]
    fn missing_context_skips_the_socket() {
        let api = FakeApi::new();
        let err = launch(&api, &PluginContext::default(), Target::Tab, Path::new("/r"), "a", "b").unwrap_err();
        assert_eq!(err, "no focused workspace");
        assert!(api.calls().is_empty());
    }

    #[test]
    fn unexpected_response_shape_is_an_error() {
        let api = FakeApi::new().ok("pane.split", json!({}));
        let err = launch(&api, &ctx(), Target::SplitDown, Path::new("/r"), "a", "b").unwrap_err();
        assert_eq!(err, "pane.split: response has no /pane/pane_id");
    }

    #[test]
    fn api_errors_are_returned_as_text() {
        let api = FakeApi::new().err("tab.create", "workspace_not_found", "gone");
        let err = launch(&api, &ctx(), Target::Tab, Path::new("/r"), "a", "b").unwrap_err();
        assert_eq!(err, "tab.create: gone (workspace_not_found)");
    }
}
