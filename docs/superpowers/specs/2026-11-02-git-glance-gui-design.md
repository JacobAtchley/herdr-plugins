# Git Glance in the Herdr GUI — design

Pilot for running this repo's plugins in both Herdr front ends: the TUI
(`herdr`, plugins via `herdr-plugin.toml`) and the new desktop app
(`herdr-app` on herdr-core, extended by JavaScript "mods" in
`~/.config/herdr-app/mods/`). The app's own manual is
`~/.config/herdr-app/AGENTS.md`; its type files `herdr.d.ts` and
`gpui-kit.d.ts` sit beside it and are the API truth.

## Intent

Jacob uses git-glance mid-coding to see pending changes, stage **specific
files**, commit with his own message, and stash. He wants the same tool in
the GUI without maintaining git logic twice.

Approach ("option C"): Rust stays the single source of git behaviour. The
`git-glance` binary gains headless subcommands that print JSON. The TUI keeps
its ratatui UI unchanged; a new GUI mod is a thin native UI that calls the
binary with `exec()` and renders the JSON. If the pilot works, the same
pattern is applied to the other plugins.

## Goals

- Full parity with the TUI in the GUI: status (branch, upstream ahead/behind,
  stash count, conflict/staged/changes/untracked sections), stage/unstage one
  file, stage all/unstage all, commit with a message, diff of one entry,
  branch list with filter + switch + create, stash push with optional
  message, stash list with pop/apply/drop.
- Both presentations in the GUI from one mod: a **popup** opened by a key
  (closes with Esc, like the TUI) and a **split pane** that can stay open.
- Repo is the focused terminal's folder, as in the TUI.
- Usable by keyboard and mouse in the GUI.
- TUI behaviour unchanged.

## Non-goals

- Remote machines (spaces on another machine via `on:`). Local only for now.
- New git features beyond what the TUI does.
- Porting the other plugins (later, after the pilot).
- Replacing or editing the GUI's shipped `changes` mod.

## Anti-goals

- Git logic (porcelain parsing, staging rules, unborn-branch handling, stash
  verbs) reimplemented in JavaScript. The mod only shells out and renders.
- The TUI regressing or its code being reshaped for the GUI's sake beyond
  extracting a shared layer.
- Polling timers in the split pane (the app's manual forbids redraw timers;
  use `watchFolder` plus focus changes).

## Decided

- **One binary, new subcommands.** `git-glance ui` stays as is. New
  subcommands share `git.rs`, `status.rs` and `ops.rs`. The existing
  `Op → Outcome` layer in `ops.rs` is the seam: each CLI subcommand builds an
  `Op`, runs `ops::run`, and prints the `Outcome` (plus notice) as JSON.
- **Repo selection is explicit.** Every subcommand takes `--cwd <dir>`; the
  binary discovers the repo from it. No herdr env vars are read on this path.
- **JSON contract.** One JSON object on stdout per call. Success:
  `{"ok": true, "notice": <string|null>, ...outcome}` where the outcome is one
  of `snapshot`, `diff`, `branches`, `stashes`. Failure: `{"ok": false,
  "error": "<message>"}` with a nonzero exit code. Field names in snake_case.
  The snapshot includes the repo name and whether it is unborn so the mod
  never has to infer git state.
- **Mod lives in the repo** at `plugins/git-glance/gui/` (`main.js`,
  `mod.json`, any helpers/icons) and is symlinked to
  `~/.config/herdr-app/mods/git-glance` by the repo's tooling (`just`).
- **Keys** (Claude's call): `cmd-g` opens the popup (same as the TUI's
  `cmd+g`; unused in the GUI's `keys.json`), `cmd-shift-g` toggles the split
  panel. Documented in the README, added to `keys.json` by the user (or by
  `just` only if it can do so without clobbering their file — builder's call).
- **Popup** (verified by the builder check): `cmd-g` → `mod.open:git-glance`,
  with an overlay size in `mod.json`. The mod tells popup from pane through
  `props().overlay`.
- **"Stays open" mode is a split pane** (Jacob's decision): `cmd-shift-g`
  splits git-glance in as a mod pane on the right of the current tab
  (`pane.split {direction: "right", mod: "git-glance"}`); pressing it again
  in that tab closes that pane. It belongs to the tab, not the whole app.
  `mod.json` uses `place: "none"`; no layout files are created or edited,
  and the user stays on Herdr's shipped `main` layout.
- **Refresh** (Claude's call): refresh when the focused terminal's folder
  changes and on `watchFolder(repo root)` events; reads must not touch the
  index (`--no-optional-locks`, like the shipped `changes` mod) so the
  watcher doesn't feed itself. When the git-glance pane itself has focus,
  keep the last terminal folder (as the `changes` mod does).
- **CLI recomputes git state.** Flags like `unborn` and whole-entry diff
  inputs are derived by the binary from `--cwd` and a path, never passed in
  by the mod.

## Left to the builder

- Exact subcommand names/flags and JSON field shapes within the contract
  above; a test per subcommand against the existing `TestRepo` helper.
- How the mod locates the binary (e.g. a symlink inside the mod folder, or a
  path written at link time). Must survive `cargo build` rebuilds.
- Visual layout and key handling within the mod, following the app's theme
  colours and the TUI's key vocabulary where it fits (j/k, space to toggle
  stage, `c` commit, `z` stashes, `b` branches, Enter for diff).
- How `just` builds/links the mod alongside `herdr plugin link`.
- Whether `herdr-app check` runs in CI (it needs the app installed, so likely
  a local-only step documented in the README).

## Done means

- `just ci` passes; new subcommands are covered by tests.
- `herdr-app check plugins/git-glance/gui` passes.
- In the GUI: open the popup, stage one file, commit with a message, stash
  and pop, switch branch, view a diff; the split pane updates when files
  change.
- The TUI popup still works as before.
