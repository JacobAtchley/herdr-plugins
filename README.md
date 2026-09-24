# herdr-plugins

Custom [herdr](https://herdr.dev) plugins, built and linked from one repo.

## Dev loop

```sh
just          # build every plugin and link it with herdr
just test     # run all tests
just dev-one command-palette   # build + link one plugin
```

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

(Replace this block with the variant that worked in Task 12 Step 6 if it was a fallback.)

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
