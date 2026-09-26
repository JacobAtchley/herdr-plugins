//! Runs git for one repository. Every call is a short-lived `git` process
//! with no terminal: prompts are disabled so nothing can block the popup.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::status::{self, Entry, Section, Snapshot};

/// Diffs beyond this are cut off: the popup only needs a glance.
const MAX_DIFF_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    /// e.g. "3 days ago".
    pub age: String,
    /// e.g. "[ahead 1, behind 2]", or empty.
    pub track: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    /// Position in the stash list (`stash@{index}`).
    pub index: usize,
    pub age: String,
    pub subject: String,
}

pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// Finds the repository containing `cwd`.
    pub fn discover(cwd: &Path) -> Result<Self, String> {
        let out = run_in(cwd, &["rev-parse", "--show-toplevel"])?;
        let root = String::from_utf8_lossy(&out).trim_end_matches('\n').to_string();
        Ok(Self { root: PathBuf::from(root) })
    }

    #[cfg(test)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        self.root.file_name().map_or_else(|| self.root.display().to_string(), |n| n.to_string_lossy().into_owned())
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        status::parse(&self.git(&["status", "--porcelain=v2", "--branch", "--show-stash", "-z"])?)
    }

    pub fn stage(&self, path: &str) -> Result<(), String> {
        self.git(&["add", "-A", "--", path]).map(drop)
    }

    /// Before the first commit there is no HEAD to restore from, so the
    /// path is dropped from the index instead.
    pub fn unstage(&self, path: &str, unborn: bool) -> Result<(), String> {
        let args: &[&str] =
            if unborn { &["rm", "-r", "--cached", "-q", "--", path] } else { &["restore", "--staged", "--", path] };
        self.git(args).map(drop)
    }

    pub fn stage_all(&self) -> Result<(), String> {
        self.git(&["add", "-A"]).map(drop)
    }

    pub fn unstage_all(&self, unborn: bool) -> Result<(), String> {
        let args: &[&str] = if unborn { &["rm", "-r", "--cached", "-q", "--", "."] } else { &["reset", "-q"] };
        self.git(args).map(drop)
    }

    /// Returns the new commit's short hash.
    pub fn commit(&self, message: &str) -> Result<String, String> {
        self.git(&["commit", "-q", "-m", message])?;
        let out = self.git(&["rev-parse", "--short", "HEAD"])?;
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }

    pub fn diff(&self, entry: &Entry) -> Result<String, String> {
        let out = match entry.section {
            Section::Staged => self.git(&["diff", "--cached", "--no-ext-diff", "--", &entry.path])?,
            Section::Unstaged | Section::Conflict => self.git(&["diff", "--no-ext-diff", "--", &entry.path])?,
            Section::Untracked if entry.path.ends_with('/') => {
                self.git(&["ls-files", "--others", "--exclude-standard", "--", &entry.path])?
            }
            // `--no-index` exits 1 when the files differ, which they always do here.
            Section::Untracked => self.git_allow(&["diff", "--no-index", "--", "/dev/null", &entry.path], &[1])?,
        };
        Ok(truncated(out))
    }

    /// Local branches, most recently committed first.
    pub fn branches(&self) -> Result<Vec<Branch>, String> {
        let out = self.git(&[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(HEAD)%00%(refname:short)%00%(committerdate:relative)%00%(upstream:track)",
            "refs/heads",
        ])?;
        Ok(String::from_utf8_lossy(&out)
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\0');
                let (head, name, age, track) = (f.next()?, f.next()?, f.next()?, f.next().unwrap_or(""));
                Some(Branch { name: name.into(), current: head == "*", age: age.into(), track: track.into() })
            })
            .collect())
    }

    pub fn switch(&self, name: &str, create: bool) -> Result<(), String> {
        let args: &[&str] = if create { &["switch", "-q", "-c", name] } else { &["switch", "-q", name] };
        self.git(args).map(drop)
    }

    pub fn stashes(&self) -> Result<Vec<Stash>, String> {
        let out = self.git(&["stash", "list", "--format=%cr%x00%gs"])?;
        Ok(String::from_utf8_lossy(&out)
            .lines()
            .enumerate()
            .map(|(index, line)| {
                let (age, subject) = line.split_once('\0').unwrap_or(("", line));
                Stash { index, age: age.into(), subject: subject.into() }
            })
            .collect())
    }

    /// Stashes tracked and untracked changes.
    pub fn stash_push(&self, message: &str) -> Result<(), String> {
        let mut args = vec!["stash", "push", "-q", "--include-untracked"];
        if !message.is_empty() {
            args.extend(["-m", message]);
        }
        self.git(&args).map(drop)
    }

    pub fn stash(&self, verb: StashVerb, index: usize) -> Result<(), String> {
        let target = format!("stash@{{{index}}}");
        self.git(&["stash", verb.as_str(), "-q", &target]).map(drop)
    }

    fn git(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        self.git_allow(args, &[])
    }

    fn git_allow(&self, args: &[&str], ok_codes: &[i32]) -> Result<Vec<u8>, String> {
        run(&self.root, args, ok_codes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StashVerb {
    Pop,
    Apply,
    Drop,
}

impl StashVerb {
    pub fn as_str(self) -> &'static str {
        match self {
            StashVerb::Pop => "pop",
            StashVerb::Apply => "apply",
            StashVerb::Drop => "drop",
        }
    }
}

fn run_in(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    run(cwd, args, &[])
}

fn run(cwd: &Path, args: &[&str], ok_codes: &[i32]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        // Read-only commands skip the optional index refresh lock, so glancing
        // never contends with an editor or agent running git in the same repo.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    let code = out.status.code();
    if out.status.success() || code.is_some_and(|c| ok_codes.contains(&c)) {
        return Ok(out.stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let message = stderr.trim();
    Err(if message.is_empty() { format!("git {} failed ({})", args[0], out.status) } else { message.to_string() })
}

fn truncated(mut out: Vec<u8>) -> String {
    let cut = out.len() > MAX_DIFF_BYTES;
    out.truncate(MAX_DIFF_BYTES);
    let mut text = String::from_utf8_lossy(&out).into_owned();
    if cut {
        text.push_str("\n… diff truncated");
    }
    text
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::fs;

    /// A throwaway repository with a fixed identity, independent of the
    /// developer's global git config.
    pub struct TestRepo {
        pub dir: tempfile::TempDir,
        pub repo: Repo,
    }

    impl TestRepo {
        pub fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let sh = |args: &[&str]| run_in(dir.path(), args).unwrap();
            sh(&["init", "-q", "-b", "main"]);
            sh(&["config", "user.name", "Test"]);
            sh(&["config", "user.email", "test@example.com"]);
            sh(&["config", "commit.gpgsign", "false"]);
            let repo = Repo::discover(dir.path()).unwrap();
            Self { dir, repo }
        }

        pub fn write(&self, path: &str, content: &str) {
            let path = self.dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        pub fn commit_all(&self, message: &str) {
            self.repo.stage_all().unwrap();
            self.repo.commit(message).unwrap();
        }
    }

    fn paths(snapshot: &Snapshot, section: Section) -> Vec<&str> {
        snapshot.entries.iter().filter(|e| e.section == section).map(|e| e.path.as_str()).collect()
    }

    #[test]
    fn discover_finds_the_root_from_a_subdirectory() {
        let t = TestRepo::new();
        t.write("a/b/file.txt", "x");
        let repo = Repo::discover(&t.dir.path().join("a/b")).unwrap();
        assert_eq!(repo.root().canonicalize().unwrap(), t.dir.path().canonicalize().unwrap());
        assert_eq!(repo.name(), t.dir.path().file_name().unwrap().to_string_lossy());
    }

    #[test]
    fn discover_outside_a_repo_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = Repo::discover(dir.path()).err().unwrap();
        assert!(err.contains("not a git repository"), "{err}");
    }

    #[test]
    fn stage_and_unstage_before_and_after_the_first_commit() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        let s = t.repo.snapshot().unwrap();
        assert!(s.is_unborn());
        assert_eq!(paths(&s, Section::Untracked), ["a.txt"]);

        t.repo.stage("a.txt").unwrap();
        assert_eq!(paths(&t.repo.snapshot().unwrap(), Section::Staged), ["a.txt"]);
        t.repo.unstage("a.txt", true).unwrap();
        assert_eq!(paths(&t.repo.snapshot().unwrap(), Section::Untracked), ["a.txt"]);

        t.commit_all("first");
        t.write("a.txt", "2");
        t.repo.stage("a.txt").unwrap();
        let s = t.repo.snapshot().unwrap();
        assert!(!s.is_unborn());
        assert_eq!(paths(&s, Section::Staged), ["a.txt"]);
        t.repo.unstage("a.txt", false).unwrap();
        assert_eq!(paths(&t.repo.snapshot().unwrap(), Section::Unstaged), ["a.txt"]);
    }

    #[test]
    fn staging_a_deleted_file_records_the_deletion() {
        let t = TestRepo::new();
        t.write("gone.txt", "x");
        t.commit_all("first");
        fs::remove_file(t.dir.path().join("gone.txt")).unwrap();
        t.repo.stage("gone.txt").unwrap();
        let s = t.repo.snapshot().unwrap();
        assert_eq!(s.entries.len(), 1);
        assert_eq!((s.entries[0].section, s.entries[0].code), (Section::Staged, 'D'));
    }

    #[test]
    fn stage_all_unstage_all_and_commit() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        t.write("b.txt", "1");
        t.repo.stage_all().unwrap();
        assert_eq!(paths(&t.repo.snapshot().unwrap(), Section::Staged), ["a.txt", "b.txt"]);
        t.repo.unstage_all(true).unwrap();
        assert_eq!(paths(&t.repo.snapshot().unwrap(), Section::Untracked), ["a.txt", "b.txt"]);

        t.repo.stage_all().unwrap();
        let hash = t.repo.commit("first").unwrap();
        assert!(hash.len() >= 7, "{hash}");
        let s = t.repo.snapshot().unwrap();
        assert!(s.entries.is_empty());
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        assert_eq!(s.branch.oid.as_deref(), Some(&hash[..7]));

        t.write("a.txt", "2");
        t.repo.stage_all().unwrap();
        t.repo.unstage_all(false).unwrap();
        assert_eq!(paths(&t.repo.snapshot().unwrap(), Section::Unstaged), ["a.txt"]);
    }

    #[test]
    fn commit_failure_reports_git_stderr() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        t.commit_all("first");
        let err = t.repo.commit("nothing").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn diffs_for_each_section() {
        let t = TestRepo::new();
        t.write("a.txt", "one\n");
        t.commit_all("first");
        t.write("a.txt", "two\n");
        t.write("new.txt", "fresh\n");
        t.write("dir/inner.txt", "x");

        let s = t.repo.snapshot().unwrap();
        let find = |path: &str| s.entries.iter().find(|e| e.path == path).unwrap().clone();

        let unstaged = t.repo.diff(&find("a.txt")).unwrap();
        assert!(unstaged.contains("-one") && unstaged.contains("+two"), "{unstaged}");
        let untracked = t.repo.diff(&find("new.txt")).unwrap();
        assert!(untracked.contains("+fresh"), "{untracked}");
        let dir = t.repo.diff(&find("dir/")).unwrap();
        assert_eq!(dir.trim(), "dir/inner.txt");

        t.repo.stage("a.txt").unwrap();
        let s = t.repo.snapshot().unwrap();
        let staged = t.repo.diff(s.entries.iter().find(|e| e.section == Section::Staged).unwrap()).unwrap();
        assert!(staged.contains("+two"), "{staged}");
    }

    #[test]
    fn large_diffs_are_truncated() {
        let text = truncated(vec![b'x'; MAX_DIFF_BYTES + 10]);
        assert!(text.ends_with("… diff truncated"));
        assert_eq!(truncated(b"small".to_vec()), "small");
    }

    #[test]
    fn branches_list_switch_and_create() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        t.commit_all("first");
        t.repo.switch("feature", true).unwrap();
        let branches = t.repo.branches().unwrap();
        let names: Vec<_> = branches.iter().map(|b| (b.name.as_str(), b.current)).collect();
        assert!(names.contains(&("feature", true)), "{names:?}");
        assert!(names.contains(&("main", false)), "{names:?}");
        assert!(branches.iter().all(|b| !b.age.is_empty()));

        t.repo.switch("main", false).unwrap();
        assert_eq!(t.repo.snapshot().unwrap().branch.head.as_deref(), Some("main"));
        assert!(t.repo.switch("nope", false).is_err());
    }

    #[test]
    fn stash_push_list_apply_pop_and_drop() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        t.commit_all("first");

        t.write("a.txt", "2");
        t.write("untracked.txt", "u");
        t.repo.stash_push("wip one").unwrap();
        let s = t.repo.snapshot().unwrap();
        assert!(s.entries.is_empty(), "untracked files are stashed too: {s:?}");
        assert_eq!(s.stashes, 1);

        t.write("a.txt", "3");
        t.repo.stash_push("").unwrap();
        let stashes = t.repo.stashes().unwrap();
        assert_eq!(stashes.len(), 2);
        assert_eq!(stashes[0].index, 0);
        assert!(stashes[0].subject.starts_with("WIP on main"), "{stashes:?}");
        assert!(stashes[1].subject.ends_with("wip one"), "{stashes:?}");

        t.repo.stash(StashVerb::Drop, 0).unwrap();
        t.repo.stash(StashVerb::Apply, 0).unwrap();
        assert_eq!(t.repo.stashes().unwrap().len(), 1);
        assert!(!t.repo.snapshot().unwrap().entries.is_empty());

        t.repo.stash_push("again").unwrap();
        t.repo.stash(StashVerb::Pop, 0).unwrap();
        assert_eq!(t.repo.stashes().unwrap().len(), 1);
    }
}
