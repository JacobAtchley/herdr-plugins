//! Carries out an `Op` against the repository.

use crate::app::{Op, Outcome};
use crate::git::{Repo, StashVerb};

/// Returns the outcome plus an optional notice for the footer. Every op that
/// changes the repository ends with a fresh snapshot.
pub fn run(repo: &Repo, op: &Op) -> Result<(Outcome, Option<String>), String> {
    let snapshot = |notice: Option<String>| Ok((Outcome::Snapshot(repo.snapshot()?), notice));
    match op {
        Op::Refresh => snapshot(None),
        Op::Stage(path) => {
            repo.stage(path)?;
            snapshot(None)
        }
        Op::Unstage { path, unborn } => {
            repo.unstage(path, *unborn)?;
            snapshot(None)
        }
        Op::StageAll => {
            repo.stage_all()?;
            snapshot(None)
        }
        Op::UnstageAll { unborn } => {
            repo.unstage_all(*unborn)?;
            snapshot(None)
        }
        Op::Commit(message) => {
            let hash = repo.commit(message)?;
            snapshot(Some(format!("committed {hash}")))
        }
        Op::Diff(entry) => {
            let title = match &entry.orig {
                Some(orig) => format!("{orig} → {}", entry.path),
                None => entry.path.clone(),
            };
            Ok((Outcome::Diff { title, text: repo.diff(entry)? }, None))
        }
        Op::ListBranches => Ok((Outcome::Branches(repo.branches()?), None)),
        Op::Switch { name, create } => {
            repo.switch(name, *create)?;
            snapshot(Some(format!("{} {name}", if *create { "created" } else { "switched to" })))
        }
        Op::ListStashes => Ok((Outcome::Stashes(repo.stashes()?), None)),
        Op::StashPush(message) => {
            repo.stash_push(message)?;
            snapshot(Some("stashed changes".into()))
        }
        Op::Stash(verb, index) => {
            repo.stash(*verb, *index)?;
            let notice = Some(format!("{} stash@{{{index}}}", past_tense(*verb)));
            match verb {
                // Dropping changes nothing on disk, so stay in the stash list.
                StashVerb::Drop => Ok((Outcome::Stashes(repo.stashes()?), notice)),
                StashVerb::Pop | StashVerb::Apply => snapshot(notice),
            }
        }
    }
}

fn past_tense(verb: StashVerb) -> &'static str {
    match verb {
        StashVerb::Pop => "popped",
        StashVerb::Apply => "applied",
        StashVerb::Drop => "dropped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::TestRepo;
    use crate::status::Section;

    fn snapshot_of(result: (Outcome, Option<String>)) -> (crate::status::Snapshot, Option<String>) {
        match result {
            (Outcome::Snapshot(s), notice) => (s, notice),
            other => panic!("expected a snapshot, got {other:?}"),
        }
    }

    #[test]
    fn stage_commit_and_notice() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        let (s, _) = snapshot_of(run(&t.repo, &Op::Stage("a.txt".into())).unwrap());
        assert_eq!(s.entries[0].section, Section::Staged);
        let (s, notice) = snapshot_of(run(&t.repo, &Op::Commit("first".into())).unwrap());
        assert!(s.entries.is_empty());
        assert!(notice.unwrap().starts_with("committed "));
    }

    #[test]
    fn switch_create_and_errors_pass_through() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        t.commit_all("first");
        let (s, notice) = snapshot_of(run(&t.repo, &Op::Switch { name: "topic".into(), create: true }).unwrap());
        assert_eq!(s.branch.head.as_deref(), Some("topic"));
        assert_eq!(notice.as_deref(), Some("created topic"));
        assert!(run(&t.repo, &Op::Switch { name: "missing".into(), create: false }).is_err());
    }

    #[test]
    fn stash_drop_stays_in_the_list_and_pop_returns_a_snapshot() {
        let t = TestRepo::new();
        t.write("a.txt", "1");
        t.commit_all("first");
        for n in ["2", "3"] {
            t.write("a.txt", n);
            run(&t.repo, &Op::StashPush(String::new())).unwrap();
        }
        let (outcome, notice) = run(&t.repo, &Op::Stash(StashVerb::Drop, 1)).unwrap();
        assert!(matches!(outcome, Outcome::Stashes(ref list) if list.len() == 1), "{outcome:?}");
        assert_eq!(notice.as_deref(), Some("dropped stash@{1}"));
        let (s, notice) = snapshot_of(run(&t.repo, &Op::Stash(StashVerb::Pop, 0)).unwrap());
        assert_eq!(s.stashes, 0);
        assert_eq!(s.entries.len(), 1);
        assert_eq!(notice.as_deref(), Some("popped stash@{0}"));
    }

    #[test]
    fn diff_title_shows_renames() {
        let t = TestRepo::new();
        t.write("old.txt", "content\n");
        t.commit_all("first");
        std::fs::rename(t.dir.path().join("old.txt"), t.dir.path().join("new.txt")).unwrap();
        t.repo.stage_all().unwrap();
        let s = t.repo.snapshot().unwrap();
        let (outcome, _) = run(&t.repo, &Op::Diff(s.entries[0].clone())).unwrap();
        assert!(matches!(outcome, Outcome::Diff { ref title, .. } if title == "old.txt → new.txt"), "{outcome:?}");
    }
}
