# Command Palette Plugin — Design

Date: 2026-09-24
Status: Approved design, pending implementation plan

## Goal

Turn this repository into a home for custom herdr plugins, and ship the first
one: an editor-style command palette. Pressing `cmd+k` inside herdr opens a
popup that lists workspaces, tabs, agents/panes, built-in herdr commands, other
plugins' actions, and user-defined commands. Typing fuzzy-filters the list;
Enter runs the selected item.

### Success criteria

- `cmd+k` opens the palette in under ~50 ms perceived latency.
- Every item kind is listed and runs correctly against a live herdr 0.9.1.
- Fuzzy matching feels like fzf/VS Code (subsequence matching, word-boundary
  bonuses, highlighted match positions).
- Frequently and recently used items rise to the top.
- One command (`just`) rebuilds and links every plugin in the repo after a
  change.
- Adding a second plugin to the repo requires no build tooling changes.

## Constraints

- herdr plugin v1: plugins are directories with a `herdr-plugin.toml`
  manifest. There is no SDK; the herdr CLI and socket API are the plugin API.
- Plugin UI is a terminal process. The palette runs as a `placement = "popup"`
  plugin pane, which is session-modal, receives all input including Escape, and
  closes when the process exits.
- The palette can only trigger what the herdr CLI/socket API exposes. Internal
  UI modes (settings, copy mode, navigate mode) are not reachable.
- The user's terminal is Ghostty, which binds `cmd+k` to `clear_screen` by
  default. That binding must be removed so the key reaches herdr.
- Toolchain: Rust (cargo 1.98), just 1.58. Target platforms: macOS and Linux.

## Decisions

| Decision | Choice | Reason |
|---|---|---|
| Language | Rust | Instant startup (the popup launches on every keypress), no runtime dependency, typed models catch herdr API drift at compile time. |
| TUI | ratatui + crossterm | Standard Rust TUI stack, testable via `TestBackend`. |
| Fuzzy matcher | nucleo | Helix's matcher: fzf-quality scoring, match positions for highlighting, fzf query syntax. |
| Layout | Unified list with type badges, frecency-ranked | Chosen over prefix modes and grouped sections. |
| Task runner | just | User preference over make. |

## Repository layout

```
herdr-plugins/
  Cargo.toml                    # cargo workspace: crates/*, plugins/*
  justfile
  README.md                     # adding a plugin, dev loop, keybinding setup
  crates/
    herdr-client/               # shared library for all plugins in this repo
      src/
        lib.rs
        socket.rs               # JSON request/response over HERDR_SOCKET_PATH
        models.rs               # Workspace, Tab, Pane, Agent, PluginAction, ...
        context.rs              # parse HERDR_PLUGIN_CONTEXT_JSON
  plugins/
    command-palette/
      herdr-plugin.toml
      Cargo.toml
      src/
        main.rs                 # `open` and `ui` subcommands
        item.rs
        sources/
          mod.rs
          workspaces.rs
          tabs.rs
          agents.rs
          builtins.rs
          plugins.rs
          user.rs
        matcher.rs
        frecency.rs
        exec.rs
        ui/
          mod.rs
          render.rs
          input.rs
```

### Build output location

Each plugin builds with `--target-dir target` so its binary lands inside the
plugin directory (`plugins/command-palette/target/release/command-palette`).
Manifest commands reference that relative path. herdr runs plugin commands with
the plugin directory as the working directory, so the path resolves for both
`herdr plugin link` and a GitHub install of
`owner/herdr-plugins/plugins/command-palette`. `target/` is gitignored.

## Manifest

```toml
id = "jacob.command-palette"
name = "Command Palette"
version = "0.1.0"
min_herdr_version = "0.9.0"
description = "Editor-style fuzzy command palette for workspaces, tabs, agents, and commands"
platforms = ["macos", "linux"]

[[build]]
command = ["cargo", "build", "--release", "--target-dir", "target"]

[[actions]]
id = "open"
title = "Open command palette"
contexts = ["workspace"]
command = ["sh", "-c", "exec \"$HERDR_BIN_PATH\" plugin pane open --plugin jacob.command-palette --entrypoint palette"]

[[panes]]
id = "palette"
title = "Command Palette"
placement = "popup"
width = "60%"
height = "50%"
command = ["target/release/command-palette", "ui"]
```

- The `open` action exists because keybindings target actions, not panes. It
  opens the `palette` popup through the herdr CLI. Verified on 0.9.1: the popup
  process still receives `HERDR_PLUGIN_CONTEXT_JSON` describing the tiled pane
  underneath.
