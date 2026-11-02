# Project Runner in the Herdr GUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The project-runner script picker works in the Herdr desktop app with TUI parity, its project discovery and ranking staying in Rust.

**Architecture:** `project-runner` gains a headless `scripts` subcommand that prints the discovered project and the ranked scripts (with highlight runs and the argv to launch) as JSON. A herdr-app mod in `plugins/project-runner/gui/project-runner/` calls it via `exec()` on open and on every query change, renders the list, and launches with the app's `tab.new` / `terminal.open`.

**Tech Stack:** Rust (serde_json, nucleo-matcher, tempfile tests), herdr-app gpui-kit JS mod, `node --test`.

**Spec:** `docs/superpowers/specs/2026-11-02-project-runner-gui-design.md`

## Global Constraints

- `project-runner ui` and the TUI behave exactly as before.
- JSON envelope: success `{"ok": true, ...}`, failure `{"ok": false, "error": "<message>"}`; exit 0 ok, 1 request failed, 2 bad arguments; snake_case fields; one object on stdout.
- No discovery, package-manager, quoting or ranking logic in JavaScript.
- Mod at `plugins/project-runner/gui/project-runner/`, `bin` symlink → `../../target/release/project-runner`, linked by `scripts/plugins.py` `link_app_mods` (no change needed there).
- `mod.json`: `place: "none"`, overlay popup 0.6 × 0.5.
- Key `cmd-r` → `mod.open:project-runner`.
- Remote space message: "project runner works on local folders only".
- Live app testing only in a throwaway space Claude creates, never in a space Jacob is using; close it afterwards.

## Review Focus

1. Script names with spaces, quotes or `:` (`"build:prod"`, `"it's"`) → argv carries the name as one element, no shell quoting needed; the TUI's quoted run line still used where a shell string is built. (Task 1 test `argv_keeps_odd_names_whole`.)
2. Non-ASCII script names (`"tëst"`, emoji) → highlights land on the right letters; the mod never indexes graphemes itself because Rust sends `segments`. (Task 1 test `segments_split_on_graphemes`.)
3. fzf-syntax queries (`!lint`, `'exact`, `^start`, `end$`) → same results as the TUI because it is the same `ScriptMatcher`. (Task 1 test `query_uses_tui_matcher_syntax`.)
4. Fast typing → an older response arriving after a newer one is dropped. (Task 3 test `older_results_are_dropped`.)
5. Focus on a mod pane (no terminal focused) → folder falls back to the space folder; with no folder at all the popup says "No folder: focus a terminal first". (Task 3, checked in the app in Task 4.)

---

### Task 1: `scripts` JSON subcommand

**Files:**
- Create: `plugins/project-runner/src/cli.rs`
- Modify: `plugins/project-runner/src/main.rs` (dispatch), `plugins/project-runner/src/project.rs` (`Manager::argv`, `Serialize` where handy)
- Test: unit tests in `cli.rs`; create `plugins/project-runner/tests/cli.rs`

