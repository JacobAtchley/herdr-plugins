//! Runs the built binary the way the Herdr app's mod does.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

fn glance(args: &[&str]) -> (Option<i32>, Value) {
    let Output { status, stdout, .. } = Command::new(env!("CARGO_BIN_EXE_git-glance")).args(args).output().unwrap();
    (status.code(), serde_json::from_slice(&stdout).unwrap())
}

fn git_init(dir: &Path) {
    let ok = Command::new("git").args(["init", "-q"]).current_dir(dir).status().unwrap().success();
    assert!(ok);
}

#[test]
fn status_prints_json_and_exits_by_outcome() {
    let repo = tempfile::tempdir().unwrap();
    git_init(repo.path());
    let (code, value) = glance(&["status", "--cwd", repo.path().to_str().unwrap()]);
    assert_eq!((code, &value["ok"]), (Some(0), &Value::Bool(true)), "{value}");

    let plain = tempfile::tempdir().unwrap();
    let (code, value) = glance(&["status", "--cwd", plain.path().to_str().unwrap()]);
    assert_eq!((code, &value["ok"]), (Some(1), &Value::Bool(false)), "{value}");

    let (code, value) = glance(&["status"]);
    assert_eq!((code, &value["ok"]), (Some(2), &Value::Bool(false)), "{value}");
}
