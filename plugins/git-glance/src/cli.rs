//! Headless commands for other front ends (the Herdr app's mod): each call
//! runs one `Op` against the repository at `--cwd` and prints one JSON object.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use serde_json::{Value, json};

use crate::app::{Op, Outcome};
use crate::git::{Repo, StashVerb};
use crate::ops;
use crate::status::{Section, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Status,
    Stage(String),
    Unstage(String),
    StageAll,
    UnstageAll,
    Commit(String),
    Diff { path: String, section: Section },
    Branches,
    Switch { name: String, create: bool },
    Stashes,
    StashPush(String),
    Stash(StashVerb, usize),
}

/// Prints the result of one command; exits 1 when git refused it and 2 when
/// the arguments made no sense.
pub fn main(args: &[String]) -> ExitCode {
    let (value, code) = match parse(args) {
        Ok((cwd, request)) => {
            let value = execute(&cwd, &request);
            let code = if value["ok"] == true { 0 } else { 1 };
            (value, code)
        }
        Err(error) => (json!({ "ok": false, "error": error }), 2),
    };
    println!("{value}");
    ExitCode::from(code)
}

/// Options may come anywhere; what's left is the command and its operands.
pub fn parse(args: &[String]) -> Result<(PathBuf, Request), String> {
    let (mut cwd, mut message, mut section, mut create) = (None, None, None, false);
    let mut words = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || iter.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--cwd" => cwd = Some(PathBuf::from(value()?)),
            "--message" => message = Some(value()?),
            "--section" => section = Some(parse_section(&value()?)?),
            "--create" => create = true,
            _ => words.push(arg.as_str()),
        }
    }
    let cwd = cwd.ok_or("missing --cwd <dir>")?;
    let request = match words.as_slice() {
        ["status"] => Request::Status,
        ["stage", path] => Request::Stage(path.to_string()),
        ["unstage", path] => Request::Unstage(path.to_string()),
        ["stage-all"] => Request::StageAll,
        ["unstage-all"] => Request::UnstageAll,
        ["commit"] => Request::Commit(message.ok_or("commit needs --message <text>")?),
        ["diff", path] => {
            Request::Diff { path: path.to_string(), section: section.ok_or("diff needs --section <section>")? }
        }
        ["branches"] => Request::Branches,
        ["switch", name] => Request::Switch { name: name.to_string(), create },
        ["stashes"] => Request::Stashes,
        ["stash-push"] => Request::StashPush(message.unwrap_or_default()),
        ["stash", verb, index] => {
            let verb = match *verb {
                "pop" => StashVerb::Pop,
                "apply" => StashVerb::Apply,
                "drop" => StashVerb::Drop,
                _ => return Err(format!("unknown stash verb: {verb}")),
            };
            Request::Stash(verb, index.parse().map_err(|_| format!("bad stash index: {index}"))?)
        }
        [] => return Err("missing command".into()),
        _ => return Err(format!("unknown command: {}", words.join(" "))),
    };
    Ok((cwd, request))
}

fn parse_section(name: &str) -> Result<Section, String> {
    Ok(match name {
        "conflict" => Section::Conflict,
        "staged" => Section::Staged,
        "unstaged" => Section::Unstaged,
        "untracked" => Section::Untracked,
        _ => return Err(format!("unknown section: {name}")),
    })
}

/// Never fails: a git error becomes `{"ok": false, "error": ...}`.
pub fn execute(cwd: &Path, request: &Request) -> Value {
    run(cwd, request).unwrap_or_else(|error| json!({ "ok": false, "error": error }))
}

fn run(cwd: &Path, request: &Request) -> Result<Value, String> {
    let repo = Repo::discover(cwd)?;
    let op = match request {
        Request::Status => Op::Refresh,
        Request::Stage(path) => Op::Stage(path.clone()),
        Request::Unstage(path) => Op::Unstage { path: path.clone(), unborn: repo.snapshot()?.is_unborn() },
        Request::StageAll => Op::StageAll,
        Request::UnstageAll => Op::UnstageAll { unborn: repo.snapshot()?.is_unborn() },
        Request::Commit(message) => {
            let message = message.trim();
            if message.is_empty() {
                return Err("commit message cannot be empty".into());
            }
            Op::Commit(message.to_string())
        }
        // The entry comes from git, not the caller, so a rename's source path is right.
        Request::Diff { path, section } => {
            let snapshot = repo.snapshot()?;
            let entry = snapshot.entries.iter().find(|e| e.section == *section && e.path == *path);
            Op::Diff(entry.cloned().ok_or_else(|| format!("no {} change for {path}", section.title().to_lowercase()))?)
        }
        Request::Branches => Op::ListBranches,
        Request::Switch { name, create } => Op::Switch { name: name.clone(), create: *create },
        Request::Stashes => Op::ListStashes,
        Request::StashPush(message) => Op::StashPush(message.trim().to_string()),
        Request::Stash(verb, index) => Op::Stash(*verb, *index),
    };
    let (outcome, notice) = ops::run(&repo, &op)?;
    let mut value = json!({
        "ok": true,
        "repo": repo.name(),
        "root": repo.root().to_string_lossy(),
        "notice": notice,
    });
    let (key, outcome) = match outcome {
        Outcome::Snapshot(snapshot) => ("snapshot", snapshot_json(&snapshot)),
        Outcome::Diff { title, text } => ("diff", json!({ "title": title, "text": text })),
        Outcome::Branches(branches) => ("branches", json!(branches)),
        Outcome::Stashes(stashes) => ("stashes", json!(stashes)),
    };
    value[key] = outcome;
    Ok(value)
}