**Interfaces:**
- Produces (CLI): `project-runner scripts --cwd <dir> [--query <q>]` →
  ```json
  {"ok": true,
   "project": {"dir": "/repo", "name": "mono", "manager": "pnpm"},
   "total": 3,
   "scripts": [{"index": 0, "name": "start:web", "command": "nx serve web",
                "segments": [{"text": "st", "hit": true}, {"text": "art:web", "hit": false}],
                "argv": ["pnpm", "run", "start:web"]}]}
  ```
  `scripts` is in ranked order (file order for an empty query); `total` is the number of scripts in package.json (so the mod can tell "no scripts" from "no matches"); `segments` are maximal runs of the name's graphemes by highlight. Failure (no package.json, malformed JSON): `{"ok": false, "error": <Project::discover's message>}`, exit 1. Bad args: exit 2.
- Produces (Rust): `pub fn cli::main(args: &[String]) -> ExitCode`, `pub fn cli::parse(args) -> Result<(PathBuf, Option<String>), String>`, `pub fn cli::scripts(cwd: &Path, query: &str) -> serde_json::Value`, `Manager::argv(self, script: &str) -> Vec<String>`.
- `main.rs`: `Some("ui")` → TUI as now; `Some(_)` → `cli::main(&args[1..])`; none → usage `project-runner ui | project-runner scripts --cwd <dir> [--query <q>]`, exit 2.

- [ ] **Step 1: Write the failing unit tests** in `cli.rs` (temp dirs with `package.json` written by the test):
  - `parse_needs_cwd_and_knows_scripts`: `["scripts","--cwd","/r"]` → `("/r", None)`; `["scripts","--query","x","--cwd","/r"]` → query `Some("x")`; missing `--cwd`, unknown command, `--query` without value → `Err`.
  - `lists_scripts_in_file_order`: pnpm lockfile + scripts `start:web`, `start:api`, `lint` → `project.manager == "pnpm"`, names in file order, `total == 3`, each `argv == ["pnpm","run",<name>]`.
  - `query_ranks_and_highlights`: query `"serve api"` → first script `start:api`; query `"lint"` → `segments == [{"text":"lint","hit":true}]`.
  - `query_uses_tui_matcher_syntax`: `"!lint"` excludes `lint`; result order equals `ScriptMatcher::rank` for the same query.
  - `no_matches_keeps_total`: query `"zzz"` → `scripts == []`, `total == 3`.
  - `segments_split_on_graphemes`: script `"tëst"`, query `"ts"` → segments `[t hit, ë miss, s hit, t miss]` merged as runs (`[{"t",true},{"ë",false},{"s",true},{"t",false}]`).
  - `argv_keeps_odd_names_whole`: script `"it's build"` → argv `["npm","run","it's build"]`.
  - `missing_package_json_is_an_error`: empty temp dir → `ok == false`, `error` starts with `"no package.json in"`; malformed `package.json` → `ok == false`.
- [ ] **Step 2: Run** `cargo test -p project-runner cli` — Expected: FAIL to compile (`cli` missing).
- [ ] **Step 3: Implement** `cli.rs` (reuse `Project::discover` and `ScriptMatcher::rank`; `segments` from `Ranked::highlights` over `name.graphemes(true)`), `Manager::argv`, and the `main.rs` dispatch. `mod cli;` in `main.rs`.
- [ ] **Step 4: Run** `cargo test -p project-runner` — Expected: all pass, existing TUI tests untouched.
- [ ] **Step 5: End-to-end test** `tests/cli.rs` `scripts_prints_json_and_exits_by_outcome` using `env!("CARGO_BIN_EXE_project-runner")`: dir with package.json → exit 0 `ok:true`; empty dir → exit 1 `ok:false`; no `--cwd` → exit 2. Prove it can fail: temporarily break the exit code mapping, see FAIL, restore.
- [ ] **Step 6: Run** `cargo test -p project-runner && cargo clippy -p project-runner --all-targets -- -D warnings` — Expected: pass, no warnings.
- [ ] **Step 7: Commit** `feat(project-runner): JSON CLI listing ranked scripts`.

### Task 2: Confirm how the app runs argv

**Files:** none unless the check fails (then `project.rs` / `cli.rs`).

**Interfaces:**
- Consumes: Task 1 `argv`.
- Produces: a ledger ruling naming the argv form both launch paths use.

- [ ] **Step 1:** In a throwaway space Claude creates (`herdr-app do space.new --name pr-probe --folder <tmp dir with package.json {"scripts":{"hi":"echo hi-from-script"}}>`), run `herdr-app do tab.new --name probe --folder <dir> --argv '["npm","run","hi"]'` and, from that tab's terminal focus, `herdr-app do terminal.open --argv '["npm","run","hi"]' --folder <dir> --place right`. Check with `herdr-app ls` / `herdr-app shot` that each terminal printed `hi-from-script` and is still a live shell afterwards. Close the space (`space.close {id, confirmed: true}`) and switch back to the space that was active before.
  Expected: both stay open as shells → plain argv stands; ledger `Ruling:`.
- [ ] **Step 2 (only if a path exits instead of leaving a shell):** add the failing test `shell_argv_wraps_run_line` (`SHELL=/bin/zsh`, script `build:prod` → `["/bin/zsh","-ic","pnpm run 'build:prod'; exec /bin/zsh -i"]`, fallback `/bin/sh` when `SHELL` unset), watch it fail, implement `Manager::shell_argv`, use it for the affected path's `argv` (add a second field if only splits need it), run `cargo test -p project-runner`, commit `fix(project-runner): keep a shell open after the script`.

### Task 3: The app mod

**Files:**
- Create: `plugins/project-runner/gui/project-runner/{mod.json, main.js, runner.js, picker.js, picker.test.mjs}`, symlink `bin` → `../../target/release/project-runner`
- Modify: none in Rust.

**Interfaces:**
- Consumes: Task 1 CLI; Task 2 ruling on argv.
- Produces:
  - `runner.js`: `export async function scripts(cwd: string, query: string): Promise<{ok, ...}>` (exec `bin` with `["scripts", "--cwd", cwd, "--query", query]`; never throws, same error handling as git-glance's `glance.js`).
  - `picker.js` (pure, no app imports): `export class Ordered { next(): number; accept(n: number): boolean }` (as git-glance's `lifecycle.js`); `export function move(selected: number, delta: number, len: number): number` (wraps; 0 when len is 0); `export function action(key: string): "tab" | "right" | "down" | "close" | "up" | "down-move" | "clear" | null` for `enter`, `ctrl-v`, `ctrl-x`, `escape`/`ctrl-c`, `up`/`ctrl-p`, `down`/`ctrl-n`, `ctrl-u`; `export function emptyMessage(total: number): string` → `"no scripts in package.json"` when 0 else `"no matches"`.
  - `main.js` default `View`: one `InputState` for the query (its `change` re-queries and resets selection to 0); list rows render `segments` with hits in the accent colour + bold and the command dimmed; footer hints `⏎ tab  ^v split right  ^x split down  esc close` left, `<name> · <manager>` or the error right. Launch: `tab` → `run("tab.new", {name, folder: project.dir, argv})`; `right`/`down` → if `!focused()?.pane` show "focus a pane to split", else `run("terminal.open", {argv, folder: project.dir, place, name})`; on success `closeOverlay(props().overlay)`; a throw → footer error, popup stays. Discovery error → message plus "press any key"; any key closes. Remote space → "project runner works on local folders only". Folder: same `terminalFolder()` → space folder fallback as git-glance; none → "No folder: focus a terminal first". Click selects; double-click (`click_count >= 2`) runs in a tab.

- [ ] **Step 1: Write** `picker.test.mjs` tests: `older_results_are_dropped` (Ordered), `move wraps at both ends and is 0 for an empty list`, `keys map like the TUI` (every key above, plus `"a"` → null), `empty message tells no scripts from no matches`.
- [ ] **Step 2: Run** `node --test plugins/project-runner/gui/project-runner/picker.test.mjs` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `picker.js`; run again — Expected: PASS.
- [ ] **Step 4: Write** `mod.json`, `runner.js`, `main.js`, and the `bin` symlink; `cargo build --release -p project-runner --target-dir target` from the repo root if needed.
- [ ] **Step 5: Run** `herdr-app check plugins/project-runner/gui/project-runner/` — Expected: `check passed`.
- [ ] **Step 6: Run** `just` (links the mod) then, in the probe space from Task 2's method, `herdr-app do mod.open --name project-runner`, `herdr-app shot` — Expected: list with project/manager in the footer. Close it.
- [ ] **Step 7: Commit** `feat(project-runner): script picker in the Herdr app`.

### Task 4: Docs, keys, CI, final check

**Files:**
- Modify: `README.md` (Project runner: "### In the Herdr app"), `~/.config/herdr-app/keys.json` (add `"cmd-r": "mod.open:project-runner"` to `global`, after checking it's still unbound).
- `justfile` / CI: none expected — `gui-test` globs `plugins/*/gui/*/*.test.mjs` and `gui-check` globs every mod; confirm both pick up the new mod.

- [ ] **Step 1:** README section: the mod, the `cmd-r` keys.json line, keys same as the TUI plus click/double-click, how it launches (app's shell stays open; no shell-history entry), local folders only.
- [ ] **Step 2: Run** `just ci > <workspace>/ci.log 2>&1; echo $?` and `just gui-check` — Expected: 0, both mods plus the new one `check passed`, node tests include `picker.test.mjs`.
- [ ] **Step 3:** In the probe space: open the popup, type a query (`herdr-app shot`), and confirm the folder fallback message when no terminal is focused if it can be arranged. Close the probe space.
- [ ] **Step 4: Commit** `docs(project-runner): using the runner in the Herdr app`; add the key to `keys.json` (outside the repo) and validate it with `python3 -m json.tool`.
