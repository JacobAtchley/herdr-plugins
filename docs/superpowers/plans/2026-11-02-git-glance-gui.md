# Git Glance GUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run git-glance in the Herdr desktop app (herdr-app) as well as the TUI, with all git logic still in the Rust binary.

**Architecture:** The `git-glance` binary gains headless subcommands (`git-glance <cmd> --cwd <dir> …`) that run the existing `ops::run` and print one JSON object. Two herdr-app mods live in the repo under `plugins/git-glance/gui/`: `git-glance` (the UI, shown as a popup overlay or as a mod pane) and `git-glance-pane` (a 1px launcher that toggles the mod pane in the current tab, modelled on the shipped `pane-next` mod). `scripts/plugins.py link` symlinks each `gui/<mod>` folder into `~/.config/herdr-app/mods/`.

**Tech Stack:** Rust 2024 workspace (serde, serde_json 1, tempfile 3 for tests), ratatui TUI unchanged; herdr-app mods in gpui-kit JavaScript (`~/.config/herdr-app/gpui-kit.d.ts`, `herdr.d.ts` are the API truth; `~/.config/herdr-app/AGENTS.md` is the manual; `~/.config/herdr-app/mods/changes/main.js` and `mods/pane-next/main.js` are reference mods); Python 3 `scripts/plugins.py`; just.

**Spec:** `docs/superpowers/specs/2026-11-02-git-glance-gui-design.md`

## Global Constraints

- Branch `feat/git-glance-gui`. Commit after every task; end each message with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_017SHDkxeudXCfQdJrUungWE`.
- `git-glance ui` and the TUI's behaviour must not change. `just ci` (fmt, clippy `-D warnings`, tests) passes after every Rust task.
- No git logic in JavaScript: the mods only call the binary and render its JSON. The CLI recomputes `unborn` and diff entries itself.
- One JSON object on stdout per CLI call. Success: `{"ok": true, "repo": <name>, "root": <absolute repo root>, "notice": <string|null>, <outcome-key>: …}`; failure: `{"ok": false, "error": <string>}` and exit code 1. Usage errors also use the failure shape, exit code 2. snake_case fields.
- Local repos only: if the focused space has a non-null `machine`, the mod shows "git glance works on local repos only".
- Keys: `cmd-g` → `mod.open:git-glance` (popup); `cmd-shift-g` → `mod.open:git-glance-pane` (toggle pane). Documented in README; not written into the user's `keys.json` by tooling.
- No redraw timers in mods. Refresh on `watchFolder(repo root)` and on focused-folder change.
- `herdr-app check <mod folder>` must pass for both mods; it is a local step (`just gui-check`), not in CI.

## Review Focus

1. **Paths with spaces, unicode, and renames** (`my file.txt`, `naïve.rs`, `old.rs → new.rs`): stage, unstage and diff target the right file; the diff title shows `orig → path`. Pinned in Tasks 1–2.
2. **No repo / no folder** (a cwd outside git, or the focused pane is a mod with no cwd): CLI returns `ok:false` with git's message; mod shows it as an empty state instead of crashing. Pinned in Task 1 (CLI) and Task 4 (mod).
3. **Unborn repo** (no commits yet): unstage, unstage-all and status work; snapshot reports `unborn: true`. Pinned in Task 1.
4. **Rejected actions surface as errors, not silence** (commit with nothing staged, empty commit message, switch to a branch blocked by local changes, stale stash index): `ok:false` + message, mod shows it in the footer and keeps state. Pinned in Tasks 1–2 and 5.
5. **Refresh feedback loop**: status reads must not modify `.git` (else `watchFolder` re-fires forever). The binary already sets `GIT_OPTIONAL_LOCKS=0`; Task 2 pins that `status` leaves `.git/index` mtime unchanged.

---

## File Structure

```
plugins/git-glance/
  Cargo.toml                    + serde, serde_json
  src/main.rs                   dispatch: "ui" → TUI, anything else → cli::main
  src/cli.rs                    NEW: argument parsing, Request → ops, JSON output
  src/status.rs                 + Serialize derives (Snapshot, BranchInfo, Section, Entry)
  src/git.rs                    + Serialize derives (Branch, Stash)
  tests/cli.rs                  NEW: end-to-end tests of the built binary
  gui/git-glance/               NEW mod: the UI
    mod.json
    main.js                     View: state, keys, render
    glance.js                   exec wrapper around the binary
    bin -> ../../target/release/git-glance   committed relative symlink
  gui/git-glance-pane/          NEW mod: pane toggle launcher
    mod.json
    main.js
