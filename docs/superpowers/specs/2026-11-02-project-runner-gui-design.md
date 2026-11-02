# Project Runner in the Herdr GUI — design

Second plugin on the "option C" pattern proven by git-glance
(`docs/superpowers/specs/2026-11-02-git-glance-gui-design.md`): Rust keeps the
behaviour and prints JSON; the TUI keeps its ratatui UI; a herdr-app mod is a
thin native UI that calls the binary with `exec()`. The app's manual is
`~/.config/herdr-app/AGENTS.md`; `herdr.d.ts` and `gpui-kit.d.ts` beside it
are the API truth.

## Intent

Jacob uses project-runner to pick a `package.json` script for the project
they're in and run it in a new tab or split without typing the command. They
want the same picker in the app, with feature parity with the TUI.

## Goals

Parity with the TUI (`plugins/project-runner`, README "Project runner"):

- The project is found from the focused terminal's folder, by the same rules:
  walk up to the repo root (a dir holding `.git`) or filesystem root; the
  nearest `package.json` **with scripts** wins, else the nearest one at all.
- Package manager: `packageManager` field, else lockfile (pnpm, yarn, bun,
  npm) from the project dir upward, else npm.
- Scripts in `package.json` order. Typing fuzzy-filters names **and**
  commands with the TUI's matcher (nucleo, fzf syntax, smart case), best
  match first, ties in file order; matched letters in the name highlighted.
- Keys: `↑`/`↓` or `ctrl-p`/`ctrl-n` move (wrapping at the ends) · `enter`
  runs in a new tab named after the script · `ctrl-v` split right ·
  `ctrl-x` split down · `esc` (and `ctrl-c`) close · `ctrl-u` clears ·
  backspace deletes. Typing edits the query; the selection goes back to the
  top on every change.
- Footer: key hints on the left; on the right the project name and package
  manager, replaced by the error when a launch fails (the popup stays open on
  failure, closes on success).
- Empty states: "no scripts in package.json", "no matches". No
  `package.json` at all: the discovery error ("no package.json in <dir> or its
  parents") and the popup closes on any key, as in the TUI. A malformed
  `package.json` shows its parse error the same way.
- Additions for the app: clicking a script selects it; double-click (or a
  click on a selected row) runs it in a new tab. Buttons/hints for the two
  splits are the builder's call.
- TUI behaviour unchanged.

## Non-goals

- The "my commands" palette extension and porting agent-inbox / zen-mode
  (decided against for now).
- Remote spaces (another machine). Local only, like git-glance; show
  "project runner works on local folders only" there.
- New features beyond the TUI.

## Anti-goals

- Project discovery, package-manager detection, quoting or fuzzy ranking
  reimplemented in JavaScript. The mod renders what the binary returns.
- TUI code reshaped for the app beyond adding the CLI.

## Decided

- **One binary, new subcommands.** `project-runner ui` stays. New headless
  subcommands take `--cwd <dir>` and print one JSON object on stdout, with
  the same envelope as git-glance: success `{"ok": true, ...}`, failure
  `{"ok": false, "error": "<message>"}`; exit 0 ok, 1 when the request
  failed (e.g. no package.json), 2 on bad arguments. snake_case fields.
- **Ranking stays in Rust**, called per query change (Claude's call, for
  exact matcher parity). Shape, e.g. `project-runner scripts --cwd <dir>
  [--query <q>]` returning the project (`dir`, `name`, `manager`) and the
  ranked scripts (`index`, `name`, `command`, `highlights` as grapheme
  indices into the name). Exact command names are the builder's call. The
  mod must drop stale results (a slow older query can't replace a newer
  one) — reuse the `Ordered` idea from git-glance's `lifecycle.js`.
- **Launch is the app's commands, not herdr-client.** New tab:
  `run("tab.new", {name: <script>, folder: <project dir>, argv})`. Splits:
  `run("terminal.open", {argv, folder, place: "right" | "down", name})`
  (`pane.split` takes no `folder`; `herdr-app commands`). The TUI typed the
  line into a fresh shell; the app can't type into a terminal, so:
- **The script runs in the user's shell, which stays open after it ends**
  (Jacob accepted losing the shell-history entry). AGENTS.md says `tab.new`
  argv already runs "in your interactive shell (including fish), which stays
  open after the agent exits", so the argv is the plain run command, e.g.
  `["pnpm", "run", "build:prod"]`, built in Rust from the TUI's `Manager`
  (the mod never builds commands). Output stays visible, `ctrl-c` stops the
  script and leaves the shell. *Claude's call:* the builder verifies early,
  in a throwaway space of its own (not one Jacob is using), that
  `terminal.open` right/down wraps argv the same way; if it doesn't, the
  argv for splits (or both) becomes `[$SHELL, "-ic", "<run line>; exec
  $SHELL -i"]` built and tested in Rust, with the run line from
  `Manager::run_line`.
- **Launch failures** (*Claude's call*): `run()` returns null and most
  launch failures surface later as the app's own toast (a terminal exiting
  nonzero within two seconds shows its exit code and last lines). So the
  popup reports what it can see before or during `run()` and otherwise
  closes, as the TUI does on success: it checks for a focused pane before a
  split (`terminal.open` right/down fails while a layout terminal block is
  focused) and catches anything `run()` throws, showing it in the footer
  with the popup left open. No polling for the new terminal.
- **Mod lives in the repo** at `plugins/project-runner/gui/project-runner/`
  (`main.js`, `mod.json`, `bin` symlink to `../../target/release/
  project-runner`), linked into `~/.config/herdr-app/mods/` by the existing
  `link_app_mods` in `scripts/plugins.py`. Popup only: `mod.json`
  `place: "none"` with an overlay sized like the TUI's (60% × 50%).
- **Key:** `cmd-r` → `mod.open:project-runner` (free in the app's
  `keys.json`); README documents it, and Claude adds it to Jacob's
  `keys.json` at the end as for git-glance. The app closes popups on Esc
  itself; the mod handles `ctrl-c` to close as well.
- **Folder** comes from the focused terminal, then the space's folder,
  like git-glance's mod (and the TUI's focused pane, then workspace).
  `ctrl-v` is bound only in the app's `terminal` key section, so it should
  reach a focused popup; confirm in the app.
- **Tests:** Rust unit tests for the CLI (parse, JSON shape, argv/quoting,
  error exits) plus an end-to-end exit-code test like git-glance's
  `tests/cli.rs`. Any pure JS logic gets `node --test` coverage picked up by
  `just gui-test`. `just gui-check` passes for the new mod.

## Left to the builder

- Exact subcommand names and JSON field names within the envelope above.
- Popup layout and styling (follow git-glance's mod and the app theme).
- Whether discovery runs once on open with ranking per keystroke, or both
  per keystroke.
- If `ctrl-v` / `ctrl-x` don't reach the popup after all, the fallback
  (report it).
