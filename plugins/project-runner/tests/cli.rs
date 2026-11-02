//! Runs the built binary the way the Herdr app's mod does.

use std::process::{Command, Output};

use serde_json::Value;

fn runner(args: &[&str]) -> (Option<i32>, Value) {
    let Output { status, stdout, .. } = Command::new(env!("CARGO_BIN_EXE_project-runner")).args(args).output().unwrap();
    (status.code(), serde_json::from_slice(&stdout).unwrap())
}

#[test]
fn scripts_prints_json_and_exits_by_outcome() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"scripts": {"dev": "vite"}}"#).unwrap();
    let (code, value) = runner(&["scripts", "--cwd", project.path().to_str().unwrap()]);
    assert_eq!((code, &value["ok"]), (Some(0), &Value::Bool(true)), "{value}");

    let empty = tempfile::tempdir().unwrap();
    std::fs::create_dir(empty.path().join(".git")).unwrap();
    let (code, value) = runner(&["scripts", "--cwd", empty.path().to_str().unwrap()]);
    assert_eq!((code, &value["ok"]), (Some(1), &Value::Bool(false)), "{value}");

    let (code, value) = runner(&["scripts"]);
    assert_eq!((code, &value["ok"]), (Some(2), &Value::Bool(false)), "{value}");
}