scripts/plugins.py              link: also symlink gui/<mod> into herdr-app mods
justfile                        + gui-check recipe
README.md                       + Git Glance GUI section
```

---

### Task 1: JSON CLI — status, staging, commit

**Files:**
- Modify: `plugins/git-glance/Cargo.toml`, `src/status.rs`, `src/git.rs`, `src/main.rs`
- Create: `plugins/git-glance/src/cli.rs`, `plugins/git-glance/tests/cli.rs`

**Interfaces:**
- Produces CLI (all take `--cwd <dir>`, may appear anywhere after the subcommand):
  - `status`
  - `stage <path>` · `unstage <path>` · `stage-all` · `unstage-all`
  - `commit --message <text>`
- Produces in `src/cli.rs`:
  - `pub enum Request { Status, Stage(String), Unstage(String), StageAll, UnstageAll, Commit(String), /* Task 2 adds more */ }`
  - `pub fn parse(args: &[String]) -> Result<(PathBuf, Request), String>` (args exclude argv[0])
  - `pub fn execute(cwd: &Path, request: &Request) -> serde_json::Value` (never panics; returns the success or failure object)
  - `pub fn main(args: &[String]) -> ExitCode` (prints `execute`'s JSON; exit 0 / 1 / 2)
- Snapshot JSON (outcome key `snapshot`): `{ "branch": {head, oid, upstream, ahead, behind}, "stashes": n, "unborn": bool, "has_staged": bool, "entries": [{section, code, path, orig}] }` where `section` ∈ `"conflict"|"staged"|"unstaged"|"untracked"` and `code` is a one-char string.

- [ ] **Step 1: Add deps and derives.** `serde.workspace = true`, `serde_json.workspace = true` in `plugins/git-glance/Cargo.toml`. `#[derive(Serialize)]` on `Snapshot`, `BranchInfo`, `Entry`, `Branch`, `Stash`; `Section` gets `#[serde(rename_all = "lowercase")]`. `unborn`/`has_staged` are added in the CLI's output (e.g. a `#[derive(Serialize)] struct SnapshotJson<'a> { #[serde(flatten)] snapshot: &'a Snapshot, unborn: bool, has_staged: bool }`), not stored on `Snapshot`.

- [ ] **Step 2: Write failing unit tests in `src/cli.rs`** (use `crate::git::tests::TestRepo`):

```rust
#[test]
fn parse_reads_cwd_anywhere_and_rejects_unknown() {
    let a = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(parse(&a(&["stage", "--cwd", "/r", "my file.txt"])).unwrap(), (PathBuf::from("/r"), Request::Stage("my file.txt".into())));
    assert_eq!(parse(&a(&["--cwd", "/r", "commit", "--message", "fix: x"])).unwrap().1, Request::Commit("fix: x".into()));
    assert!(parse(&a(&["status"])).unwrap_err().contains("--cwd"));
    assert!(parse(&a(&["frobnicate", "--cwd", "/r"])).unwrap_err().contains("frobnicate"));
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
    assert_eq!(std::path::Path::new(v["root"].as_str().unwrap()).canonicalize().unwrap(), t.dir.path().canonicalize().unwrap());
}

#[test]
fn stage_then_unstage_in_an_unborn_repo() {
    let t = TestRepo::new();
    t.write("a.txt", "x");
    let v = execute(t.dir.path(), &Request::Stage("a.txt".into()));
    assert_eq!(v["snapshot"]["has_staged"], true);
    let v = execute(t.dir.path(), &Request::Unstage("a.txt".into()));
    assert_eq!(v["snapshot"]["has_staged"], false);
}

#[test]
fn commit_returns_notice_and_clean_snapshot() {
    let t = TestRepo::new();
    t.write("a.txt", "x");
    execute(t.dir.path(), &Request::StageAll);
    let v = execute(t.dir.path(), &Request::Commit("first".into()));
    assert!(v["notice"].as_str().unwrap().starts_with("committed "));
    assert_eq!(v["snapshot"]["entries"].as_array().unwrap().len(), 0);
}

#[test]
fn failures_are_ok_false_with_message() {
    let dir = tempfile::tempdir().unwrap();
    let v = execute(dir.path(), &Request::Status);
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("not a git repository"));
    let t = TestRepo::new();
    assert_eq!(execute(t.dir.path(), &Request::Commit("x".into()))["ok"], false); // nothing staged
    assert_eq!(execute(t.dir.path(), &Request::Commit("  ".into()))["error"], "commit message cannot be empty");
}
```

Also `stage_all_and_unstage_all_after_first_commit` (commit one file, modify two, `StageAll` → 2 staged entries, `UnstageAll` → 0 staged).

