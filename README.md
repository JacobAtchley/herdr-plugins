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