/// Spells out what the TUI derives from a snapshot, so callers never infer git state.
fn snapshot_json(snapshot: &Snapshot) -> Value {
    #[derive(Serialize)]
    struct Json<'a> {
        #[serde(flatten)]
        snapshot: &'a Snapshot,
        unborn: bool,
        has_staged: bool,
    }
    json!(Json { snapshot, unborn: snapshot.is_unborn(), has_staged: snapshot.has_staged() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::TestRepo;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parse_reads_cwd_anywhere_and_rejects_unknown() {
        assert_eq!(
            parse(&args(&["stage", "--cwd", "/r", "my file.txt"])).unwrap(),
            (PathBuf::from("/r"), Request::Stage("my file.txt".into()))
        );
        assert_eq!(
            parse(&args(&["--cwd", "/r", "commit", "--message", "fix: x"])).unwrap().1,
            Request::Commit("fix: x".into())
        );
        assert!(parse(&args(&["status"])).unwrap_err().contains("--cwd"));
        assert!(parse(&args(&["frobnicate", "--cwd", "/r"])).unwrap_err().contains("frobnicate"));
    }

    #[test]
    fn status_reports_sections_unborn_and_repo_name() {
        let t = TestRepo::new();
        t.write("naïve file.txt", "x");
        let v = execute(t.dir.path(), &Request::Status);
        assert_eq!(v["ok"], true);
        assert_eq!(v["snapshot"]["unborn"], true);
        assert_eq!(v["snapshot"]["entries"][0]["section"], "untracked");
        assert_eq!(v["snapshot"]["entries"][0]["path"], "naïve file.txt");
        assert_eq!(v["repo"], t.repo.name());
        assert_eq!(
            Path::new(v["root"].as_str().unwrap()).canonicalize().unwrap(),
            t.dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn stage_then_unstage_in_an_unborn_repo() {
        let t = TestRepo::new();
        t.write("a.txt", "x");
        let v = execute(t.dir.path(), &Request::Stage("a.txt".into()));
        assert_eq!(v["snapshot"]["has_staged"], true);
        let v = execute(t.dir.path(), &Request::Unstage("a.txt".into()));
        assert_eq!(v["ok"], true, "{v}");
        assert_eq!(v["snapshot"]["has_staged"], false);
    }

    #[test]
    fn stage_all_and_unstage_all_after_first_commit() {
        let t = TestRepo::new();
        t.write("a.txt", "x");
        t.write("b.txt", "x");
        t.commit_all("first");
        t.write("a.txt", "y");
        t.write("b.txt", "y");
        let v = execute(t.dir.path(), &Request::StageAll);
        let staged = |v: &Value| {
            v["snapshot"]["entries"].as_array().unwrap().iter().filter(|e| e["section"] == "staged").count()
        };
        assert_eq!(staged(&v), 2);
        let v = execute(t.dir.path(), &Request::UnstageAll);
        assert_eq!(staged(&v), 0);
    }

    #[test]
    fn commit_returns_notice_and_clean_snapshot() {
        let t = TestRepo::new();
        t.write("a.txt", "x");
        execute(t.dir.path(), &Request::StageAll);
        let v = execute(t.dir.path(), &Request::Commit("first".into()));
        assert!(v["notice"].as_str().unwrap().starts_with("committed "), "{v}");
        assert_eq!(v["snapshot"]["entries"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn failures_are_ok_false_with_message() {
        let dir = tempfile::tempdir().unwrap();
        let v = execute(dir.path(), &Request::Status);
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("not a git repository"), "{v}");
        let t = TestRepo::new();
        assert_eq!(execute(t.dir.path(), &Request::Commit("x".into()))["ok"], false);
        assert_eq!(execute(t.dir.path(), &Request::Commit("  ".into()))["error"], "commit message cannot be empty");
    }

    #[test]
    fn parse_reads_the_task_two_commands() {
        let p = |s: &[&str]| parse(&args(s)).map(|(_, r)| r);
        assert_eq!(
            p(&["diff", "b c.txt", "--section", "staged", "--cwd", "/r"]).unwrap(),
            Request::Diff { path: "b c.txt".into(), section: Section::Staged }
        );
        assert!(p(&["diff", "x", "--section", "bogus", "--cwd", "/r"]).is_err());
        assert!(p(&["diff", "x", "--cwd", "/r"]).is_err());
        assert_eq!(
            p(&["switch", "feat", "--create", "--cwd", "/r"]).unwrap(),
            Request::Switch { name: "feat".into(), create: true }
        );
        assert_eq!(
            p(&["switch", "feat", "--cwd", "/r"]).unwrap(),
            Request::Switch { name: "feat".into(), create: false }
        );
        assert_eq!(p(&["branches", "--cwd", "/r"]).unwrap(), Request::Branches);
        assert_eq!(p(&["stashes", "--cwd", "/r"]).unwrap(), Request::Stashes);
        assert_eq!(p(&["stash-push", "--cwd", "/r"]).unwrap(), Request::StashPush(String::new()));
        assert_eq!(p(&["stash-push", "--message", "wip", "--cwd", "/r"]).unwrap(), Request::StashPush("wip".into()));
        assert_eq!(p(&["stash", "apply", "2", "--cwd", "/r"]).unwrap(), Request::Stash(StashVerb::Apply, 2));
        assert!(p(&["stash", "pop", "notanumber", "--cwd", "/r"]).is_err());
        assert!(p(&["stash", "explode", "0", "--cwd", "/r"]).is_err());
    }

    #[test]
    fn diff_finds_the_entry_by_path_and_section() {
        let t = TestRepo::new();
        t.write("a.txt", "x\n");
        t.commit_all("first");
        let git = |a: &[&str]| {
            assert!(std::process::Command::new("git").args(a).current_dir(t.dir.path()).status().unwrap().success())
        };
        git(&["mv", "a.txt", "b c.txt"]);
        let v = execute(t.dir.path(), &Request::Diff { path: "b c.txt".into(), section: Section::Staged });
        assert_eq!(v["ok"], true, "{v}");
        assert_eq!(v["diff"]["title"], "a.txt → b c.txt");
    }

    #[test]
    fn diff_of_missing_entry_is_an_error() {
        let t = TestRepo::new();
        let v = execute(t.dir.path(), &Request::Diff { path: "nope".into(), section: Section::Unstaged });
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("nope"), "{v}");
    }

    #[test]
    fn switch_create_then_list_branches() {
        let t = TestRepo::new();
        t.write("a.txt", "x");
        t.commit_all("first");
        let v = execute(t.dir.path(), &Request::Switch { name: "feat".into(), create: true });
        assert_eq!(v["notice"], "created feat", "{v}");
        let v = execute(t.dir.path(), &Request::Branches);
        let feat = v["branches"].as_array().unwrap().iter().find(|b| b["name"] == "feat").cloned().unwrap();
        assert_eq!(feat["current"], true);
    }

    #[test]
    fn switch_blocked_by_local_changes_is_an_error() {
        let t = TestRepo::new();
        t.write("a.txt", "base\n");
        t.commit_all("first");
        execute(t.dir.path(), &Request::Switch { name: "other".into(), create: true });
        t.write("a.txt", "other\n");
        t.commit_all("other");
        execute(t.dir.path(), &Request::Switch { name: "main".into(), create: false });
        t.write("a.txt", "dirty\n");
        let v = execute(t.dir.path(), &Request::Switch { name: "other".into(), create: false });
        assert_eq!(v["ok"], false, "{v}");
    }

    #[test]
    fn stash_push_pop_round_trip() {
        let t = TestRepo::new();
        t.write("a.txt", "x");
        t.commit_all("first");
        t.write("a.txt", "y");
        let v = execute(t.dir.path(), &Request::StashPush("wip".into()));
        assert_eq!(v["notice"], "stashed changes", "{v}");
        assert_eq!(v["snapshot"]["entries"].as_array().unwrap().len(), 0);
        let v = execute(t.dir.path(), &Request::Stashes);
        assert!(v["stashes"][0]["subject"].as_str().unwrap().contains("wip"), "{v}");
        let v = execute(t.dir.path(), &Request::Stash(StashVerb::Pop, 0));
        assert_eq!(v["snapshot"]["entries"][0]["path"], "a.txt", "{v}");
    }

    #[test]
    fn stash_drop_returns_the_list_and_bad_index_errors() {
        let t = TestRepo::new();
        t.write("a.txt", "x");
        t.commit_all("first");
        t.write("a.txt", "y");
        execute(t.dir.path(), &Request::StashPush(String::new()));
        let v = execute(t.dir.path(), &Request::Stash(StashVerb::Drop, 0));
        assert_eq!(v["stashes"].as_array().unwrap().len(), 0, "{v}");
        assert_eq!(execute(t.dir.path(), &Request::Stash(StashVerb::Drop, 5))["ok"], false);
    }

    #[test]
    fn status_does_not_touch_the_index() {
        let t = TestRepo::new();
        t.write("a.txt", "x");
        t.commit_all("first");
        t.write("a.txt", "y");
        let index = t.dir.path().join(".git/index");
        let before = std::fs::metadata(&index).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(execute(t.dir.path(), &Request::Status)["ok"], true);
        assert_eq!(std::fs::metadata(&index).unwrap().modified().unwrap(), before);
    }
}