- `ui` is the interactive palette.

## User keybinding setup

herdr `config.toml`:

```toml
[[keys.command]]
key = "cmd+k"
type = "plugin_action"
command = "jacob.command-palette.open"
description = "command palette"
```

Ghostty config:

```
keybind = cmd+k=unbind
```

Verifying that `cmd+k` reaches herdr after the unbind is an explicit
implementation step. If it does not, fall back to a Ghostty remap of `cmd+k` to
a distinct escape sequence (the same technique the user already uses for
`ctrl+tab`) and bind that key in herdr.

## justfile

```just
default: dev

# Build and link every plugin. Run this after any change.
dev: build link

# Run every plugin's own manifest [[build]] commands.
build:

# Link (or relink) every plugins/*/herdr-plugin.toml.
link:

# Run all tests in the workspace.
test:

# Build and link one plugin by directory name.
dev-one name:
```

- `build` and `link` are implemented by `scripts/plugins.py`, which reads each
  manifest with Python's `tomllib`. `build` runs the manifest's own `[[build]]`
  commands (filtered by platform), so local builds match GitHub installs and
  non-Rust plugins work without justfile changes.
- `link` always runs `herdr plugin link`. Verified on 0.9.1: relinking an
  already linked plugin succeeds and refreshes its manifest, so rerunning
  `just` never fails.
- Plugins are discovered by globbing `plugins/*/herdr-plugin.toml`; adding a
  plugin requires no justfile edits.
- A rebuild is picked up on the next palette open without relinking, because
  herdr launches the binary fresh each time.

## herdr-client crate

- `Client::from_env()` connects to `HERDR_SOCKET_PATH`.
- `Client::request(method, params) -> Result<serde_json::Value>` sends one
  newline-delimited JSON request with a generated id and reads the response.
- Typed helpers for the calls the palette needs: `workspace_list`, `tab_list`,
  `pane_list`, `agent_list`, `plugin_action_list`, `plugin_action_invoke`,
  `plugin_pane_open`, plus the mutating calls used by built-ins.
- `models.rs` holds serde structs matching herdr 0.9.1 response shapes,
  validated against fixtures captured from a live server.
- `context.rs` parses `HERDR_PLUGIN_CONTEXT_JSON` into a struct exposing the
  workspace, tab, pane, and pane cwd the user was in when they opened the
  palette.
- Errors use `thiserror`; each error names the method that failed.

## Palette internals

### Item model

```rust
struct Item {
    kind: Kind,          // Workspace | Tab | Agent | Command | Plugin | User
    id: String,          // stable frecency key, e.g. "tab:w5:t2", "cmd:split-right"
    title: String,       // matched text, e.g. "CT › Claude"
    subtitle: Option<String>,
    keywords: Vec<String>, // extra matched text, e.g. agent status
    status: Option<AgentStatus>,
    action: Action,
}

enum Action {
    FocusWorkspace(String),
    FocusTab(String),
    FocusPane(String),
    Builtin(Builtin),          // may require a prompt step
    InvokePluginAction(String),
    RunUser(UserCommand),
}
```

### Sources

Each source is a function `fn(&Client, &Context) -> Result<Vec<Item>>`. All
six run in parallel threads when the palette opens.

| Source | Data | Title format |
|---|---|---|
| workspaces | `workspace.list` | workspace label |
| tabs | `tab.list` | `Workspace › Tab` |
| agents | `agent.list` joined with pane/workspace data | `Agent (workspace)` with status dot |
| builtins | static list | command name |
| plugins | `plugin.action.list`, excluding `jacob.command-palette.open` | action title, with the plugin id as subtitle (the listing carries no plugin name) |
| user | `$HERDR_PLUGIN_CONFIG_DIR/commands.toml` | command title |

A failing source logs its error and contributes no items; the footer notes it
(e.g. `agents unavailable`). The palette still opens.

### Built-in commands

All built-ins target the context captured at open time (the tiled pane
underneath the popup, from `HERDR_PLUGIN_CONTEXT_JSON`).

| Group | Command | Step |
|---|---|---|
| Workspace | New workspace | prompt: label; cwd = context pane cwd |
| Workspace | Rename workspace | prompt: label, prefilled |
| Workspace | Close workspace | confirm |
| Tab | New tab | prompt: label (optional) |
| Tab | Rename tab | prompt: label, prefilled |
| Tab | Close tab | confirm |
| Pane | Split right | none |
| Pane | Split down | none |
| Pane | Toggle zoom | none |
| Pane | Rename pane | prompt: label, prefilled |
| Pane | Close pane | confirm |
| Pane | Move pane to new tab | none |
| Pane | Move pane to new workspace | none |
| Worktree | Create worktree | prompt: branch |
| Worktree | Open worktree | prompt: branch |
| Herdr | Reload config | none |

