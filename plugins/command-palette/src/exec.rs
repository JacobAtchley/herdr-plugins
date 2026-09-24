//! Runs the selected item's action.

use std::fs::OpenOptions;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use herdr_client::{Api, PluginContext};
use serde_json::{Value, json};

use crate::item::Action;

/// `input` is the prompt text for built-ins that asked for one. `log_dir` is
/// where detached commands' stderr is appended (`palette.log`).
pub fn execute(
    api: &dyn Api,
    action: &Action,
    ctx: &PluginContext,
    input: &str,
    herdr_bin: &str,
    log_dir: &Path,
) -> Result<(), String> {
    let call = |method: &str, params: Value| api.request(method, params).map(drop).map_err(|e| e.to_string());
    match action {
        Action::FocusWorkspace(id) => call("workspace.focus", json!({"workspace_id": id})),
        Action::FocusTab(id) => call("tab.focus", json!({"tab_id": id})),
        Action::FocusPane(id) => call("agent.focus", json!({"target": id})),
        Action::Builtin(builtin) => {
            let (method, params) = builtin.request(ctx, input)?;
            call(method, params)
        }
        Action::InvokePluginAction(id) => spawn_detached(&mut delayed_plugin_invoke(herdr_bin, id), log_dir),
        Action::RunUser(cmd) => spawn_detached(&mut cmd.command(ctx.focused_pane_cwd.as_deref()), log_dir),
    }
}

/// The palette popup is a session singleton: invoking an action that opens its
/// own popup while the palette is still up fails with `ui_busy`. The delay lets
/// the palette exit (closing its popup) before herdr runs the action.
fn delayed_plugin_invoke(herdr_bin: &str, action_id: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "sleep 0.2; exec \"$0\" plugin action invoke \"$1\"", herdr_bin, action_id]);
    cmd
}

