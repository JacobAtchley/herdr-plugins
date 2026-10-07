# herdr-plugins

Custom [herdr](https://herdr.dev) plugins, built and linked from one repo.

## Dev loop

```sh
just          # build every plugin and link it with herdr
just test     # run all tests
just dev-one command-palette   # build + link one plugin
just ci       # fmt + clippy + tests, same as the PR gate
```

Pull requests into `main` must pass CI (`.github/workflows/ci.yml`: fmt,
clippy with warnings denied, and tests on Linux and macOS) before merging.

herdr launches plugin commands fresh each time, so after `just` the next
invocation uses the new build. Relinking is safe; it refreshes the manifest.

## Layout

- `crates/herdr-client` — typed client for herdr's socket API, shared by plugins.
- `plugins/<name>/herdr-plugin.toml` — one directory per plugin.
- `scripts/plugins.py` — runs each manifest's `[[build]]` commands and links it.

## Adding a plugin

1. Create `plugins/<name>/herdr-plugin.toml` (see the herdr plugin docs).
2. For a Rust plugin, add `plugins/<name>` to `members` in the root `Cargo.toml`
   and use `cargo build --release --target-dir target` as its build command.
3. Run `just`.

## Command palette (`plugins/command-palette`)

Editor-style fuzzy palette: workspaces, tabs, agents, built-in herdr commands,
other plugins' actions, and your own commands. Frequently and recently used
items rank first.

### Keybinding

`~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "cmd+k"
type = "plugin_action"
command = "jacob.command-palette.open"
description = "command palette"
```

Ghostty binds `cmd+k` itself, so free it in `~/.config/ghostty/config`:

```
keybind = cmd+k=unbind
```

### Keys

`↑`/`↓` or `ctrl-p`/`ctrl-n` move · `enter` runs · `esc` closes (or leaves a
prompt) · `ctrl-u` clears · fzf syntax works: `^start`, `end$`, `'exact`, `!not`.

### Your own commands

`~/.config/herdr/plugins/config/jacob.command-palette/commands.toml`
(`herdr plugin config-dir jacob.command-palette` prints the directory):

```toml
[[commands]]
title = "Deploy staging"
run = "just deploy staging"   # via sh -c
keywords = ["ship"]           # optional extra match words
cwd = "~/project"             # optional; default is the focused pane's cwd

[[commands]]
title = "Open repo in browser"
argv = ["gh", "browse"]       # alternative to `run`; exactly one is required
```

Commands run detached, so use them to launch things rather than for
interactive programs.

### Troubleshooting

- `~/.local/state/herdr/plugins/jacob.command-palette/palette.log` has source
  failures and action errors.
- `herdr plugin log list --plugin jacob.command-palette` shows launch failures.

## Git glance (`plugins/git-glance`)

Popup git status for the focused pane's repository: branch, upstream
ahead/behind, stash count, and staged/unstaged/untracked/conflicted files.
Stage, diff, commit, switch or create branches, and stash without leaving the
pane you're in.

### Keybinding

```toml
[[keys.command]]
key = "cmd+g"
type = "plugin_action"
command = "jacob.git-glance.open"
description = "git glance"
```

### Keys

| View | Keys |
|---|---|
| Status | `j`/`k` or `↑`/`↓` move · `space` stage/unstage · `a` stage all · `u` unstage all · `enter`/`d` diff · `c` commit · `b` branches · `z` stashes · `S` stash changes · `r` refresh · `q`/`esc` close |
| Diff | `j`/`k` scroll · `space`/`b` page · `g`/`G` top/bottom · `esc` back |
| Branches | type to filter · `enter` switches, or creates the typed name when nothing matches · `esc` back |
| Stashes | `enter`/`p` pop · `a` apply · `x` drop (asks first) · `esc` back |

`S` stashes untracked files too. Commits use `git commit -m`, so hooks run but
no editor opens. A failed commit keeps your message in the prompt.

### Notes

- The repo comes from the focused pane's cwd, then the workspace cwd.
- Read-only git calls run with `GIT_OPTIONAL_LOCKS=0`, so opening the glance
  never takes the index lock from an agent or editor working in the same repo.
- Git errors show in the footer. The full text goes to
  `~/.local/state/herdr/plugins/jacob.git-glance/glance.log`.

## Project runner (`plugins/project-runner`)

Popup picker for the `package.json` scripts of the focused pane's project.
Pick one and it runs in a new tab (or split) as `<pm> run <script>`, so output
stays visible and `ctrl-c` stops it.

- Walks up from the focused pane's cwd to the repo root; the nearest
  `package.json` **with scripts** wins, so a pane deep in an nx lib still finds
  the root's helper scripts.
- Package manager: `packageManager` field, else lockfile (pnpm, yarn, bun,
  npm), else npm.
- Scripts keep their `package.json` order; typing fuzzy-filters names and
  commands (`serve api` finds `"start:api": "nx serve api"`).

### Keybinding

```toml
[[keys.command]]
key = "cmd+r"
type = "plugin_action"
command = "jacob.project-runner.open"
description = "run project script"
```

### Keys

`↑`/`↓` or `ctrl-p`/`ctrl-n` move · `enter` runs in a new tab · `ctrl-v` split
right · `ctrl-x` split down · `esc` closes · `ctrl-u` clears.

## Agent inbox (`plugins/agent-inbox`)

Popup queue of agents that need you. Blocked agents come first, then done
ones; within each group the agent that changed state longest ago is on top.
Working agents sit underneath so you can see they are busy. The list refreshes
every second while the popup is open.

Each agent shows one line of context: a blocked agent's question, or the
first line of a done agent's last message. When neither can be found in the
pane, the agent's session title shows instead.

### Keybinding

```toml
[[keys.command]]
key = "cmd+i"
type = "plugin_action"
command = "jacob.agent-inbox.open"
description = "agent inbox"
```

### Keys

`↑`/`↓`, `j`/`k` or `ctrl-p`/`ctrl-n` move · `enter` jumps to the agent and
closes · `esc`/`q` close.

| Selected agent | Keys |
|---|---|
| Blocked | `y` presses Enter (the dialog's highlighted "yes") · `n` presses Escape |
| Done | `c` sends "continue" · `r` opens a reply prompt; `enter` sends it, `esc` cancels |

After a quick reply the agent leaves the list until herdr reports a new state
for it, and the cursor moves to the next one.

### Notes

- herdr does not report when a state started, so rows have no wait time; the
  order comes from herdr's state change counter.
- Errors show in the footer and go to
  `~/.local/state/herdr/plugins/jacob.agent-inbox/inbox.log`.

## Zen mode (`plugins/zen-mode`)

Switcher shortlist for focused work: pick up to **two workspaces** and **three
tabs** in each, then toggle zen on to focus into that set. Deactivating keeps
the selection. Unselected tabs stay open and visible in Herdr (this is a
switcher, not host chrome filtering). Missing saved workspaces/tabs toast via
`notification.show` and are skipped on activate.

State: `~/.local/state/herdr/plugins/jacob.zen-mode/zen.toml`.

### Keybinding

One binding opens the switcher. Activate or deactivate zen from inside it with
`z` (no second shortcut required). `jacob.zen-mode.toggle` remains available
from the command palette if you want it without a keybind.

```toml
[[keys.command]]
key = "cmd+shift+z"
type = "plugin_action"
command = "jacob.zen-mode.configure"
description = "zen mode"
```

### Configure keys

`↑`/`↓` or `j`/`k` move · `enter` focuses the selected tab (or workspace) and
closes · `z` toggles zen on/off (on focuses a shortlist tab and closes; off
stays in the popup) · `r` replace · `a` add workspace · `t` add tab · `d`
remove · `esc` close. In pickers, type to filter · `enter` selects · `esc`
backs out. Tabs and workspaces show agent status (`● working`, `blocked`,
`done`, `idle`) when Herdr reports one.

### Notes / limits

- Zen does **not** hide unselected workspaces or tabs in Herdr’s sidebar or tab
  bar. It only remembers a shortlist, toggles focus into it, and lets you jump
  to a picked tab from configure.
- Collapsing the left sidebar is a Herdr UI action, not something this plugin
  can do on toggle. There is no sidebar collapse API for plugins today.
  - Toggle manually: default `prefix+b` (`toggle_sidebar` in
    `~/.config/herdr/config.toml`).
  - Or start collapsed on next launch:
    ```toml
    sidebar_start_collapsed = true
    sidebar_collapsed_mode = "hidden"   # or "compact" for a thin rail
    ```