Each built-in maps to a socket request through a pure function
`fn request(builtin, context, input) -> Request`, so the mapping is unit-tested
without a socket. Reload config uses the `server.reload_config` socket method.

### User commands

`$HERDR_PLUGIN_CONFIG_DIR/commands.toml`:

```toml
[[commands]]
title = "Deploy staging"
run = "just deploy staging"      # run via sh -c
keywords = ["ship"]              # optional
cwd = "~/project"                # optional; default: context pane cwd

[[commands]]
title = "Open repo in browser"
argv = ["gh", "browse"]       # alternative to `run`; exactly one is required
```

User commands run detached (stdio to null, own process group) so they survive
the popup exiting.
A missing file means no user commands. A parse error is shown in the footer
with its line number; all other sources still load.

### Matching and ranking

- nucleo with smart case and fzf query syntax (`^`, `$`, `'`, `!`).
- Matched text: `title` plus `keywords`. Match positions within `title` are
  highlighted.
- Final score: `nucleo_score + frecency_boost`, where
  `frecency_boost = min(frecency, FRECENCY_CAP) * FRECENCY_WEIGHT`. The
  constants live in one place and are tuned during implementation so a strong
  text match always beats a frecent weak match.
- Empty query: items sorted by frecency descending, then by kind order
  (Workspace, Tab, Agent, Command, Plugin, User), then title. The currently
  focused workspace and tab are excluded from the empty-query list.

### Frecency

- Stored in `$HERDR_PLUGIN_STATE_DIR/frecency.json` as
  `{ id: { count, last_used_unix } }`.
- Score: `count * age_factor`, where age factor is 4 (under 1 hour), 2 (under
  1 day), 0.5 (under 1 week), and 0.25 otherwise.
- Updated after an item runs successfully. Written atomically (temp file plus
  rename). Entries unused for 90 days are pruned on write.
- A corrupt or unreadable file is treated as empty, and a warning is logged.

### UI

```
╭─ ❯ query█                          ─╮
│ ▸ WS   V9 Orchestrator      ● idle  │
│   TAB  CT › Claude                   │
│   CMD  Split pane right              │
│   AGT  Claude (herdr-plugins) ● work │
│   PLG  radar: Refresh                │
│   USR  Deploy staging                │
╰─ ↑↓ move  ⏎ run  esc close  38 items╯
```

- Uses the terminal's default colors plus ANSI accents, so it follows the
  user's herdr/terminal theme.
- Keys: typing edits the query; `↑`/`↓` and `ctrl-p`/`ctrl-n` move the
  selection; `enter` runs; `esc` closes (or leaves a prompt/confirm step);
  `ctrl-u` clears the query; `backspace` deletes.
- Prompt step: the input line becomes `Rename "Claude" › █`. `enter` submits,
  `esc` returns to the list with the previous query intact.
- Confirm step: `Close tab "Claude"? ⏎ confirm · esc cancel`.

### Execution

- After an action succeeds: update frecency, restore the terminal, exit. The
  popup closes when the process exits.
- Plugin actions are launched as a detached `sh -c "sleep 0.2; herdr plugin
  action invoke <id>"`. The popup is a session singleton, so an action that
  opens its own popup would fail with `ui_busy` if invoked while the palette is
  still open.
- If an action fails, the palette stays open and shows the error in the footer
  in red. The user can retry or press `esc`.
- If the socket is unreachable at startup, the palette shows the error and
  `esc` closes it.

## Testing

- **herdr-client:** tests against a fake Unix-socket server that returns
  fixture JSON captured from a live herdr 0.9.1.
- **Sources:** fixture JSON in, expected `Vec<Item>` out.
- **Built-ins:** `request(builtin, context, input)` asserted per command.
- **Matcher/ranking:** a query and an item set produce the expected order,
  including the frecency boost and highlight positions.
- **Frecency:** decay math, pruning, corrupt-file recovery.
- **User config:** `run` vs `argv`, both or neither set, invalid TOML.
- **UI:** ratatui `TestBackend` renders for empty, filtered, prompt, confirm,
  and error states, asserted line by line.
- **Manual:** `just`, then `cmd+k` in live herdr, exercising every item kind.

## Out of scope for v1

Mouse support, preview pane, multi-select, custom themes, Windows support,
publishing to the herdr marketplace, and prebuilt release binaries.