/// Starts a process that outlives the palette: no inherited stdio (the popup's
/// terminal goes away) and its own process group (no hangup when it does).
/// stderr goes to `palette.log` so a failing user `run` script or plugin
/// invocation is diagnosable; if the log can't be opened, stderr goes to null
/// rather than failing the action.
fn spawn_detached(cmd: &mut Command, log_dir: &Path) -> Result<(), String> {
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("palette.log"))
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(stderr).process_group(0);
    cmd.spawn().map(drop).map_err(|err| format!("failed to start {}: {err}", cmd.get_program().to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Action;
    use crate::sources::builtins::Builtin;
    use crate::sources::user::UserCommand;
    use crate::testing::FakeApi;
    use herdr_client::PluginContext;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn ctx() -> PluginContext {
        PluginContext {
            workspace_id: Some("w1".into()),
            tab_id: Some("w1:t1".into()),
            focused_pane_id: Some("w1:p1".into()),
            focused_pane_cwd: Some("/tmp".into()),
            ..Default::default()
        }
    }

    /// A log directory for tests that don't care where diagnostics land.
    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn wait_for(path: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(content) = std::fs::read_to_string(path)
                && !content.is_empty()
            {
                return content;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("{} never appeared", path.display());
    }

    #[test]
    fn focus_actions_call_the_matching_methods() {
        let api =
            FakeApi::new().ok("workspace.focus", json!({})).ok("tab.focus", json!({})).ok("agent.focus", json!({}));
        execute(&api, &Action::FocusWorkspace("w2".into()), &ctx(), "", "herdr", tmp().path()).unwrap();
        execute(&api, &Action::FocusTab("w2:t1".into()), &ctx(), "", "herdr", tmp().path()).unwrap();
        execute(&api, &Action::FocusPane("w2:p1".into()), &ctx(), "", "herdr", tmp().path()).unwrap();
        assert_eq!(
            api.calls(),
            [
                ("workspace.focus".to_string(), json!({"workspace_id": "w2"})),
                ("tab.focus".to_string(), json!({"tab_id": "w2:t1"})),
                ("agent.focus".to_string(), json!({"target": "w2:p1"})),
            ]
        );
    }

    #[test]
    fn builtin_sends_its_request_with_input() {
        let api = FakeApi::new().ok("tab.rename", json!({}));
        execute(&api, &Action::Builtin(Builtin::RenameTab), &ctx(), "Logs", "herdr", tmp().path()).unwrap();
        assert_eq!(api.calls(), [("tab.rename".to_string(), json!({"tab_id": "w1:t1", "label": "Logs"}))]);
    }

    #[test]
    fn builtin_validation_error_skips_the_socket() {
        let api = FakeApi::new();
        let err = execute(&api, &Action::Builtin(Builtin::RenameTab), &ctx(), "  ", "herdr", tmp().path()).unwrap_err();
        assert_eq!(err, "label cannot be empty");
        assert!(api.calls().is_empty());
    }

    #[test]
    fn stale_target_api_error_is_returned_as_text() {
        let api = FakeApi::new().err("tab.focus", "tab_not_found", "tab w1:t9 not found");
        let err = execute(&api, &Action::FocusTab("w1:t9".into()), &ctx(), "", "herdr", tmp().path()).unwrap_err();
        assert_eq!(err, "tab.focus: tab w1:t9 not found (tab_not_found)");
    }

    #[test]
    fn user_command_runs_detached() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran.txt");
        let cmd = UserCommand {
            title: "t".into(),
            run: Some(format!("pwd > '{}'", marker.display())),
            argv: None,
            keywords: vec![],
            cwd: None,
        };
        execute(&FakeApi::new(), &Action::RunUser(cmd), &ctx(), "", "herdr", dir.path()).unwrap();
        let cwd = wait_for(&marker);
        assert!(cwd.trim().ends_with("tmp"), "ran in {cwd}");
    }

    #[test]
    fn detached_command_stderr_goes_to_palette_log() {
        let dir = tempfile::tempdir().unwrap();
        let cmd = UserCommand {
            title: "t".into(),
            run: Some("echo oops >&2".into()),
            argv: None,
            keywords: vec![],
            cwd: None,
        };
        execute(&FakeApi::new(), &Action::RunUser(cmd), &ctx(), "", "herdr", dir.path()).unwrap();
        let log = wait_for(&dir.path().join("palette.log"));
        assert!(log.contains("oops"), "{log}");
    }

    #[test]
    fn user_command_with_missing_cwd_is_an_error() {
        let cmd = UserCommand {
            title: "t".into(),
            run: None,
            argv: Some(vec!["true".into()]),
            keywords: vec![],
            cwd: Some("/definitely/not/here".into()),
        };
        let err = execute(&FakeApi::new(), &Action::RunUser(cmd), &ctx(), "", "herdr", tmp().path()).unwrap_err();
        assert!(err.starts_with("failed to start"), "{err}");
    }

    #[test]
    fn plugin_action_is_invoked_through_herdr_bin_after_a_delay() {
        let dir = tempfile::tempdir().unwrap();
        let fake_herdr = dir.path().join("fake-herdr");
        let args_file = dir.path().join("args.txt");
        std::fs::write(&fake_herdr, format!("#!/bin/sh\necho \"$@\" > '{}'\n", args_file.display())).unwrap();
        std::fs::set_permissions(&fake_herdr, std::fs::Permissions::from_mode(0o755)).unwrap();

        let api = FakeApi::new();
        execute(
            &api,
            &Action::InvokePluginAction("a.b.c".into()),
            &ctx(),
            "",
            fake_herdr.to_str().unwrap(),
            dir.path(),
        )
        .unwrap();
        assert!(!args_file.exists(), "invocation must be delayed until the palette exits");
        assert_eq!(wait_for(&args_file).trim(), "plugin action invoke a.b.c");
        assert!(api.calls().is_empty());
    }
}