- [ ] **Step 3: Run** `cargo test -p git-glance cli::` — expect compile failure (no `cli` module).

- [ ] **Step 4: Implement `src/cli.rs`.** Remove `#[cfg(test)]` from `Repo::root` in `git.rs` (the CLI reports it as `root`). `execute` discovers the repo from `cwd`, takes a snapshot when it needs `unborn` (Unstage/UnstageAll), maps the request to `app::Op`, calls `ops::run`, and serialises the `Outcome` as `snapshot` / `diff` / `branches` / `stashes`. Empty or whitespace-only commit messages are rejected before git with the exact text `commit message cannot be empty` (same as the TUI). In `main.rs`, `Some("ui")` keeps running the TUI; any other first argument calls `cli::main(&args[1..])`; no arguments prints usage (`usage: git-glance ui | git-glance <command> --cwd <dir>`) to stderr, exit 2.

- [ ] **Step 5: Add one end-to-end test** in `tests/cli.rs` using `env!("CARGO_BIN_EXE_git-glance")`: run `status --cwd <tempdir with git init>`, assert exit 0 and stdout parses as JSON with `ok == true`; run `status --cwd <non-repo tempdir>`, assert exit 1 and `ok == false`; run with no `--cwd`, assert exit 2. (Integration tests can't see `TestRepo`; init with `git init -q` via `std::process::Command`.)

- [ ] **Step 6: Run** `just ci` — expect PASS.

- [ ] **Step 7: Commit** `feat(git-glance): JSON CLI for status, staging and commit`.

---

### Task 2: JSON CLI — diff, branches, stashes

**Files:**
- Modify: `plugins/git-glance/src/cli.rs`, `plugins/git-glance/tests/cli.rs`

**Interfaces:**
- Consumes: `Request`, `parse`, `execute` from Task 1.
- Produces CLI:
  - `diff <path> --section <conflict|staged|unstaged|untracked>` → outcome `diff: {title, text}`
  - `branches` → `branches: [{name, current, age, track}]`
  - `switch <name> [--create]` → `snapshot`, notice `switched to <name>` / `created <name>`
  - `stashes` → `stashes: [{index, age, subject}]`
  - `stash-push [--message <text>]` → `snapshot`, notice `stashed changes`
  - `stash <pop|apply|drop> <index>` → pop/apply: `snapshot`; drop: `stashes`; notice e.g. `popped stash@{0}`
- `Request` gains `Diff { path: String, section: Section }`, `Branches`, `Switch { name: String, create: bool }`, `Stashes`, `StashPush(String)`, `Stash(StashVerb, usize)`.

- [ ] **Step 1: Write failing tests:**
  - `diff_finds_the_entry_by_path_and_section`: commit `a.txt`, rename to `b c.txt` with `git mv`, stage; `Diff{path:"b c.txt", section:Staged}` → `ok`, `diff.title == "a.txt → b c.txt"`.
  - `diff_of_missing_entry_is_an_error`: `Diff{path:"nope", section:Unstaged}` → `ok:false`, error contains `nope`.
  - `switch_create_then_list_branches`: after first commit, `Switch{name:"feat", create:true}` → notice `created feat`; `Branches` → contains `{"name":"feat","current":true}`.
  - `switch_blocked_by_local_changes_is_an_error`: two branches with conflicting edits to the same file, dirty worktree → `ok:false`.
  - `stash_push_pop_round_trip`: dirty file, `StashPush("wip")` → notice `stashed changes`, entries empty; `Stashes` → one with subject containing `wip`; `Stash(Pop,0)` → file back in entries.
  - `stash_drop_returns_the_list_and_bad_index_errors`: `Stash(Drop,0)` → outcome key `stashes`; `Stash(Drop,5)` → `ok:false`.
  - `status_does_not_touch_the_index`: record `.git/index` mtime, sleep 1.1 s, `execute(Status)`, mtime unchanged.
  - Parse cases: `diff x --section bogus` errors; `stash pop notanumber` errors; `stash-push` without `--message` → `StashPush("")`.

- [ ] **Step 2: Run** `cargo test -p git-glance cli::` — expect FAIL.

- [ ] **Step 3: Implement.** `Diff` takes a fresh snapshot and finds the entry with equal `path` and `section` (so `orig` comes from git, not the caller); missing → `no <section> change for <path>`. Everything else maps 1:1 onto existing `Op`s.

- [ ] **Step 4: Run** `just ci` — expect PASS.

- [ ] **Step 5: Commit** `feat(git-glance): JSON CLI for diff, branches and stashes`.

---

### Task 3: Mod scaffolding, linking, and a read-only status view

**Files:**
- Create: `plugins/git-glance/gui/git-glance/{mod.json,main.js,glance.js}`, symlink `gui/git-glance/bin`
- Modify: `scripts/plugins.py`, `justfile`

**Interfaces:**
- Produces `glance.js`:
  - `export async function glance(cwd: string, args: string[]): Promise<object>` — runs `exec(BIN, [...args, "--cwd", cwd])` where `BIN = appHome() + "/mods/git-glance/bin"`; parses stdout JSON; if `exec` rejects or stdout isn't JSON, resolves `{ ok: false, error: <stderr or reason> }`. Never throws.
- Produces `mod.json`: `{ "place": "none", "title": "Git Glance", "overlay": { "kind": "dialog", "width": 0.7, "height": 0.6 } }`.
- Produces `plugins.py link` behaviour: for each plugin with a `gui/` dir, for each child folder `<mod>`, ensure `$HERDR_APP_HOME or ~/.config/herdr-app` `/mods/<mod>` is a symlink to it. Skip (print a note) if the app folder doesn't exist. Replace an existing symlink; refuse (print error, continue) if a real folder is there.
- Produces `just gui-check`: runs `herdr-app check` on every `plugins/*/gui/*/` folder.

- [ ] **Step 1: Spike symlinked mods.** Create `gui/git-glance/` with a minimal `main.js` (a View rendering `div().child("git glance")`) and `mod.json`; `ln -s ../../target/release/git-glance plugins/git-glance/gui/git-glance/bin`; manually `ln -s $PWD/plugins/git-glance/gui/git-glance ~/.config/herdr-app/mods/git-glance`; run `herdr-app check ~/.config/herdr-app/mods/git-glance` and `herdr-app do mod.open --name git-glance`, then `herdr-app status` and `herdr-app shot`. Expected: check passes, the dialog shows. **If the app ignores symlinked mod folders**, switch `plugins.py` to copy the folder instead and make `bin` an absolute symlink written at link time (and gitignore it); note the change in the commit message.

- [ ] **Step 2: Implement `link` in `plugins.py`** as specified above; add `gui-check` to the justfile. Run `just dev-one git-glance`; expect `[jacob.git-glance] linked …` plus one `linked mod git-glance` line.

- [ ] **Step 3: Implement `glance.js` and a read-only status render** in `main.js`:
  - Location (copy the approach of `mods/changes/main.js` `location()`): focused pane's `cwd` → `focused().cwd` → active space `folder`. Popup: read once in `init`. Pane: `props().cwd` if given, then follow focus changes in `render`, keeping the last non-null terminal folder when focus is on a mod pane.
  - If the active space has a `machine`, render the local-only message and don't call the binary.
  - Render header (repo, branch, ↑ahead ↓behind, stash count), then sections in order Conflicts / Staged / Changes / Untracked with `code` + path (`orig → path` for renames). Section titles match the TUI's (`status.rs` `Section::title`).
  - `ok:false` renders the error text centered as an empty state.
  - Refresh loop: `cx.spawn` loop on `watchFolder(root)` using the response's `root`; restart the loop when `root` changes.
  - Use `theme()` colours only.

- [ ] **Step 4: Verify.** `just gui-check` passes. `herdr-app do mod.open --name git-glance` with a terminal focused in this repo; `herdr-app shot $SCRATCH/glance.png` shows the status list. Touch a file; the list updates without a key press.

- [ ] **Step 5: Commit** `feat(git-glance): herdr-app mod with live status view`.

---

### Task 4: Status interactions — select, stage, commit, stash push

**Files:**
- Modify: `plugins/git-glance/gui/git-glance/main.js`

**Interfaces:**
- Consumes: `glance(cwd, args)` (Task 3), CLI from Tasks 1–2.
- Key vocabulary (focus handle on the root; `track_focus` + `on_key_down`), mirroring `src/app.rs` `status_key`:
  `j/k/↑/↓/ctrl-n/ctrl-p` move · `g/G/home/end` · `space` stage/unstage selected · `a` stage all · `u` unstage all · `enter`/`d` diff (Task 5) · `c` commit prompt · `S` stash prompt · `b` branches (Task 5) · `z` stashes (Task 5) · `r` refresh · `esc`/`q` close (popup: `closeOverlay(props().overlay)`; pane: `run("pane.close", {pane})` for its own pane).
- Mouse: click a row selects it; a per-row button (or double-click) toggles stage; footer buttons for Commit and Stash.

- [ ] **Step 1: Implement selection** that survives refreshes by `(section, path)`, falling back to the same index clamped.
- [ ] **Step 2: Implement stage/unstage/all** through `glance`; apply returned `snapshot`; show `notice` (info) or `error` (error colour) in a footer line, cleared on the next action.
- [ ] **Step 3: Implement the prompts** with `InputState` + `Input` (see `mods/changes/main.js`): commit prompt only opens when `has_staged`, else footer error `nothing staged to commit`; stash prompt only when entries exist, else `no local changes to stash`. Enter submits, Esc cancels and returns focus to the list. Messages same as the TUI (`src/app.rs`).
- [ ] **Step 4: Verify** `just gui-check`. In the app: stage one file of two with `space`, commit `test: gui commit` with `c`, confirm with `git log -1 --oneline` in a terminal; `S` with message `wip`, confirm `git stash list`. Error path: `c` with nothing staged shows the footer error. Screenshot with `herdr-app shot`.
- [ ] **Step 5: Commit** `feat(git-glance): stage, commit and stash from the GUI`.

---

### Task 5: Diff, branches, and stashes views

**Files:**
- Modify: `plugins/git-glance/gui/git-glance/main.js` (split views into `views.js` if `main.js` passes ~400 lines)

**Interfaces:**
- Consumes: CLI `diff`, `branches`, `switch`, `stashes`, `stash` from Task 2.
- Views mirror `src/app.rs`: `View::Diff`, `View::Branches` (type-to-filter, case-insensitive substring; Enter on a match switches, Enter with no match and a non-empty query creates; Enter on current branch returns to status), `View::Stashes` (`enter`/`p` pop, `a` apply, `x` drop after a confirm: `Drop stash@{N}? It cannot be recovered from here.`). `esc` returns to status from every view.

- [ ] **Step 1: Diff view.** Read-only `Editor` (`EditorState.new({ language: "diff", value, readonly: true, font_family: settings().font_family })`) or coloured rows like `mods/changes/main.js` `diffRows`; title is the CLI's `diff.title`. Scroll with `j/k`, `space`/`b` page, `g/G`.
- [ ] **Step 2: Branches view** with filter input, rows `name  age  track`, current marked.
- [ ] **Step 3: Stashes view** with confirm overlay for drop (`openOverlay({ mod: "confirm", … })` per the shipped `confirm` mod's props, or an inline confirm row — implementer's choice).
- [ ] **Step 4: Verify** `just gui-check`. In the app: open a diff of a renamed file (title `old → new`), create branch `gui-test` by typing it in the branch view, switch back to `feat/git-glance-gui`, pop/drop a stash. Delete `gui-test` afterwards.
- [ ] **Step 5: Commit** `feat(git-glance): diff, branch and stash views in the GUI`.

---

### Task 6: Pane toggle mod and docs

**Files:**
- Create: `plugins/git-glance/gui/git-glance-pane/{mod.json,main.js}`
- Modify: `README.md`

**Interfaces:**
- `mod.json`: `{ "place": "none", "title": "Git Glance pane", "overlay": { "kind": "popup", "width": 1, "height": 1, "frame": "none" } }` (same as `pane-next`).
- `main.js` `init`: close own overlay; `const tab = focused()?.tab`; if `panes().find(p => p.tab === tab && p.mod === "git-glance")` exists → `run("pane.close", { pane })`; else compute the folder as in Task 3 and `run("pane.split", { direction: "right", mod: "git-glance", props: { cwd } })`. `render` returns `div()`.

- [ ] **Step 1: Implement**, run `just dev-one git-glance` (links the second mod), `just gui-check`.
- [ ] **Step 2: Verify** with `herdr-app do mod.open --name git-glance-pane` twice: first opens a right pane showing this repo, second closes it. In the pane, focus a terminal in another repo; the pane switches repos. `esc` in the pane closes it.
- [ ] **Step 3: README** — add "Git Glance in the Herdr app" under the git-glance docs: what the two mods are, that `just` links them, the `keys.json` lines to add (`"cmd-g": "mod.open:git-glance"`, `"cmd-shift-g": "mod.open:git-glance-pane"` in `global`), `just gui-check`, and that it works on local repos only.
- [ ] **Step 4: Commit** `feat(git-glance): toggle git glance as a pane in the Herdr app`.

---

### Task 7: Whole-branch verification

- [ ] **Step 1:** `just ci` passes; `just gui-check` passes.
- [ ] **Step 2:** TUI regression: in herdr TUI, `cmd+g` opens the old popup and stage/commit works.
- [ ] **Step 3:** Walk the spec's "Done means" list in the app and record results (screenshots via `herdr-app shot`) for the final report.
