# Command Palette Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn this repo into a home for herdr plugins, and ship an editor-style `cmd+k` command palette that fuzzy-filters workspaces, tabs, agents, built-in commands, other plugins' actions, and user-defined commands.

**Architecture:** A cargo workspace holds a shared `herdr-client` crate (typed JSON client for herdr's Unix socket) and the `command-palette` plugin binary. herdr opens the binary as a `placement = "popup"` plugin pane. On start it loads all item sources in parallel, ranks them with nucleo plus a frecency boost, renders with ratatui, and runs the chosen item's action through the socket before exiting (which closes the popup). A `just` recipe backed by `scripts/plugins.py` builds every plugin from its own manifest `[[build]]` commands and links it with herdr.

**Tech Stack:** Rust 2024 edition (cargo 1.98), ratatui 0.30 (with its bundled crossterm), nucleo-matcher 0.3, serde/serde_json, toml 1, thiserror 2, tempfile 3 (tests), just 1.58, Python 3.12 (`tomllib`), herdr 0.9.1.

**Spec:** `docs/superpowers/specs/2026-09-24-command-palette-design.md`

## Global Constraints

- Plugin id: `jacob.command-palette`. Action id: `open`. Pane entrypoint id: `palette`.
- `min_herdr_version = "0.9.0"`; `platforms = ["macos", "linux"]`.
- The palette binary builds to `plugins/command-palette/target/release/command-palette` via `cargo build --release --target-dir target` run inside the plugin directory.
- Every herdr call from Rust goes through the `herdr_client::Api` trait (newline-delimited JSON over `HERDR_SOCKET_PATH`), except plugin-action invocation, which shells out to `$HERDR_BIN_PATH` (see Task 9).
- JSON models ignore unknown fields; unknown `agent_status` strings map to `Unknown`.
- Frecency file: `$HERDR_PLUGIN_STATE_DIR/frecency.json`. Log file: `$HERDR_PLUGIN_STATE_DIR/palette.log`. User commands: `$HERDR_PLUGIN_CONFIG_DIR/commands.toml`.
- Frecency age factors: 4 (< 1 hour), 2 (< 1 day), 0.5 (< 1 week), 0.25 otherwise; prune entries unused for 90 days.
- Ranking: `nucleo_score + min(frecency, FRECENCY_CAP) * FRECENCY_WEIGHT` with `FRECENCY_CAP = 15.0`, `FRECENCY_WEIGHT = 1.0`.
- The palette must never write to stdout/stderr while the TUI is up; diagnostics go to `palette.log`.
- Commit after every task. End every commit message with `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- Work on branch `feat/command-palette`.

## Review Focus

1. **Unicode, emoji, and very long labels** (for example a tab named `🚀 deploy` repeated to 200 characters): rendering must not panic, and the row must be clipped to the popup width. Pinned in Task 11.
2. **Tiny popups** (a terminal resized to 1–3 rows or 10 columns): layout must not panic. Pinned in Task 11.
3. **Stale targets** (a tab closed by someone else between palette open and Enter): the API error shows in the footer, the palette stays open, and it stays in list mode. Pinned in Tasks 9 and 10.
4. **Queries made only of fzf operators** (`!`, `^`, `'`, `$`): no panic; ranking still returns a sane list. Pinned in Task 7.
5. **User command whose `cwd` does not exist**: spawn failure surfaces as an error string rather than a crash. Pinned in Task 9.

---

## File Structure

```
Cargo.toml                                  workspace root
justfile                                    dev / build / link / test / dev-one
scripts/plugins.py                          manifest-driven build + link for every plugin
README.md                                   repo usage, adding plugins, palette setup
.gitignore                                  + target/
crates/herdr-client/
  Cargo.toml
  src/lib.rs                                re-exports
  src/models.rs                             Workspace, Tab, Agent, PluginAction, AgentStatus
  src/context.rs                            PluginContext (HERDR_PLUGIN_CONTEXT_JSON)
  src/client.rs                             Api trait, Client, Error
  tests/fixtures/*.json                     trimmed real herdr 0.9.1 responses
  tests/models.rs
  tests/client.rs
plugins/command-palette/
  Cargo.toml
  herdr-plugin.toml
  src/main.rs                               `ui` entrypoint, event loop
  src/item.rs                               Item, Kind, Action
  src/frecency.rs                           Frecency store
  src/matcher.rs                            rank()
  src/app.rs                                pure key-driven state machine
  src/render.rs                             ratatui drawing
  src/exec.rs                               run an Action
  src/log.rs                                append to palette.log
  src/testing.rs                            FakeApi (cfg(test) only)
  src/sources/mod.rs                        load_all(), label helpers
  src/sources/builtins.rs                   Builtin enum, steps, request mapping, items
  src/sources/user.rs                       commands.toml parsing, UserCommand
  src/sources/workspaces.rs
  src/sources/tabs.rs
  src/sources/agents.rs
  src/sources/plugins.rs
```

The spec sketched `ui/{mod,render,input}.rs`. This plan uses `app.rs`, which holds input handling as a pure state machine, and `render.rs`, which draws. The split makes input logic testable without a terminal.

---

### Task 1: Repo scaffold, build tooling, and a linkable stub plugin

**Files:**
- Create: `Cargo.toml`, `justfile`, `scripts/plugins.py`, `README.md`
- Create: `crates/herdr-client/Cargo.toml`, `crates/herdr-client/src/lib.rs`
- Create: `plugins/command-palette/Cargo.toml`, `plugins/command-palette/herdr-plugin.toml`, `plugins/command-palette/src/main.rs`
- Modify: `.gitignore` (append)

**Interfaces:**
- Consumes: nothing.
- Produces: workspace dependency names used by every later task (`herdr-client`, `serde`, `serde_json`, `thiserror`, `toml`, `nucleo-matcher`, `ratatui`, `tempfile`); the recipes `just`, `just build`, `just link`, `just test`, `just dev-one <name>`.

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
# Rust plugins must be listed here. Plugins in other languages live under
# plugins/ too but are not cargo members.
members = ["crates/herdr-client", "plugins/command-palette"]

[workspace.package]
edition = "2024"

[workspace.dependencies]
herdr-client = { path = "crates/herdr-client" }
nucleo-matcher = "0.3"
ratatui = "0.30"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
thiserror = "2"
toml = "1"
```

- [ ] **Step 2: Create the herdr-client crate stub**

`crates/herdr-client/Cargo.toml`:

```toml
[package]
name = "herdr-client"
version = "0.1.0"
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

`crates/herdr-client/src/lib.rs`:

```rust
//! Typed client for the herdr socket API, shared by the plugins in this repo.
```

- [ ] **Step 3: Create the command-palette crate stub**

`plugins/command-palette/Cargo.toml`:

```toml
[package]
name = "command-palette"
version = "0.1.0"
edition.workspace = true

[dependencies]
herdr-client.workspace = true
nucleo-matcher.workspace = true
ratatui.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

`plugins/command-palette/src/main.rs`:

```rust
use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: command-palette ui");
    ExitCode::from(2)
}
```

- [ ] **Step 4: Create the plugin manifest**

`plugins/command-palette/herdr-plugin.toml`:

```toml
id = "jacob.command-palette"
name = "Command Palette"
version = "0.1.0"
min_herdr_version = "0.9.0"
description = "Editor-style fuzzy command palette for workspaces, tabs, agents, and commands"
platforms = ["macos", "linux"]

[[build]]
command = ["cargo", "build", "--release", "--target-dir", "target"]

# Keybindings target actions, not panes, so this action opens the popup.
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

- [ ] **Step 5: Create the build/link script**

`scripts/plugins.py`:

```python
#!/usr/bin/env python3
"""Build or link the herdr plugins under plugins/.

Each plugin is built by running the [[build]] commands from its own
herdr-plugin.toml, so local builds match what `herdr plugin install` runs and
plugins in any language work without changes here.

Usage: plugins.py build|link [plugin-dir-name ...]
"""

import os
import pathlib
import platform
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
PLATFORM = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[platform.system()]


def manifests(names):
    if names:
        paths = [ROOT / "plugins" / name / "herdr-plugin.toml" for name in names]
        missing = [str(p) for p in paths if not p.is_file()]
        if missing:
            sys.exit(f"no manifest: {', '.join(missing)}")
        return paths
    return sorted((ROOT / "plugins").glob("*/herdr-plugin.toml"))


def supported(entry, default_platforms):
    platforms = entry.get("platforms", default_platforms)
    return platforms is None or PLATFORM in platforms


def build(manifest):
    data = tomllib.loads(manifest.read_text())
    for step in data.get("build", []):
        if not supported(step, data.get("platforms")):
            continue
        print(f"[{data['id']}] {' '.join(step['command'])}", flush=True)
        subprocess.run(step["command"], cwd=manifest.parent, check=True)


def link(manifest):
    data = tomllib.loads(manifest.read_text())
    herdr = os.environ.get("HERDR_BIN_PATH", "herdr")
    subprocess.run(
        [herdr, "plugin", "link", str(manifest.parent)],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    print(f"[{data['id']}] linked {manifest.parent.relative_to(ROOT)}", flush=True)


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in ("build", "link"):
        sys.exit(__doc__)
    action = build if sys.argv[1] == "build" else link
    found = manifests(sys.argv[2:])
    if not found:
        sys.exit("no plugins found under plugins/")
    for manifest in found:
        try:
            action(manifest)
        except subprocess.CalledProcessError as err:
            sys.exit(f"{manifest.parent.name}: {' '.join(err.cmd)} exited with {err.returncode}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 6: Create the justfile**

`justfile`:

```just
# Build and link every plugin. Run this after any change.
dev: build link

# Run each plugin's manifest [[build]] commands (all plugins, or the named ones).
build *plugins:
    python3 scripts/plugins.py build {{plugins}}

# Link (or relink) plugins with herdr. Relinking is safe and refreshes the manifest.
link *plugins:
    python3 scripts/plugins.py link {{plugins}}

# Build and link a single plugin by its directory name under plugins/.
dev-one name: (build name) (link name)

# Run every test in the cargo workspace.
test:
    cargo test --workspace
```

- [ ] **Step 7: Ignore build output and write the README**

Append to `.gitignore`:

```
# Rust build output (workspace root and per-plugin target dirs)
target/
```

`README.md`:

````markdown
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
````

- [ ] **Step 8: Verify the build and link loop**

Run: `cargo test --workspace`
Expected: compiles, `test result: ok. 0 passed` for each crate.

Run: `just`
Expected output ends with:
```
[jacob.command-palette] cargo build --release --target-dir target
[jacob.command-palette] linked plugins/command-palette
```

Run: `ls plugins/command-palette/target/release/command-palette && herdr plugin action list --plugin jacob.command-palette`
Expected: the binary path prints, and the JSON lists an action with `"action_id":"open"`.

Run: `just` again.
Expected: same success output (relinking is idempotent).

Run: `git status --short`
Expected: no `target/` paths listed.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock justfile scripts/plugins.py README.md .gitignore crates plugins
git commit -m "feat: scaffold plugin repo with just build/link loop and palette stub

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: herdr-client models and plugin context

**Files:**
- Create: `crates/herdr-client/src/models.rs`, `crates/herdr-client/src/context.rs`
- Modify: `crates/herdr-client/src/lib.rs`
- Create: `crates/herdr-client/tests/fixtures/{workspace_list,tab_list,agent_list,plugin_action_list,context}.json`
- Test: `crates/herdr-client/tests/models.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `herdr_client::models::AgentStatus` — `Idle | Working | Blocked | Done | Unknown`, `Copy`, `Default = Unknown`, `fn as_str(self) -> &'static str`.
  - `herdr_client::models::Workspace { workspace_id: String, number: u32, label: String, focused: bool, agent_status: AgentStatus }`
  - `herdr_client::models::Tab { tab_id: String, workspace_id: String, number: u32, label: String, focused: bool, agent_status: AgentStatus }`
  - `herdr_client::models::Agent { pane_id: String, workspace_id: String, tab_id: String, agent: Option<String>, name: Option<String>, display_agent: Option<String>, agent_status: AgentStatus, focused: bool, cwd: Option<String> }`
  - `herdr_client::models::PluginAction { plugin_id: String, action_id: String, title: String, description: Option<String> }` with `fn qualified_id(&self) -> String`.
  - `herdr_client::PluginContext { workspace_id, workspace_label, workspace_cwd, tab_id, tab_label, focused_pane_id, focused_pane_cwd, focused_pane_agent: Option<String> }` with `fn from_json(&str) -> serde_json::Result<Self>`, `fn from_env_value(Option<&str>) -> Self`, `fn from_env() -> Self`.
  - All models derive `Debug, Clone, PartialEq, Deserialize`.

- [ ] **Step 1: Add fixtures captured from a live herdr 0.9.1 (trimmed)**

`crates/herdr-client/tests/fixtures/workspace_list.json`:

```json
{"id":"t","result":{"type":"workspace_list","workspaces":[
  {"workspace_id":"w1","number":1,"label":"CT","focused":false,"pane_count":8,"tab_count":8,"active_tab_id":"w1:t3","agent_status":"unknown"},
  {"workspace_id":"w4","number":3,"label":"jacob","focused":false,"pane_count":4,"tab_count":4,"active_tab_id":"w4:t1","agent_status":"idle"},
  {"workspace_id":"w5","number":4,"label":"V9 Orchestrator","focused":true,"pane_count":3,"tab_count":3,"active_tab_id":"w5:t2","agent_status":"idle"}
]}}
```

`crates/herdr-client/tests/fixtures/tab_list.json`:

```json
{"id":"t","result":{"type":"tab_list","tabs":[
  {"tab_id":"w1:t3","workspace_id":"w1","number":3,"label":"Claude","focused":false,"pane_count":1,"agent_status":"unknown"},
  {"tab_id":"w4:t1","workspace_id":"w4","number":1,"label":"Hermes","focused":false,"pane_count":1,"agent_status":"idle"},
  {"tab_id":"w5:t2","workspace_id":"w5","number":2,"label":"Claude","focused":true,"pane_count":1,"agent_status":"idle"}
]}}
```

`crates/herdr-client/tests/fixtures/agent_list.json`:

```json
{"id":"t","result":{"type":"agent_list","agents":[
  {"terminal_id":"term_1","agent":"hermes","terminal_title":"hermes","agent_status":"idle","workspace_id":"w4","tab_id":"w4:t1","pane_id":"w4:p1","focused":false,"state_change_seq":20,"cwd":"/Users/me/jacob","revision":6},
  {"terminal_id":"term_2","agent":"claude","display_agent":"Claude Code","name":"reviewer","terminal_title":"✳ Claude Code","agent_status":"working","agent_session":{"source":"herdr:claude","agent":"claude","kind":"id","value":"13b24a25"},"workspace_id":"w5","tab_id":"w5:t2","pane_id":"w5:p2","focused":true,"state_change_seq":3,"cwd":"/Users/me/v9","revision":2}
]}}
```

`crates/herdr-client/tests/fixtures/plugin_action_list.json`:

```json
{"id":"t","result":{"type":"plugin_action_list","actions":[
  {"plugin_id":"jacob.command-palette","action_id":"open","title":"Open command palette","command":["sh","-c","..."],"contexts":["workspace"],"platforms":["macos","linux"]},
  {"plugin_id":"hhdebb.herdr-radar","action_id":"refresh","title":"Refresh radar","description":"Rescan agents","command":["radar","refresh"],"contexts":["workspace"]}
]}}
```

`crates/herdr-client/tests/fixtures/context.json`:

```json
{"workspace_id":"w6","workspace_label":"herdr-plugins","workspace_cwd":"/Users/me/herdr-plugins","tab_id":"w6:t1","tab_label":"1","focused_pane_id":"w6:p1","focused_pane_cwd":"/Users/me/herdr-plugins","focused_pane_agent":"claude","focused_pane_status":"working","invocation_source":"api","correlation_id":"plugin-pane"}
```

- [ ] **Step 2: Write the failing tests**

`crates/herdr-client/tests/models.rs`:

```rust
use herdr_client::PluginContext;
use herdr_client::models::{Agent, AgentStatus, PluginAction, Tab, Workspace};
use serde_json::Value;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn result_field(name: &str, field: &str) -> Value {
    let value: Value = serde_json::from_str(&fixture(name)).unwrap();
    value["result"][field].clone()
}

#[test]
fn parses_workspaces() {
    let workspaces: Vec<Workspace> =
        serde_json::from_value(result_field("workspace_list.json", "workspaces")).unwrap();
    assert_eq!(workspaces.len(), 3);
    assert_eq!(workspaces[2].workspace_id, "w5");
    assert_eq!(workspaces[2].label, "V9 Orchestrator");
    assert_eq!(workspaces[2].number, 4);
    assert!(workspaces[2].focused);
    assert_eq!(workspaces[2].agent_status, AgentStatus::Idle);
}

#[test]
fn parses_tabs() {
    let tabs: Vec<Tab> = serde_json::from_value(result_field("tab_list.json", "tabs")).unwrap();
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs[1].tab_id, "w4:t1");
    assert_eq!(tabs[1].workspace_id, "w4");
    assert_eq!(tabs[1].label, "Hermes");
}

#[test]
fn parses_agents_with_optional_fields() {
    let agents: Vec<Agent> =
        serde_json::from_value(result_field("agent_list.json", "agents")).unwrap();
    assert_eq!(agents[0].agent.as_deref(), Some("hermes"));
    assert_eq!(agents[0].name, None);
    assert_eq!(agents[0].display_agent, None);
    assert_eq!(agents[1].name.as_deref(), Some("reviewer"));
    assert_eq!(agents[1].display_agent.as_deref(), Some("Claude Code"));
    assert_eq!(agents[1].agent_status, AgentStatus::Working);
    assert_eq!(agents[1].pane_id, "w5:p2");
}

#[test]
fn unknown_agent_status_maps_to_unknown() {
    let tab: Tab = serde_json::from_str(
        r#"{"tab_id":"w1:t1","workspace_id":"w1","number":1,"label":"x","agent_status":"sleeping"}"#,
    )
    .unwrap();
    assert_eq!(tab.agent_status, AgentStatus::Unknown);
    assert!(!tab.focused);
}

#[test]
fn missing_label_defaults_to_empty() {
    let ws: Workspace = serde_json::from_str(r#"{"workspace_id":"w9","number":9}"#).unwrap();
    assert_eq!(ws.label, "");
    assert_eq!(ws.agent_status, AgentStatus::Unknown);
}

#[test]
fn plugin_action_qualified_id() {
    let actions: Vec<PluginAction> =
        serde_json::from_value(result_field("plugin_action_list.json", "actions")).unwrap();
    assert_eq!(actions[1].qualified_id(), "hhdebb.herdr-radar.refresh");
    assert_eq!(actions[1].description.as_deref(), Some("Rescan agents"));
    assert_eq!(actions[0].description, None);
}

#[test]
fn agent_status_strings() {
    assert_eq!(AgentStatus::Idle.as_str(), "idle");
    assert_eq!(AgentStatus::Working.as_str(), "working");
    assert_eq!(AgentStatus::Blocked.as_str(), "blocked");
    assert_eq!(AgentStatus::Done.as_str(), "done");
    assert_eq!(AgentStatus::Unknown.as_str(), "unknown");
}

#[test]
fn parses_plugin_context() {
    let ctx = PluginContext::from_json(&fixture("context.json")).unwrap();
    assert_eq!(ctx.workspace_id.as_deref(), Some("w6"));
    assert_eq!(ctx.workspace_label.as_deref(), Some("herdr-plugins"));
    assert_eq!(ctx.tab_id.as_deref(), Some("w6:t1"));
    assert_eq!(ctx.tab_label.as_deref(), Some("1"));
    assert_eq!(ctx.focused_pane_id.as_deref(), Some("w6:p1"));
    assert_eq!(ctx.focused_pane_cwd.as_deref(), Some("/Users/me/herdr-plugins"));
}

#[test]
fn context_from_missing_or_invalid_env_is_default() {
    assert_eq!(PluginContext::from_env_value(None), PluginContext::default());
    assert_eq!(PluginContext::from_env_value(Some("not json")), PluginContext::default());
    let ctx = PluginContext::from_env_value(Some(r#"{"tab_id":"w1:t1"}"#));
    assert_eq!(ctx.tab_id.as_deref(), Some("w1:t1"));
    assert_eq!(ctx.workspace_id, None);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p herdr-client --test models`
Expected: FAIL to compile with `unresolved import herdr_client::PluginContext` / `could not find models`.

- [ ] **Step 4: Implement models**

`crates/herdr-client/src/models.rs`:

```rust
//! Response models for the herdr 0.9.1 socket API. Unknown fields are ignored
//! so newer herdr versions keep parsing.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    #[default]
    #[serde(other)]
    Unknown,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Idle => "idle",
            AgentStatus::Working => "working",
            AgentStatus::Blocked => "blocked",
            AgentStatus::Done => "done",
            AgentStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub number: u32,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub agent_status: AgentStatus,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Tab {
    pub tab_id: String,
    pub workspace_id: String,
    pub number: u32,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub agent_status: AgentStatus,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Agent {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent: Option<String>,
    pub name: Option<String>,
    pub display_agent: Option<String>,
    #[serde(default)]
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub focused: bool,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PluginAction {
    pub plugin_id: String,
    pub action_id: String,
    pub title: String,
    pub description: Option<String>,
}

impl PluginAction {
    /// The globally unique id herdr uses for `plugin.action.invoke`.
    pub fn qualified_id(&self) -> String {
        format!("{}.{}", self.plugin_id, self.action_id)
    }
}
```

- [ ] **Step 5: Implement the plugin context**

`crates/herdr-client/src/context.rs`:

```rust
use serde::Deserialize;

/// Where the user was when a plugin was invoked, from `HERDR_PLUGIN_CONTEXT_JSON`.
/// For popup panes this describes the tiled pane underneath the popup.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct PluginContext {
    pub workspace_id: Option<String>,
    pub workspace_label: Option<String>,
    pub workspace_cwd: Option<String>,
    pub tab_id: Option<String>,
    pub tab_label: Option<String>,
    pub focused_pane_id: Option<String>,
    pub focused_pane_cwd: Option<String>,
    pub focused_pane_agent: Option<String>,
}

impl PluginContext {
    pub fn from_json(raw: &str) -> serde_json::Result<Self> {
        serde_json::from_str(raw)
    }

    /// Missing or unparseable context yields an empty context rather than an
    /// error, so a plugin opened outside herdr still starts.
    pub fn from_env_value(raw: Option<&str>) -> Self {
        raw.and_then(|raw| Self::from_json(raw).ok()).unwrap_or_default()
    }

    pub fn from_env() -> Self {
        Self::from_env_value(std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok().as_deref())
    }
}
```

`crates/herdr-client/src/lib.rs`:

```rust
//! Typed client for the herdr socket API, shared by the plugins in this repo.

pub mod context;
pub mod models;

pub use context::PluginContext;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p herdr-client --test models`
Expected: PASS, 9 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/herdr-client
git commit -m "feat(herdr-client): add socket API models and plugin context

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: herdr-client socket client and `Api` trait

**Files:**
- Create: `crates/herdr-client/src/client.rs`
- Modify: `crates/herdr-client/src/lib.rs`
- Test: `crates/herdr-client/tests/client.rs`

**Interfaces:**
- Consumes: models from Task 2.
- Produces:
  - `herdr_client::Error` enum: `NoSocket`, `Io { method: String, source: std::io::Error }`, `Api { method: String, code: String, message: String }`, `Protocol { method: String, detail: String }`. `Display` for `Api` is `"{method}: {message} ({code})"`.
  - `herdr_client::Api` trait (`Send + Sync`): required `fn request(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, Error>` returning the response's `result` object; provided `workspace_list`, `tab_list`, `agent_list`, `plugin_action_list` returning `Result<Vec<Model>, Error>`.
  - `herdr_client::Client` (`Clone`): `fn new(path: impl Into<PathBuf>) -> Self`, `fn from_env() -> Result<Self, Error>`, implements `Api`.

- [ ] **Step 1: Write the failing tests**

`crates/herdr-client/tests/client.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use herdr_client::{Api, Client, Error};
use serde_json::{Value, json};

/// A fake herdr server: answers one connection per canned response, in order,
/// and returns the requests it received.
fn serve(responses: Vec<String>) -> (tempfile::TempDir, PathBuf, JoinHandle<Vec<Value>>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("herdr.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let handle = thread::spawn(move || {
        let mut received = Vec::new();
        for response in responses {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            received.push(serde_json::from_str(&line).unwrap());
            let mut stream = stream;
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(b"\n").unwrap();
        }
        received
    });
    (dir, path, handle)
}

fn compact_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    serde_json::to_string(&value).unwrap()
}

#[test]
fn request_sends_method_and_params_and_returns_result() {
    let (_dir, path, server) = serve(vec![r#"{"id":"x","result":{"type":"ok"}}"#.into()]);
    let client = Client::new(&path);
    let result = client.request("tab.focus", json!({"tab_id": "w1:t2"})).unwrap();
    assert_eq!(result, json!({"type": "ok"}));

    let received = server.join().unwrap();
    assert_eq!(received[0]["method"], "tab.focus");
    assert_eq!(received[0]["params"], json!({"tab_id": "w1:t2"}));
    assert!(received[0]["id"].as_str().unwrap().starts_with("req_"));
}

#[test]
fn api_error_maps_to_error_api() {
    let (_dir, path, _server) = serve(vec![
        r#"{"id":"x","error":{"code":"pane_not_found","message":"pane bogus not found"}}"#.into(),
    ]);
    let err = Client::new(&path).request("pane.get", json!({"pane_id": "bogus"})).unwrap_err();
    match &err {
        Error::Api { method, code, message } => {
            assert_eq!(method, "pane.get");
            assert_eq!(code, "pane_not_found");
            assert_eq!(message, "pane bogus not found");
        }
        other => panic!("expected Api error, got {other:?}"),
    }
    assert_eq!(err.to_string(), "pane.get: pane bogus not found (pane_not_found)");
}

#[test]
fn invalid_json_is_protocol_error() {
    let (_dir, path, _server) = serve(vec!["not json".into()]);
    let err = Client::new(&path).request("ping", json!({})).unwrap_err();
    assert!(matches!(err, Error::Protocol { ref method, .. } if method == "ping"), "{err:?}");
}

#[test]
fn missing_result_is_protocol_error() {
    let (_dir, path, _server) = serve(vec![r#"{"id":"x"}"#.into()]);
    let err = Client::new(&path).request("ping", json!({})).unwrap_err();
    assert!(matches!(err, Error::Protocol { .. }), "{err:?}");
}

#[test]
fn unreachable_socket_is_io_error_naming_the_method() {
    let dir = tempfile::tempdir().unwrap();
    let err = Client::new(dir.path().join("nope.sock"))
        .request("workspace.list", json!({}))
        .unwrap_err();
    assert!(matches!(err, Error::Io { ref method, .. } if method == "workspace.list"), "{err:?}");
    assert!(err.to_string().starts_with("workspace.list: "));
}

#[test]
fn typed_list_helpers_parse_fixtures() {
    let (_dir, path, server) = serve(vec![
        compact_fixture("workspace_list.json"),
        compact_fixture("tab_list.json"),
        compact_fixture("agent_list.json"),
        compact_fixture("plugin_action_list.json"),
    ]);
    let client = Client::new(&path);
    assert_eq!(client.workspace_list().unwrap().len(), 3);
    assert_eq!(client.tab_list().unwrap().len(), 3);
    assert_eq!(client.agent_list().unwrap().len(), 2);
    assert_eq!(client.plugin_action_list().unwrap()[1].action_id, "refresh");

    let methods: Vec<String> = server
        .join()
        .unwrap()
        .iter()
        .map(|r| r["method"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(methods, ["workspace.list", "tab.list", "agent.list", "plugin.action.list"]);
}

#[test]
fn list_helper_reports_missing_field() {
    let (_dir, path, _server) = serve(vec![r#"{"id":"x","result":{"type":"workspace_list"}}"#.into()]);
    let err = Client::new(&path).workspace_list().unwrap_err();
    match err {
        Error::Protocol { method, detail } => {
            assert_eq!(method, "workspace.list");
            assert!(detail.contains("workspaces"), "{detail}");
        }
        other => panic!("expected Protocol error, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p herdr-client --test client`
Expected: FAIL to compile with `unresolved imports herdr_client::Api, herdr_client::Client, herdr_client::Error`.

- [ ] **Step 3: Implement the client**

`crates/herdr-client/src/client.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::models::{Agent, PluginAction, Tab, Workspace};

const READ_TIMEOUT: Duration = Duration::from_secs(5);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HERDR_SOCKET_PATH is not set")]
    NoSocket,
    #[error("{method}: {source}")]
    Io {
        method: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{method}: {message} ({code})")]
    Api { method: String, code: String, message: String },
    #[error("{method}: invalid response: {detail}")]
    Protocol { method: String, detail: String },
}

/// The herdr socket API. `Client` talks to a real server; tests substitute a fake.
pub trait Api: Send + Sync {
    /// Sends one request and returns the response's `result` object.
    fn request(&self, method: &str, params: Value) -> Result<Value, Error>;

    fn workspace_list(&self) -> Result<Vec<Workspace>, Error> {
        list(self.request("workspace.list", json!({}))?, "workspace.list", "workspaces")
    }

    fn tab_list(&self) -> Result<Vec<Tab>, Error> {
        list(self.request("tab.list", json!({}))?, "tab.list", "tabs")
    }

    fn agent_list(&self) -> Result<Vec<Agent>, Error> {
        list(self.request("agent.list", json!({}))?, "agent.list", "agents")
    }

    fn plugin_action_list(&self) -> Result<Vec<PluginAction>, Error> {
        list(self.request("plugin.action.list", json!({}))?, "plugin.action.list", "actions")
    }
}

fn list<T: DeserializeOwned>(mut result: Value, method: &str, field: &str) -> Result<Vec<T>, Error> {
    let protocol = |detail: String| Error::Protocol { method: method.to_string(), detail };
    let items = result
        .get_mut(field)
        .map(Value::take)
        .ok_or_else(|| protocol(format!("missing `{field}`")))?;
    serde_json::from_value(items).map_err(|e| protocol(e.to_string()))
}

/// One newline-delimited JSON request per connection over herdr's Unix socket.
#[derive(Debug, Clone)]
pub struct Client {
    path: PathBuf,
}

impl Client {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_env() -> Result<Self, Error> {
        std::env::var_os("HERDR_SOCKET_PATH").map(Self::new).ok_or(Error::NoSocket)
    }
}

impl Api for Client {
    fn request(&self, method: &str, params: Value) -> Result<Value, Error> {
        let io = |source: std::io::Error| Error::Io { method: method.to_string(), source };
        let id = format!("req_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');

        let mut stream = UnixStream::connect(&self.path).map_err(io)?;
        stream.set_read_timeout(Some(READ_TIMEOUT)).map_err(io)?;
        stream.write_all(line.as_bytes()).map_err(io)?;

        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).map_err(io)?;
        parse_response(method, &reply)
    }
}

fn parse_response(method: &str, reply: &str) -> Result<Value, Error> {
    let protocol = |detail: String| Error::Protocol { method: method.to_string(), detail };
    let mut value: Value = serde_json::from_str(reply.trim()).map_err(|e| protocol(e.to_string()))?;
    if let Some(error) = value.get("error") {
        return Err(Error::Api {
            method: method.to_string(),
            code: error["code"].as_str().unwrap_or("unknown").to_string(),
            message: error["message"].as_str().unwrap_or_default().to_string(),
        });
    }
    value
        .get_mut("result")
        .map(Value::take)
        .ok_or_else(|| protocol("missing `result`".to_string()))
}
```

Replace `crates/herdr-client/src/lib.rs` with:

```rust
//! Typed client for the herdr socket API, shared by the plugins in this repo.

pub mod client;
pub mod context;
pub mod models;

pub use client::{Api, Client, Error};
pub use context::PluginContext;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p herdr-client`
Expected: PASS, 7 client tests plus 9 model tests.

- [ ] **Step 5: Commit**

```bash
git add crates/herdr-client
git commit -m "feat(herdr-client): add Unix socket client and Api trait

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Built-in commands (steps and request mapping)

**Files:**
- Create: `plugins/command-palette/src/sources/mod.rs`, `plugins/command-palette/src/sources/builtins.rs`
- Modify: `plugins/command-palette/src/main.rs` (add `mod sources;`)

**Interfaces:**
- Consumes: `herdr_client::PluginContext` (Task 2).
- Produces (in `crate::sources::builtins`):
  - `enum Builtin` (`Debug, Clone, Copy, PartialEq, Eq`): `NewWorkspace, RenameWorkspace, CloseWorkspace, NewTab, RenameTab, CloseTab, SplitRight, SplitDown, ToggleZoom, RenamePane, ClosePane, MovePaneToNewTab, MovePaneToNewWorkspace, CreateWorktree, OpenWorktree, ReloadConfig`.
  - `const Builtin::ALL: [Builtin; 16]`
  - `fn title(self) -> &'static str`, `fn slug(self) -> &'static str`, `fn keywords(self) -> &'static [&'static str]`
  - `enum Step { Run, Prompt { label: String, initial: String }, Confirm { question: String } }` (`Debug, Clone, PartialEq, Eq`)
  - `fn step(self, ctx: &PluginContext) -> Step`
  - `fn request(self, ctx: &PluginContext, input: &str) -> Result<(&'static str, serde_json::Value), String>`
  - Task 8 adds `pub fn items() -> Vec<Item>` to this file.

- [ ] **Step 1: Wire the module**

`plugins/command-palette/src/sources/mod.rs`:

```rust
pub mod builtins;
```

Replace `plugins/command-palette/src/main.rs` with:

```rust
mod sources;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: command-palette ui");
    ExitCode::from(2)
}
```

- [ ] **Step 2: Write the failing tests**

Create `plugins/command-palette/src/sources/builtins.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use herdr_client::PluginContext;
    use serde_json::json;

    fn ctx() -> PluginContext {
        PluginContext {
            workspace_id: Some("w6".into()),
            workspace_label: Some("herdr-plugins".into()),
            tab_id: Some("w6:t1".into()),
            tab_label: Some("Claude".into()),
            focused_pane_id: Some("w6:p1".into()),
            focused_pane_cwd: Some("/repo".into()),
            ..PluginContext::default()
        }
    }

    #[test]
    fn all_lists_every_builtin_once_with_unique_slugs() {
        let mut slugs: Vec<_> = Builtin::ALL.iter().map(|b| b.slug()).collect();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), 16);
    }

    #[test]
    fn requests_map_to_socket_methods() {
        let c = ctx();
        let cases: Vec<(Builtin, &str, &str, serde_json::Value)> = vec![
            (Builtin::NewWorkspace, "api", "workspace.create", json!({"cwd": "/repo", "label": "api", "focus": true})),
            (Builtin::NewWorkspace, "", "workspace.create", json!({"cwd": "/repo", "label": null, "focus": true})),
            (Builtin::RenameWorkspace, " core ", "workspace.rename", json!({"workspace_id": "w6", "label": "core"})),
            (Builtin::CloseWorkspace, "", "workspace.close", json!({"workspace_id": "w6"})),
            (Builtin::NewTab, "logs", "tab.create", json!({"workspace_id": "w6", "cwd": "/repo", "label": "logs", "focus": true})),
            (Builtin::RenameTab, "Review", "tab.rename", json!({"tab_id": "w6:t1", "label": "Review"})),
            (Builtin::CloseTab, "", "tab.close", json!({"tab_id": "w6:t1"})),
            (Builtin::SplitRight, "", "pane.split", json!({"target_pane_id": "w6:p1", "direction": "right", "cwd": "/repo", "focus": true})),
            (Builtin::SplitDown, "", "pane.split", json!({"target_pane_id": "w6:p1", "direction": "down", "cwd": "/repo", "focus": true})),
            (Builtin::ToggleZoom, "", "pane.zoom", json!({"pane_id": "w6:p1", "mode": "toggle"})),
            (Builtin::RenamePane, "server", "pane.rename", json!({"pane_id": "w6:p1", "label": "server"})),
            (Builtin::RenamePane, "", "pane.rename", json!({"pane_id": "w6:p1", "label": null})),
            (Builtin::ClosePane, "", "pane.close", json!({"pane_id": "w6:p1"})),
            (Builtin::MovePaneToNewTab, "", "pane.move", json!({"pane_id": "w6:p1", "destination": {"type": "new_tab", "workspace_id": "w6"}, "focus": true})),
            (Builtin::MovePaneToNewWorkspace, "", "pane.move", json!({"pane_id": "w6:p1", "destination": {"type": "new_workspace"}, "focus": true})),
            (Builtin::CreateWorktree, "feat/x", "worktree.create", json!({"workspace_id": "w6", "branch": "feat/x", "focus": true})),
            (Builtin::OpenWorktree, "main", "worktree.open", json!({"workspace_id": "w6", "branch": "main", "focus": true})),
            (Builtin::ReloadConfig, "", "server.reload_config", json!({})),
        ];
        for (builtin, input, method, params) in cases {
            assert_eq!(builtin.request(&c, input), Ok((method, params)), "{builtin:?} with {input:?}");
        }
    }

    #[test]
    fn required_input_must_not_be_blank() {
        let c = ctx();
        assert_eq!(Builtin::RenameTab.request(&c, "   "), Err("label cannot be empty".to_string()));
        assert_eq!(Builtin::RenameWorkspace.request(&c, ""), Err("label cannot be empty".to_string()));
        assert_eq!(Builtin::CreateWorktree.request(&c, ""), Err("branch cannot be empty".to_string()));
    }

    #[test]
    fn missing_context_is_an_error_not_a_panic() {
        let empty = PluginContext::default();
        assert_eq!(Builtin::CloseTab.request(&empty, ""), Err("no focused tab".to_string()));
        assert_eq!(Builtin::SplitRight.request(&empty, ""), Err("no focused pane".to_string()));
        assert_eq!(Builtin::CloseWorkspace.request(&empty, ""), Err("no focused workspace".to_string()));
        assert!(Builtin::ReloadConfig.request(&empty, "").is_ok());
        assert!(Builtin::NewWorkspace.request(&empty, "").is_ok());
    }

    #[test]
    fn steps_prompt_confirm_or_run() {
        let c = ctx();
        assert_eq!(Builtin::SplitRight.step(&c), Step::Run);
        assert_eq!(Builtin::ReloadConfig.step(&c), Step::Run);
        assert_eq!(
            Builtin::RenameTab.step(&c),
            Step::Prompt { label: "Rename tab".into(), initial: "Claude".into() }
        );
        assert_eq!(
            Builtin::RenameWorkspace.step(&c),
            Step::Prompt { label: "Rename workspace".into(), initial: "herdr-plugins".into() }
        );
        assert_eq!(
            Builtin::NewWorkspace.step(&c),
            Step::Prompt { label: "New workspace label".into(), initial: String::new() }
        );
        assert_eq!(
            Builtin::CreateWorktree.step(&c),
            Step::Prompt { label: "New worktree branch".into(), initial: String::new() }
        );
        assert_eq!(Builtin::CloseTab.step(&c), Step::Confirm { question: "Close tab \"Claude\"?".into() });
        assert_eq!(
            Builtin::CloseWorkspace.step(&c),
            Step::Confirm { question: "Close workspace \"herdr-plugins\"?".into() }
        );
        assert_eq!(Builtin::ClosePane.step(&c), Step::Confirm { question: "Close the focused pane?".into() });
    }

    #[test]
    fn confirm_without_labels_uses_generic_wording() {
        let empty = PluginContext::default();
        assert_eq!(
            Builtin::CloseTab.step(&empty),
            Step::Confirm { question: "Close the current tab?".into() }
        );
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p command-palette builtins`
Expected: FAIL to compile with `cannot find type Builtin` / `cannot find type Step`.

- [ ] **Step 4: Implement built-ins**

Insert above the test module in `plugins/command-palette/src/sources/builtins.rs`:

```rust
//! Curated herdr commands that map to socket API calls. Each one acts on the
//! context captured when the palette opened (the pane under the popup).

use herdr_client::PluginContext;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    NewTab,
    RenameTab,
    CloseTab,
    SplitRight,
    SplitDown,
    ToggleZoom,
    RenamePane,
    ClosePane,
    MovePaneToNewTab,
    MovePaneToNewWorkspace,
    CreateWorktree,
    OpenWorktree,
    ReloadConfig,
}

/// What the palette does after the user picks a built-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Run,
    Prompt { label: String, initial: String },
    Confirm { question: String },
}

impl Builtin {
    pub const ALL: [Builtin; 16] = [
        Builtin::NewWorkspace,
        Builtin::RenameWorkspace,
        Builtin::CloseWorkspace,
        Builtin::NewTab,
        Builtin::RenameTab,
        Builtin::CloseTab,
        Builtin::SplitRight,
        Builtin::SplitDown,
        Builtin::ToggleZoom,
        Builtin::RenamePane,
        Builtin::ClosePane,
        Builtin::MovePaneToNewTab,
        Builtin::MovePaneToNewWorkspace,
        Builtin::CreateWorktree,
        Builtin::OpenWorktree,
        Builtin::ReloadConfig,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Builtin::NewWorkspace => "New workspace",
            Builtin::RenameWorkspace => "Rename workspace",
            Builtin::CloseWorkspace => "Close workspace",
            Builtin::NewTab => "New tab",
            Builtin::RenameTab => "Rename tab",
            Builtin::CloseTab => "Close tab",
            Builtin::SplitRight => "Split pane right",
            Builtin::SplitDown => "Split pane down",
            Builtin::ToggleZoom => "Toggle pane zoom",
            Builtin::RenamePane => "Rename pane",
            Builtin::ClosePane => "Close pane",
            Builtin::MovePaneToNewTab => "Move pane to new tab",
            Builtin::MovePaneToNewWorkspace => "Move pane to new workspace",
            Builtin::CreateWorktree => "Create worktree",
            Builtin::OpenWorktree => "Open worktree",
            Builtin::ReloadConfig => "Reload herdr config",
        }
    }

    /// Stable id used as the frecency key (`cmd:<slug>`).
    pub fn slug(self) -> &'static str {
        match self {
            Builtin::NewWorkspace => "new-workspace",
            Builtin::RenameWorkspace => "rename-workspace",
            Builtin::CloseWorkspace => "close-workspace",
            Builtin::NewTab => "new-tab",
            Builtin::RenameTab => "rename-tab",
            Builtin::CloseTab => "close-tab",
            Builtin::SplitRight => "split-right",
            Builtin::SplitDown => "split-down",
            Builtin::ToggleZoom => "toggle-zoom",
            Builtin::RenamePane => "rename-pane",
            Builtin::ClosePane => "close-pane",
            Builtin::MovePaneToNewTab => "move-pane-new-tab",
            Builtin::MovePaneToNewWorkspace => "move-pane-new-workspace",
            Builtin::CreateWorktree => "create-worktree",
            Builtin::OpenWorktree => "open-worktree",
            Builtin::ReloadConfig => "reload-config",
        }
    }

    /// Extra words that match this command but are not shown.
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            Builtin::NewWorkspace | Builtin::NewTab | Builtin::CreateWorktree => &["create", "add"],
            Builtin::CloseWorkspace | Builtin::CloseTab | Builtin::ClosePane => &["kill", "remove"],
            Builtin::SplitRight => &["vertical"],
            Builtin::SplitDown => &["horizontal"],
            Builtin::ToggleZoom => &["maximize", "fullscreen"],
            Builtin::ReloadConfig => &["refresh", "settings"],
            _ => &[],
        }
    }

    pub fn step(self, ctx: &PluginContext) -> Step {
        match self {
            Builtin::NewWorkspace => prompt("New workspace label", None),
            Builtin::RenameWorkspace => prompt("Rename workspace", ctx.workspace_label.as_deref()),
            Builtin::CloseWorkspace => confirm("workspace", ctx.workspace_label.as_deref()),
            Builtin::NewTab => prompt("New tab label", None),
            Builtin::RenameTab => prompt("Rename tab", ctx.tab_label.as_deref()),
            Builtin::CloseTab => confirm("tab", ctx.tab_label.as_deref()),
            Builtin::RenamePane => prompt("Rename pane", None),
            Builtin::ClosePane => Step::Confirm { question: "Close the focused pane?".into() },
            Builtin::CreateWorktree => prompt("New worktree branch", None),
            Builtin::OpenWorktree => prompt("Open worktree branch", None),
            Builtin::SplitRight
            | Builtin::SplitDown
            | Builtin::ToggleZoom
            | Builtin::MovePaneToNewTab
            | Builtin::MovePaneToNewWorkspace
            | Builtin::ReloadConfig => Step::Run,
        }
    }

    /// The socket method and params for this command. `input` is the prompt
    /// text (empty for commands without a prompt).
    pub fn request(self, ctx: &PluginContext, input: &str) -> Result<(&'static str, Value), String> {
        let workspace = || need(&ctx.workspace_id, "workspace");
        let tab = || need(&ctx.tab_id, "tab");
        let pane = || need(&ctx.focused_pane_id, "pane");
        let cwd = &ctx.focused_pane_cwd;
        let label = optional(input);

        Ok(match self {
            Builtin::NewWorkspace => {
                ("workspace.create", json!({"cwd": cwd, "label": label, "focus": true}))
            }
            Builtin::RenameWorkspace => (
                "workspace.rename",
                json!({"workspace_id": workspace()?, "label": required(input, "label")?}),
            ),
            Builtin::CloseWorkspace => ("workspace.close", json!({"workspace_id": workspace()?})),
            Builtin::NewTab => (
                "tab.create",
                json!({"workspace_id": workspace()?, "cwd": cwd, "label": label, "focus": true}),
            ),
            Builtin::RenameTab => {
                ("tab.rename", json!({"tab_id": tab()?, "label": required(input, "label")?}))
            }
            Builtin::CloseTab => ("tab.close", json!({"tab_id": tab()?})),
            Builtin::SplitRight => split(pane()?, "right", cwd),
            Builtin::SplitDown => split(pane()?, "down", cwd),
            Builtin::ToggleZoom => ("pane.zoom", json!({"pane_id": pane()?, "mode": "toggle"})),
            Builtin::RenamePane => ("pane.rename", json!({"pane_id": pane()?, "label": label})),
            Builtin::ClosePane => ("pane.close", json!({"pane_id": pane()?})),
            Builtin::MovePaneToNewTab => (
                "pane.move",
                json!({
                    "pane_id": pane()?,
                    "destination": {"type": "new_tab", "workspace_id": workspace()?},
                    "focus": true
                }),
            ),
            Builtin::MovePaneToNewWorkspace => (
                "pane.move",
                json!({"pane_id": pane()?, "destination": {"type": "new_workspace"}, "focus": true}),
            ),
            Builtin::CreateWorktree => (
                "worktree.create",
                json!({"workspace_id": workspace()?, "branch": required(input, "branch")?, "focus": true}),
            ),
            Builtin::OpenWorktree => (
                "worktree.open",
                json!({"workspace_id": workspace()?, "branch": required(input, "branch")?, "focus": true}),
            ),
            Builtin::ReloadConfig => ("server.reload_config", json!({})),
        })
    }
}

fn prompt(label: &str, initial: Option<&str>) -> Step {
    Step::Prompt { label: label.to_string(), initial: initial.unwrap_or_default().to_string() }
}

fn confirm(what: &str, label: Option<&str>) -> Step {
    let question = match label {
        Some(label) => format!("Close {what} \"{label}\"?"),
        None => format!("Close the current {what}?"),
    };
    Step::Confirm { question }
}

fn split(pane: String, direction: &str, cwd: &Option<String>) -> (&'static str, Value) {
    (
        "pane.split",
        json!({"target_pane_id": pane, "direction": direction, "cwd": cwd, "focus": true}),
    )
}

fn need(value: &Option<String>, what: &str) -> Result<String, String> {
    value.clone().ok_or_else(|| format!("no focused {what}"))
}

fn optional(input: &str) -> Option<String> {
    let trimmed = input.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn required(input: &str, what: &str) -> Result<String, String> {
    optional(input).ok_or_else(|| format!("{what} cannot be empty"))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p command-palette builtins`
Expected: PASS, 6 tests. (Dead-code warnings are expected until later tasks use these items.)

- [ ] **Step 6: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): add built-in herdr commands with prompt/confirm steps

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: User-defined commands (`commands.toml`)

**Files:**
- Create: `plugins/command-palette/src/sources/user.rs`
- Modify: `plugins/command-palette/src/sources/mod.rs` (add `pub mod user;`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces (in `crate::sources::user`):
  - `struct UserCommand { title: String, run: Option<String>, argv: Option<Vec<String>>, keywords: Vec<String>, cwd: Option<String> }` (`Debug, Clone, PartialEq, Eq, Deserialize`)
  - `fn parse(src: &str) -> Result<Vec<UserCommand>, String>`: the error is a single line prefixed `commands.toml`.
  - `fn load(path: &Path) -> Result<Vec<UserCommand>, String>`: a missing file returns `Ok(vec![])`.
  - `impl UserCommand { fn command(&self, default_cwd: Option<&str>) -> std::process::Command }`
  - Task 8 adds `pub fn items(commands: &[UserCommand]) -> Vec<Item>` to this file.

- [ ] **Step 1: Wire the module**

`plugins/command-palette/src/sources/mod.rs`:

```rust
pub mod builtins;
pub mod user;
```

- [ ] **Step 2: Write the failing tests**

Create `plugins/command-palette/src/sources/user.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn parses_run_and_argv_commands() {
        let cmds = parse(
            r#"
            [[commands]]
            title = "Deploy staging"
            run = "just deploy staging"
            keywords = ["ship"]

            [[commands]]
            title = "Open notes"
            argv = ["open", "-a", "Notes"]
            cwd = "/tmp"
            "#,
        )
        .unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].run.as_deref(), Some("just deploy staging"));
        assert_eq!(cmds[0].keywords, ["ship"]);
        assert_eq!(cmds[1].argv.as_ref().unwrap(), &["open", "-a", "Notes"]);
        assert_eq!(cmds[1].cwd.as_deref(), Some("/tmp"));
    }

    #[test]
    fn empty_file_has_no_commands() {
        assert_eq!(parse("").unwrap(), vec![]);
    }

    #[test]
    fn requires_exactly_one_of_run_or_argv() {
        let both = parse("[[commands]]\ntitle = \"x\"\nrun = \"a\"\nargv = [\"b\"]\n").unwrap_err();
        assert_eq!(both, "commands.toml: command 1 (\"x\") needs exactly one of `run` or `argv`");
        let neither = parse("[[commands]]\ntitle = \"y\"\n").unwrap_err();
        assert!(neither.contains("command 1 (\"y\")"), "{neither}");
        let empty = parse("[[commands]]\ntitle = \"z\"\nargv = []\n").unwrap_err();
        assert!(empty.contains("empty argv"), "{empty}");
    }

    #[test]
    fn syntax_errors_report_the_line_on_one_line() {
        let err = parse("[[commands]]\ntitle =\n").unwrap_err();
        assert!(err.starts_with("commands.toml line 2: "), "{err}");
        assert!(!err.contains('\n'), "{err}");
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let err = parse("[[commands]]\ntitle = \"x\"\nrun = \"a\"\ncommand = \"typo\"\n").unwrap_err();
        assert!(err.starts_with("commands.toml"), "{err}");
    }

    #[test]
    fn missing_file_means_no_commands() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("commands.toml")).unwrap(), vec![]);
    }

    #[test]
    fn load_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("commands.toml");
        std::fs::write(&path, "[[commands]]\ntitle = \"x\"\nrun = \"true\"\n").unwrap();
        assert_eq!(load(&path).unwrap()[0].title, "x");
    }

    #[test]
    fn run_builds_sh_command_with_default_cwd() {
        let cmd = UserCommand {
            title: "t".into(),
            run: Some("echo hi".into()),
            argv: None,
            keywords: vec![],
            cwd: None,
        }
        .command(Some("/repo"));
        assert_eq!(cmd.get_program(), "sh");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [OsStr::new("-c"), OsStr::new("echo hi")]);
        assert_eq!(cmd.get_current_dir().unwrap(), std::path::Path::new("/repo"));
    }

    #[test]
    fn argv_builds_direct_command_and_explicit_cwd_wins() {
        let cmd = UserCommand {
            title: "t".into(),
            run: None,
            argv: Some(vec!["ls".into(), "-la".into()]),
            keywords: vec![],
            cwd: Some("/etc".into()),
        }
        .command(Some("/repo"));
        assert_eq!(cmd.get_program(), "ls");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [OsStr::new("-la")]);
        assert_eq!(cmd.get_current_dir().unwrap(), std::path::Path::new("/etc"));
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_home("~"), std::path::PathBuf::from(&home));
        assert_eq!(expand_home("~/src"), std::path::PathBuf::from(&home).join("src"));
        assert_eq!(expand_home("/abs"), std::path::PathBuf::from("/abs"));
        assert_eq!(expand_home("~other"), std::path::PathBuf::from("~other"));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p command-palette user`
Expected: FAIL to compile with `cannot find function parse` / `cannot find struct UserCommand`.

- [ ] **Step 4: Implement user commands**

Insert above the tests in `plugins/command-palette/src/sources/user.rs`:

```rust
//! User-defined commands from `$HERDR_PLUGIN_CONFIG_DIR/commands.toml`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserCommand {
    pub title: String,
    /// Shell script run with `sh -c`.
    #[serde(default)]
    pub run: Option<String>,
    /// Program and arguments run directly, without a shell.
    #[serde(default)]
    pub argv: Option<Vec<String>>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandsFile {
    #[serde(default)]
    commands: Vec<UserCommand>,
}

pub fn parse(src: &str) -> Result<Vec<UserCommand>, String> {
    let file: CommandsFile = toml::from_str(src).map_err(|err| {
        let line = err.span().map(|span| src[..span.start].matches('\n').count() + 1);
        match line {
            Some(line) => format!("commands.toml line {line}: {}", err.message()),
            None => format!("commands.toml: {}", err.message()),
        }
    })?;
    for (index, cmd) in file.commands.iter().enumerate() {
        let which = format!("command {} (\"{}\")", index + 1, cmd.title);
        match (&cmd.run, &cmd.argv) {
            (Some(_), None) => {}
            (None, Some(argv)) if !argv.is_empty() => {}
            (None, Some(_)) => return Err(format!("commands.toml: {which} has an empty argv")),
            _ => return Err(format!("commands.toml: {which} needs exactly one of `run` or `argv`")),
        }
    }
    Ok(file.commands)
}

pub fn load(path: &Path) -> Result<Vec<UserCommand>, String> {
    match std::fs::read_to_string(path) {
        Ok(src) => parse(&src),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(format!("commands.toml: {err}")),
    }
}

impl UserCommand {
    /// Builds the process to spawn. `default_cwd` is the focused pane's cwd.
    pub fn command(&self, default_cwd: Option<&str>) -> Command {
        let mut cmd = match (&self.run, &self.argv) {
            (Some(script), _) => {
                let mut cmd = Command::new("sh");
                cmd.arg("-c").arg(script);
                cmd
            }
            (None, Some(argv)) => {
                let mut cmd = Command::new(&argv[0]);
                cmd.args(&argv[1..]);
                cmd
            }
            (None, None) => unreachable!("parse() rejects commands without run or argv"),
        };
        if let Some(dir) = self.cwd.as_deref().or(default_cwd) {
            cmd.current_dir(expand_home(dir));
        }
        cmd
    }
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix('~'), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            PathBuf::from(home).join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(path),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p command-palette user`
Expected: PASS, 10 tests.

- [ ] **Step 6: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): parse user-defined commands from commands.toml

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Item model and frecency store

**Files:**
- Create: `plugins/command-palette/src/item.rs`, `plugins/command-palette/src/frecency.rs`
- Modify: `plugins/command-palette/src/main.rs` (add `mod frecency; mod item;`)

**Interfaces:**
- Consumes: `Builtin` (Task 4), `UserCommand` (Task 5), `herdr_client::models::AgentStatus` (Task 2).
- Produces:
  - `crate::item::Kind` (`Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash`): `Workspace, Tab, Agent, Command, Plugin, User`, declared in that order; the derived `Ord` is the tiebreak order. Has `fn badge(self) -> &'static str` returning `"WS" | "TAB" | "AGT" | "CMD" | "PLG" | "USR"`.
  - `crate::item::Action` (`Debug, Clone, PartialEq`): `FocusWorkspace(String), FocusTab(String), FocusPane(String), Builtin(Builtin), InvokePluginAction(String), RunUser(UserCommand)`.
  - `crate::item::Item { kind: Kind, id: String, title: String, subtitle: Option<String>, keywords: Vec<String>, status: Option<AgentStatus>, current: bool, action: Action }` (`Debug, Clone, PartialEq`), with builder `Item::new(kind, id, title, action)` and chainable `.subtitle(s)`, `.keywords(vec)`, `.status(s)`, `.current(bool)`. It also has `fn haystack(&self) -> String`, which is the title first, then the subtitle and keywords, separated by spaces.
  - `crate::frecency::{Frecency, Entry, age_factor, now_unix}`: `Frecency::load(PathBuf) -> (Frecency, Option<String>)`, `score(&self, id: &str, now: u64) -> f64`, `record(&mut self, id: &str, now: u64)`, `save(&mut self, now: u64) -> std::io::Result<()>`.

- [ ] **Step 1: Wire the modules**

Replace `plugins/command-palette/src/main.rs` with:

```rust
mod frecency;
mod item;
mod sources;

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("usage: command-palette ui");
    ExitCode::from(2)
}
```

- [ ] **Step 2: Write the failing tests**

`plugins/command-palette/src/item.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haystack_puts_title_first_then_subtitle_and_keywords() {
        let item = Item::new(Kind::Plugin, "plugin:a.b", "Refresh radar", Action::InvokePluginAction("a.b".into()))
            .subtitle("hhdebb.herdr-radar")
            .keywords(vec!["rescan".into(), "agents".into()]);
        assert_eq!(item.haystack(), "Refresh radar hhdebb.herdr-radar rescan agents");
    }

    #[test]
    fn haystack_without_extras_is_title() {
        let item = Item::new(Kind::Workspace, "ws:w1", "CT", Action::FocusWorkspace("w1".into()));
        assert_eq!(item.haystack(), "CT");
        assert!(!item.current);
        assert_eq!(item.status, None);
    }

    #[test]
    fn kind_order_and_badges() {
        assert!(Kind::Workspace < Kind::Tab && Kind::Tab < Kind::Agent && Kind::Agent < Kind::Command);
        assert!(Kind::Command < Kind::Plugin && Kind::Plugin < Kind::User);
        let badges: Vec<_> = [Kind::Workspace, Kind::Tab, Kind::Agent, Kind::Command, Kind::Plugin, Kind::User]
            .iter()
            .map(|k| k.badge())
            .collect();
        assert_eq!(badges, ["WS", "TAB", "AGT", "CMD", "PLG", "USR"]);
    }
}
```

`plugins/command-palette/src/frecency.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn age_factor_buckets() {
        assert_eq!(age_factor(0), 4.0);
        assert_eq!(age_factor(HOUR - 1), 4.0);
        assert_eq!(age_factor(HOUR), 2.0);
        assert_eq!(age_factor(DAY - 1), 2.0);
        assert_eq!(age_factor(DAY), 0.5);
        assert_eq!(age_factor(WEEK - 1), 0.5);
        assert_eq!(age_factor(WEEK), 0.25);
    }

    #[test]
    fn missing_file_is_empty_without_warning() {
        let dir = tempfile::tempdir().unwrap();
        let (f, warning) = Frecency::load(dir.path().join("frecency.json"));
        assert_eq!(warning, None);
        assert_eq!(f.score("ws:w1", NOW), 0.0);
    }

    #[test]
    fn record_increments_and_scores_by_age() {
        let dir = tempfile::tempdir().unwrap();
        let (mut f, _) = Frecency::load(dir.path().join("frecency.json"));
        f.record("ws:w1", NOW);
        f.record("ws:w1", NOW);
        assert_eq!(f.score("ws:w1", NOW), 8.0);
        assert_eq!(f.score("ws:w1", NOW + DAY), 1.0);
        assert_eq!(f.score("ws:other", NOW), 0.0);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("frecency.json");
        let (mut f, _) = Frecency::load(path.clone());
        f.record("tab:w1:t1", NOW);
        f.save(NOW).unwrap();
        let (loaded, warning) = Frecency::load(path.clone());
        assert_eq!(warning, None);
        assert_eq!(loaded.score("tab:w1:t1", NOW), 4.0);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn save_prunes_entries_older_than_90_days() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frecency.json");
        let (mut f, _) = Frecency::load(path.clone());
        f.record("old", NOW - 91 * DAY);
        f.record("fresh", NOW - DAY);
        f.save(NOW).unwrap();
        let (loaded, _) = Frecency::load(path);
        assert_eq!(loaded.score("old", NOW), 0.0);
        assert!(loaded.score("fresh", NOW) > 0.0);
    }

    #[test]
    fn corrupt_file_is_empty_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frecency.json");
        std::fs::write(&path, "{not json").unwrap();
        let (f, warning) = Frecency::load(path);
        assert!(warning.unwrap().contains("corrupt"));
        assert_eq!(f.score("anything", NOW), 0.0);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p command-palette -- item frecency`
Expected: FAIL to compile (`cannot find type Item`, `cannot find function age_factor`).

- [ ] **Step 4: Implement the item model**

Insert above the tests in `plugins/command-palette/src/item.rs`:

```rust
//! One row in the palette, whatever its source.

use herdr_client::models::AgentStatus;

use crate::sources::builtins::Builtin;
use crate::sources::user::UserCommand;

/// Declaration order is the tiebreak order when scores are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Workspace,
    Tab,
    Agent,
    Command,
    Plugin,
    User,
}

impl Kind {
    pub fn badge(self) -> &'static str {
        match self {
            Kind::Workspace => "WS",
            Kind::Tab => "TAB",
            Kind::Agent => "AGT",
            Kind::Command => "CMD",
            Kind::Plugin => "PLG",
            Kind::User => "USR",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    FocusWorkspace(String),
    FocusTab(String),
    FocusPane(String),
    Builtin(Builtin),
    InvokePluginAction(String),
    RunUser(UserCommand),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: Kind,
    /// Stable frecency key, e.g. `tab:w5:t2` or `cmd:split-right`.
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub keywords: Vec<String>,
    pub status: Option<AgentStatus>,
    /// The workspace/tab/pane the user is already in; hidden when the query is empty.
    pub current: bool,
    pub action: Action,
}

impl Item {
    pub fn new(kind: Kind, id: impl Into<String>, title: impl Into<String>, action: Action) -> Self {
        Self {
            kind,
            id: id.into(),
            title: title.into(),
            subtitle: None,
            keywords: Vec::new(),
            status: None,
            current: false,
            action,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    pub fn status(mut self, status: AgentStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// The text the fuzzy matcher sees. The title comes first so match
    /// positions below the title's length index into the title.
    pub fn haystack(&self) -> String {
        let mut parts = vec![self.title.as_str()];
        parts.extend(self.subtitle.as_deref());
        parts.extend(self.keywords.iter().map(String::as_str));
        parts.join(" ")
    }
}
```

- [ ] **Step 5: Implement frecency**

Insert above the tests in `plugins/command-palette/src/frecency.rs`:

```rust
//! Usage history: items used often and recently rank higher.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;
const PRUNE_AFTER: u64 = 90 * DAY;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub count: u32,
    pub last_used_unix: u64,
}

#[derive(Debug)]
pub struct Frecency {
    path: PathBuf,
    entries: HashMap<String, Entry>,
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn age_factor(age_secs: u64) -> f64 {
    match age_secs {
        a if a < HOUR => 4.0,
        a if a < DAY => 2.0,
        a if a < WEEK => 0.5,
        _ => 0.25,
    }
}

impl Frecency {
    /// Never fails: an unreadable or corrupt file yields an empty store plus a
    /// warning for the log.
    pub fn load(path: PathBuf) -> (Self, Option<String>) {
        let (entries, warning) = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(entries) => (entries, None),
                Err(err) => (HashMap::new(), Some(format!("frecency.json is corrupt, starting fresh: {err}"))),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (HashMap::new(), None),
            Err(err) => (HashMap::new(), Some(format!("cannot read frecency.json: {err}"))),
        };
        (Self { path, entries }, warning)
    }

    pub fn score(&self, id: &str, now: u64) -> f64 {
        self.entries
            .get(id)
            .map(|e| f64::from(e.count) * age_factor(now.saturating_sub(e.last_used_unix)))
            .unwrap_or(0.0)
    }

    pub fn record(&mut self, id: &str, now: u64) {
        let entry = self.entries.entry(id.to_string()).or_insert(Entry { count: 0, last_used_unix: now });
        entry.count = entry.count.saturating_add(1);
        entry.last_used_unix = now;
    }

    /// Prunes stale entries, then writes atomically (temp file + rename).
    pub fn save(&mut self, now: u64) -> std::io::Result<()> {
        self.entries.retain(|_, e| now.saturating_sub(e.last_used_unix) < PRUNE_AFTER);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&self.entries)?)?;
        std::fs::rename(&tmp, &self.path)
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p command-palette -- item frecency`
Expected: PASS, 9 tests.

- [ ] **Step 7: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): add item model and frecency store

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Fuzzy ranking

**Files:**
- Create: `plugins/command-palette/src/matcher.rs`
- Modify: `plugins/command-palette/src/main.rs` (add `mod matcher;`)

**Interfaces:**
- Consumes: `Item`, `Kind` (Task 6).
- Produces (in `crate::matcher`):
  - `const FRECENCY_CAP: f64 = 15.0; const FRECENCY_WEIGHT: f64 = 1.0;`
  - `struct Ranked { index: usize, score: f64, highlights: Vec<usize> }` (`Debug, Clone, PartialEq`): `index` points into the items slice, and `highlights` are char indices into `items[index].title`.
  - `fn rank(query: &str, items: &[Item], frecency: impl Fn(&str) -> f64) -> Vec<Ranked>`

- [ ] **Step 1: Wire the module**

Add `mod matcher;` to `plugins/command-palette/src/main.rs` below `mod item;`.

- [ ] **Step 2: Write the failing tests**

`plugins/command-palette/src/matcher.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Item, Kind};

    fn item(kind: Kind, id: &str, title: &str) -> Item {
        Item::new(kind, id, title, Action::FocusWorkspace(id.into()))
    }

    fn titles<'a>(items: &'a [Item], ranked: &[Ranked]) -> Vec<&'a str> {
        ranked.iter().map(|r| items[r.index].title.as_str()).collect()
    }

    fn no_frecency(_: &str) -> f64 {
        0.0
    }

    #[test]
    fn empty_query_hides_current_and_sorts_by_frecency_then_kind_then_title() {
        let items = vec![
            item(Kind::Command, "cmd:b", "B command"),
            item(Kind::Workspace, "ws:cur", "Current").current(true),
            item(Kind::Tab, "tab:z", "Z tab"),
            item(Kind::Workspace, "ws:a", "A workspace"),
            item(Kind::Command, "cmd:a", "A command"),
        ];
        let ranked = rank("", &items, |id| if id == "cmd:b" { 5.0 } else { 0.0 });
        assert_eq!(titles(&items, &ranked), ["B command", "A workspace", "Z tab", "A command"]);
        assert!(ranked.iter().all(|r| r.highlights.is_empty()));
    }

    #[test]
    fn whitespace_query_counts_as_empty() {
        let items = vec![item(Kind::Workspace, "ws:cur", "Current").current(true)];
        assert!(rank("   ", &items, no_frecency).is_empty());
    }

    #[test]
    fn query_filters_out_non_matches_and_includes_current() {
        let items = vec![
            item(Kind::Workspace, "ws:1", "herdr-plugins").current(true),
            item(Kind::Workspace, "ws:2", "boardwalk"),
        ];
        assert_eq!(titles(&items, &rank("herdr", &items, no_frecency)), ["herdr-plugins"]);
    }

    #[test]
    fn strong_text_match_beats_frecent_weak_match() {
        let items = vec![
            item(Kind::Command, "cmd:swap", "Swap pane left in tab"),
            item(Kind::Command, "cmd:split", "Split pane right"),
        ];
        let ranked = rank("split", &items, |id| if id == "cmd:swap" { 1000.0 } else { 0.0 });
        assert_eq!(titles(&items, &ranked)[0], "Split pane right");
    }

    #[test]
    fn frecency_breaks_equal_text_matches() {
        let items = vec![
            item(Kind::Tab, "tab:ct", "CT › Claude"),
            item(Kind::Tab, "tab:jacob", "jacob › Claude"),
        ];
        let ranked = rank("claude", &items, |id| if id == "tab:jacob" { 3.0 } else { 0.0 });
        assert_eq!(titles(&items, &ranked), ["jacob › Claude", "CT › Claude"]);
    }

    #[test]
    fn highlights_are_char_indices_into_the_title() {
        let items = vec![item(Kind::Tab, "tab:ct", "CT › Claude")];
        let ranked = rank("cl", &items, no_frecency);
        assert_eq!(ranked[0].highlights, [5, 6]);
    }

    #[test]
    fn keyword_only_match_has_no_title_highlights() {
        let items = vec![
            Item::new(Kind::Command, "cmd:close-tab", "Close tab", Action::FocusTab("x".into()))
                .keywords(vec!["kill".into()]),
        ];
        let ranked = rank("kill", &items, no_frecency);
        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].highlights.is_empty());
    }

    #[test]
    fn operator_only_queries_do_not_panic() {
        let items = vec![item(Kind::Workspace, "ws:1", "CT"), item(Kind::Tab, "tab:1", "CT › Claude")];
        for query in ["!", "^", "'", "$", "!!", "^$", "' '"] {
            let ranked = rank(query, &items, no_frecency);
            assert!(ranked.len() <= items.len(), "{query}");
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p command-palette matcher`
Expected: FAIL to compile with `cannot find function rank`.

- [ ] **Step 4: Implement ranking**

Insert above the tests in `plugins/command-palette/src/matcher.rs`:

```rust
//! Fuzzy ranking: nucleo text score plus a capped frecency boost.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::item::Item;

/// Keep the maximum boost below typical gaps between a strong and a weak
/// nucleo match (about 20 points for 5-letter queries), so text relevance
/// wins and frecency mostly breaks ties.
pub const FRECENCY_CAP: f64 = 15.0;
pub const FRECENCY_WEIGHT: f64 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub index: usize,
    pub score: f64,
    /// Char indices into the item's title to highlight.
    pub highlights: Vec<usize>,
}

pub fn rank(query: &str, items: &[Item], frecency: impl Fn(&str) -> f64) -> Vec<Ranked> {
    let mut ranked: Vec<Ranked> = if query.trim().is_empty() {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.current)
            .map(|(index, item)| Ranked { index, score: frecency(&item.id), highlights: Vec::new() })
            .collect()
    } else {
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf = Vec::new();
        let mut indices = Vec::new();
        items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                indices.clear();
                let haystack = item.haystack();
                let score = pattern.indices(Utf32Str::new(&haystack, &mut buf), &mut matcher, &mut indices)?;
                indices.sort_unstable();
                indices.dedup();
                let title_len = item.title.chars().count();
                let highlights = indices.iter().map(|&i| i as usize).filter(|&i| i < title_len).collect();
                let boost = frecency(&item.id).min(FRECENCY_CAP) * FRECENCY_WEIGHT;
                Some(Ranked { index, score: f64::from(score) + boost, highlights })
            })
            .collect()
    };
    ranked.sort_by(|a, b| {
        let (ia, ib) = (&items[a.index], &items[b.index]);
        b.score
            .total_cmp(&a.score)
            .then_with(|| ia.kind.cmp(&ib.kind))
            .then_with(|| ia.title.cmp(&ib.title))
    });
    ranked
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p command-palette matcher`
Expected: PASS, 8 tests. If `strong_text_match_beats_frecent_weak_match` fails, lower `FRECENCY_CAP` (and update the Global Constraints line). Don't change the test.

- [ ] **Step 6: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): rank items with nucleo plus capped frecency boost

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Item sources and parallel loading

**Files:**
- Create: `plugins/command-palette/src/sources/{workspaces,tabs,agents,plugins}.rs`, `plugins/command-palette/src/testing.rs`
- Modify: `plugins/command-palette/src/sources/mod.rs`, `plugins/command-palette/src/sources/builtins.rs` (add `items`), `plugins/command-palette/src/sources/user.rs` (add `items`), `plugins/command-palette/src/main.rs` (add `#[cfg(test)] mod testing;`)

**Interfaces:**
- Consumes: `Api`, models, `PluginContext` (Tasks 2–3); `Item`, `Kind`, `Action` (Task 6); `Builtin` (Task 4); `UserCommand`, `user::load` (Task 5).
- Produces:
  - `crate::sources::Loaded { items: Vec<Item>, notices: Vec<String> }`
  - `crate::sources::load_all(api: &dyn Api, ctx: &PluginContext, config_dir: &Path) -> Loaded`
  - `crate::sources::{workspace_label(workspaces: &[Workspace], id: &str) -> String, tab_label(tabs: &[Tab], id: &str) -> String}`
  - `workspaces::items(&[Workspace], &PluginContext) -> Vec<Item>`, `tabs::items(&[Tab], &[Workspace], &PluginContext) -> Vec<Item>`, `agents::items(&[Agent], &[Workspace], &[Tab], &PluginContext) -> Vec<Item>`, `plugins::items(&[PluginAction]) -> Vec<Item>`, `builtins::items() -> Vec<Item>`, `user::items(&[UserCommand]) -> Vec<Item>`
  - `plugins::SELF_ACTION: &str = "jacob.command-palette.open"`
  - `crate::testing::FakeApi` (test only): `FakeApi::new()`, `.ok(method, result: Value) -> Self`, `.err(method, code, message) -> Self`, `.calls() -> Vec<(String, Value)>`; unfaked methods return `Error::Api { code: "unknown_method", .. }`.

- [ ] **Step 1: Add the FakeApi test helper**

`plugins/command-palette/src/testing.rs`:

```rust
//! Test double for the herdr socket API.

use std::collections::HashMap;
use std::sync::Mutex;

use herdr_client::{Api, Error};
use serde_json::Value;

#[derive(Default)]
pub struct FakeApi {
    responses: HashMap<String, Result<Value, (String, String)>>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl FakeApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ok(mut self, method: &str, result: Value) -> Self {
        self.responses.insert(method.to_string(), Ok(result));
        self
    }

    pub fn err(mut self, method: &str, code: &str, message: &str) -> Self {
        self.responses.insert(method.to_string(), Err((code.to_string(), message.to_string())));
        self
    }

    pub fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().unwrap().clone()
    }
}

impl Api for FakeApi {
    fn request(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.calls.lock().unwrap().push((method.to_string(), params));
        match self.responses.get(method) {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err((code, message))) => Err(Error::Api {
                method: method.to_string(),
                code: code.clone(),
                message: message.clone(),
            }),
            None => Err(Error::Api {
                method: method.to_string(),
                code: "unknown_method".into(),
                message: "not faked".into(),
            }),
        }
    }
}
```

In `plugins/command-palette/src/main.rs`, add below the other `mod` lines:

```rust
#[cfg(test)]
mod testing;
```

- [ ] **Step 2: Write the failing tests for the pure builders**

`plugins/command-palette/src/sources/workspaces.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Kind};
    use herdr_client::models::AgentStatus;
    use serde_json::json;

    #[test]
    fn builds_workspace_items() {
        let workspaces: Vec<Workspace> = serde_json::from_value(json!([
            {"workspace_id": "w1", "number": 1, "label": "CT", "agent_status": "unknown"},
            {"workspace_id": "w5", "number": 4, "label": "V9", "agent_status": "idle"},
            {"workspace_id": "w7", "number": 6, "label": ""}
        ]))
        .unwrap();
        let ctx = PluginContext { workspace_id: Some("w5".into()), ..Default::default() };
        let items = items(&workspaces, &ctx);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, Kind::Workspace);
        assert_eq!(items[0].id, "ws:w1");
        assert_eq!(items[0].title, "CT");
        assert_eq!(items[0].subtitle.as_deref(), Some("#1"));
        assert_eq!(items[0].action, Action::FocusWorkspace("w1".into()));
        assert!(!items[0].current);
        assert!(items[1].current);
        assert_eq!(items[1].status, Some(AgentStatus::Idle));
        assert_eq!(items[2].title, "Workspace 6");
    }
}
```

`plugins/command-palette/src/sources/tabs.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Kind};
    use serde_json::json;

    #[test]
    fn builds_tab_items_titled_with_workspace() {
        let workspaces: Vec<Workspace> =
            serde_json::from_value(json!([{"workspace_id": "w1", "number": 1, "label": "CT"}])).unwrap();
        let tabs: Vec<Tab> = serde_json::from_value(json!([
            {"tab_id": "w1:t3", "workspace_id": "w1", "number": 3, "label": "Claude", "agent_status": "working"},
            {"tab_id": "w9:t1", "workspace_id": "w9", "number": 1, "label": ""}
        ]))
        .unwrap();
        let ctx = PluginContext { tab_id: Some("w1:t3".into()), ..Default::default() };
        let items = items(&tabs, &workspaces, &ctx);
        assert_eq!(items[0].kind, Kind::Tab);
        assert_eq!(items[0].id, "tab:w1:t3");
        assert_eq!(items[0].title, "CT › Claude");
        assert_eq!(items[0].action, Action::FocusTab("w1:t3".into()));
        assert!(items[0].current);
        assert_eq!(items[1].title, "w9 › Tab 1");
        assert!(!items[1].current);
    }
}
```

`plugins/command-palette/src/sources/agents.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Kind};
    use herdr_client::models::AgentStatus;
    use serde_json::json;

    #[test]
    fn builds_agent_items_preferring_name_then_display_then_kind() {
        let workspaces: Vec<Workspace> =
            serde_json::from_value(json!([{"workspace_id": "w5", "number": 4, "label": "V9"}])).unwrap();
        let tabs: Vec<Tab> = serde_json::from_value(json!([
            {"tab_id": "w5:t2", "workspace_id": "w5", "number": 2, "label": "Claude"}
        ]))
        .unwrap();
        let agents: Vec<Agent> = serde_json::from_value(json!([
            {"pane_id": "w5:p2", "workspace_id": "w5", "tab_id": "w5:t2", "agent": "claude",
             "display_agent": "Claude Code", "name": "reviewer", "agent_status": "working"},
            {"pane_id": "w5:p3", "workspace_id": "w5", "tab_id": "w5:t2", "agent": "codex",
             "display_agent": "Codex", "agent_status": "blocked"},
            {"pane_id": "w5:p4", "workspace_id": "w5", "tab_id": "w5:t2", "agent": "pi"},
            {"pane_id": "w5:p5", "workspace_id": "w5", "tab_id": "w5:t2"}
        ]))
        .unwrap();
        let ctx = PluginContext { focused_pane_id: Some("w5:p3".into()), ..Default::default() };
        let items = items(&agents, &workspaces, &tabs, &ctx);
        let titles: Vec<_> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["reviewer (V9)", "Codex (V9)", "pi (V9)", "agent (V9)"]);
        assert_eq!(items[0].kind, Kind::Agent);
        assert_eq!(items[0].id, "agent:w5:p2");
        assert_eq!(items[0].subtitle.as_deref(), Some("Claude"));
        assert_eq!(items[0].status, Some(AgentStatus::Working));
        assert_eq!(items[0].keywords, ["working", "claude"]);
        assert_eq!(items[0].action, Action::FocusPane("w5:p2".into()));
        assert!(items[1].current);
    }
}
```

`plugins/command-palette/src/sources/plugins.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Kind};
    use serde_json::json;

    #[test]
    fn builds_plugin_items_excluding_the_palette_itself() {
        let actions: Vec<PluginAction> = serde_json::from_value(json!([
            {"plugin_id": "jacob.command-palette", "action_id": "open", "title": "Open command palette"},
            {"plugin_id": "hhdebb.herdr-radar", "action_id": "refresh", "title": "Refresh radar",
             "description": "Rescan agents"},
            {"plugin_id": "x.y", "action_id": "go", "title": "Go"}
        ]))
        .unwrap();
        let items = items(&actions);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, Kind::Plugin);
        assert_eq!(items[0].id, "plugin:hhdebb.herdr-radar.refresh");
        assert_eq!(items[0].title, "Refresh radar");
        assert_eq!(items[0].subtitle.as_deref(), Some("hhdebb.herdr-radar"));
        assert_eq!(items[0].keywords, ["Rescan agents"]);
        assert_eq!(items[0].action, Action::InvokePluginAction("hhdebb.herdr-radar.refresh".into()));
        assert!(items[1].keywords.is_empty());
    }
}
```

Append to the existing `mod tests` in `plugins/command-palette/src/sources/builtins.rs`:

```rust
    #[test]
    fn items_cover_every_builtin_as_commands() {
        let items = items();
        assert_eq!(items.len(), 16);
        assert!(items.iter().all(|i| i.kind == crate::item::Kind::Command));
        let split = items.iter().find(|i| i.id == "cmd:split-right").unwrap();
        assert_eq!(split.title, "Split pane right");
        assert_eq!(split.keywords, ["vertical"]);
        assert_eq!(split.action, crate::item::Action::Builtin(Builtin::SplitRight));
    }
```

Append to the existing `mod tests` in `plugins/command-palette/src/sources/user.rs`:

```rust
    #[test]
    fn items_use_title_as_id_and_keep_keywords() {
        let cmds = parse("[[commands]]\ntitle = \"Deploy\"\nrun = \"x\"\nkeywords = [\"ship\"]\n").unwrap();
        let items = items(&cmds);
        assert_eq!(items[0].kind, crate::item::Kind::User);
        assert_eq!(items[0].id, "user:Deploy");
        assert_eq!(items[0].keywords, ["ship"]);
        assert_eq!(items[0].action, crate::item::Action::RunUser(cmds[0].clone()));
    }
```

- [ ] **Step 3: Write the failing tests for `load_all`**

Replace `plugins/command-palette/src/sources/mod.rs` with the module list plus tests only:

```rust
pub mod agents;
pub mod builtins;
pub mod plugins;
pub mod tabs;
pub mod user;
pub mod workspaces;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Kind;
    use crate::testing::FakeApi;
    use serde_json::json;

    fn full_api() -> FakeApi {
        FakeApi::new()
            .ok("workspace.list", json!({"workspaces": [{"workspace_id": "w1", "number": 1, "label": "CT"}]}))
            .ok("tab.list", json!({"tabs": [{"tab_id": "w1:t1", "workspace_id": "w1", "number": 1, "label": "Claude"}]}))
            .ok("agent.list", json!({"agents": [{"pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1", "agent": "claude"}]}))
            .ok("plugin.action.list", json!({"actions": [{"plugin_id": "a.b", "action_id": "c", "title": "Do C"}]}))
    }

    fn kinds(loaded: &Loaded) -> Vec<Kind> {
        let mut kinds: Vec<Kind> = loaded.items.iter().map(|i| i.kind).collect();
        kinds.dedup();
        kinds
    }

    #[test]
    fn loads_every_source() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("commands.toml"), "[[commands]]\ntitle = \"U\"\nrun = \"true\"\n").unwrap();
        let loaded = load_all(&full_api(), &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices, Vec::<String>::new());
        assert_eq!(kinds(&loaded), [Kind::Workspace, Kind::Tab, Kind::Agent, Kind::Command, Kind::Plugin, Kind::User]);
        assert_eq!(loaded.items.iter().find(|i| i.kind == Kind::Tab).unwrap().title, "CT › Claude");
    }

    #[test]
    fn a_failing_source_becomes_a_notice_and_others_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let api = full_api().err("agent.list", "internal", "boom");
        let loaded = load_all(&api, &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices, ["agents unavailable: agent.list: boom (internal)"]);
        assert!(loaded.items.iter().any(|i| i.kind == Kind::Workspace));
        assert!(!loaded.items.iter().any(|i| i.kind == Kind::Agent));
    }

    #[test]
    fn missing_workspaces_fall_back_to_ids_in_titles() {
        let dir = tempfile::tempdir().unwrap();
        let api = full_api().err("workspace.list", "internal", "boom");
        let loaded = load_all(&api, &PluginContext::default(), dir.path());
        assert_eq!(loaded.items.iter().find(|i| i.kind == Kind::Tab).unwrap().title, "w1 › Claude");
        assert_eq!(loaded.notices.len(), 1);
    }

    #[test]
    fn invalid_commands_file_is_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("commands.toml"), "[[commands]]\ntitle =\n").unwrap();
        let loaded = load_all(&full_api(), &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices.len(), 1);
        assert!(loaded.notices[0].starts_with("commands.toml line 2"), "{}", loaded.notices[0]);
        assert!(loaded.items.iter().any(|i| i.kind == Kind::Command));
    }

    #[test]
    fn socket_down_leaves_only_local_sources() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_all(&FakeApi::new(), &PluginContext::default(), dir.path());
        assert_eq!(loaded.notices.len(), 4);
        assert_eq!(kinds(&loaded), [Kind::Command]);
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p command-palette sources`
Expected: FAIL to compile (`cannot find function items`, `cannot find function load_all`, `cannot find struct Loaded`).

- [ ] **Step 5: Implement the builders**

Insert above the tests in `plugins/command-palette/src/sources/workspaces.rs`:

```rust
use herdr_client::PluginContext;
use herdr_client::models::Workspace;

use crate::item::{Action, Item, Kind};

pub fn items(workspaces: &[Workspace], ctx: &PluginContext) -> Vec<Item> {
    workspaces
        .iter()
        .map(|ws| {
            Item::new(
                Kind::Workspace,
                format!("ws:{}", ws.workspace_id),
                super::workspace_label(workspaces, &ws.workspace_id),
                Action::FocusWorkspace(ws.workspace_id.clone()),
            )
            .subtitle(format!("#{}", ws.number))
            .status(ws.agent_status)
            .current(ctx.workspace_id.as_deref() == Some(ws.workspace_id.as_str()))
        })
        .collect()
}
```

Insert above the tests in `plugins/command-palette/src/sources/tabs.rs`:

```rust
use herdr_client::PluginContext;
use herdr_client::models::{Tab, Workspace};

use crate::item::{Action, Item, Kind};

pub fn items(tabs: &[Tab], workspaces: &[Workspace], ctx: &PluginContext) -> Vec<Item> {
    tabs.iter()
        .map(|tab| {
            let title = format!(
                "{} › {}",
                super::workspace_label(workspaces, &tab.workspace_id),
                super::tab_label(tabs, &tab.tab_id)
            );
            Item::new(Kind::Tab, format!("tab:{}", tab.tab_id), title, Action::FocusTab(tab.tab_id.clone()))
                .status(tab.agent_status)
                .current(ctx.tab_id.as_deref() == Some(tab.tab_id.as_str()))
        })
        .collect()
}
```

Insert above the tests in `plugins/command-palette/src/sources/agents.rs`:

```rust
use herdr_client::PluginContext;
use herdr_client::models::{Agent, Tab, Workspace};

use crate::item::{Action, Item, Kind};

pub fn items(agents: &[Agent], workspaces: &[Workspace], tabs: &[Tab], ctx: &PluginContext) -> Vec<Item> {
    agents
        .iter()
        .map(|agent| {
            let name = agent
                .name
                .as_deref()
                .or(agent.display_agent.as_deref())
                .or(agent.agent.as_deref())
                .unwrap_or("agent");
            let title = format!("{name} ({})", super::workspace_label(workspaces, &agent.workspace_id));
            let mut keywords = vec![agent.agent_status.as_str().to_string()];
            keywords.extend(agent.agent.clone());
            Item::new(Kind::Agent, format!("agent:{}", agent.pane_id), title, Action::FocusPane(agent.pane_id.clone()))
                .subtitle(super::tab_label(tabs, &agent.tab_id))
                .keywords(keywords)
                .status(agent.agent_status)
                .current(ctx.focused_pane_id.as_deref() == Some(agent.pane_id.as_str()))
        })
        .collect()
}
```

Insert above the tests in `plugins/command-palette/src/sources/plugins.rs`:

```rust
use herdr_client::models::PluginAction;

use crate::item::{Action, Item, Kind};

/// The palette's own `open` action; listing it would just reopen the palette.
pub const SELF_ACTION: &str = "jacob.command-palette.open";

pub fn items(actions: &[PluginAction]) -> Vec<Item> {
    actions
        .iter()
        .map(|action| (action, action.qualified_id()))
        .filter(|(_, qualified)| qualified != SELF_ACTION)
        .map(|(action, qualified)| {
            Item::new(Kind::Plugin, format!("plugin:{qualified}"), &action.title, Action::InvokePluginAction(qualified))
                .subtitle(&action.plugin_id)
                .keywords(action.description.iter().cloned().collect())
        })
        .collect()
}
```

Add to `plugins/command-palette/src/sources/builtins.rs`, directly above `fn prompt(`:

```rust
pub fn items() -> Vec<crate::item::Item> {
    use crate::item::{Action, Item, Kind};
    Builtin::ALL
        .iter()
        .map(|&builtin| {
            Item::new(Kind::Command, format!("cmd:{}", builtin.slug()), builtin.title(), Action::Builtin(builtin))
                .keywords(builtin.keywords().iter().map(|k| k.to_string()).collect())
        })
        .collect()
}
```

Add to `plugins/command-palette/src/sources/user.rs`, directly above `fn expand_home(`:

```rust
pub fn items(commands: &[UserCommand]) -> Vec<crate::item::Item> {
    use crate::item::{Action, Item, Kind};
    commands
        .iter()
        .map(|cmd| {
            Item::new(Kind::User, format!("user:{}", cmd.title), &cmd.title, Action::RunUser(cmd.clone()))
                .keywords(cmd.keywords.clone())
        })
        .collect()
}
```

- [ ] **Step 6: Implement `load_all` and the label helpers**

In `plugins/command-palette/src/sources/mod.rs`, insert between the `pub mod` lines and the tests:

```rust
use std::path::Path;

use herdr_client::models::{Tab, Workspace};
use herdr_client::{Api, Error, PluginContext};

use crate::item::Item;

pub struct Loaded {
    pub items: Vec<Item>,
    /// One line per source that failed to load, for the footer and the log.
    pub notices: Vec<String>,
}

/// Fetches every remote source in parallel, then builds items. A failing
/// source adds a notice instead of failing the palette.
pub fn load_all(api: &dyn Api, ctx: &PluginContext, config_dir: &Path) -> Loaded {
    let (workspace_list, tab_list, agent_list, action_list) = std::thread::scope(|scope| {
        let workspaces = scope.spawn(|| api.workspace_list());
        let tabs = scope.spawn(|| api.tab_list());
        let agents = scope.spawn(|| api.agent_list());
        let actions = scope.spawn(|| api.plugin_action_list());
        (
            workspaces.join().expect("workspace source panicked"),
            tabs.join().expect("tab source panicked"),
            agents.join().expect("agent source panicked"),
            actions.join().expect("plugin source panicked"),
        )
    });

    let mut notices = Vec::new();
    let ws = or_notice(workspace_list, "workspaces", &mut notices);
    let tabs = or_notice(tab_list, "tabs", &mut notices);
    let agents = or_notice(agent_list, "agents", &mut notices);
    let actions = or_notice(action_list, "plugin actions", &mut notices);

    let mut items = Vec::new();
    items.extend(workspaces::items(&ws, ctx));
    items.extend(tabs::items(&tabs, &ws, ctx));
    items.extend(agents::items(&agents, &ws, &tabs, ctx));
    items.extend(builtins::items());
    items.extend(plugins::items(&actions));
    match user::load(&config_dir.join("commands.toml")) {
        Ok(commands) => items.extend(user::items(&commands)),
        Err(err) => notices.push(err),
    }
    Loaded { items, notices }
}

fn or_notice<T>(result: Result<Vec<T>, Error>, source: &str, notices: &mut Vec<String>) -> Vec<T> {
    result.unwrap_or_else(|err| {
        notices.push(format!("{source} unavailable: {err}"));
        Vec::new()
    })
}

/// A workspace's label, `Workspace <n>` when blank, or its id when unknown.
pub fn workspace_label(workspaces: &[Workspace], id: &str) -> String {
    match workspaces.iter().find(|ws| ws.workspace_id == id) {
        Some(ws) if !ws.label.is_empty() => ws.label.clone(),
        Some(ws) => format!("Workspace {}", ws.number),
        None => id.to_string(),
    }
}

/// A tab's label, `Tab <n>` when blank, or its id when unknown.
pub fn tab_label(tabs: &[Tab], id: &str) -> String {
    match tabs.iter().find(|tab| tab.tab_id == id) {
        Some(tab) if !tab.label.is_empty() => tab.label.clone(),
        Some(tab) => format!("Tab {}", tab.number),
        None => id.to_string(),
    }
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p command-palette`
Expected: PASS, all tests including the 5 `sources::tests` and the 6 new builder tests.

- [ ] **Step 8: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): load workspaces, tabs, agents, plugin and user items in parallel

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Executing actions

**Files:**
- Create: `plugins/command-palette/src/exec.rs`
- Modify: `plugins/command-palette/src/main.rs` (add `mod exec;`)

**Interfaces:**
- Consumes: `Api` (Task 3), `Action` (Task 6), `Builtin::request` (Task 4), `UserCommand::command` (Task 5), `FakeApi` (Task 8).
- Produces: `crate::exec::execute(api: &dyn Api, action: &Action, ctx: &PluginContext, input: &str, herdr_bin: &str) -> Result<(), String>`.

- [ ] **Step 1: Wire the module**

Add `mod exec;` to `plugins/command-palette/src/main.rs` below `mod frecency;`.

- [ ] **Step 2: Write the failing tests**

`plugins/command-palette/src/exec.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::builtins::Builtin;
    use crate::sources::user::UserCommand;
    use crate::testing::FakeApi;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

    fn ctx() -> PluginContext {
        PluginContext {
            workspace_id: Some("w1".into()),
            tab_id: Some("w1:t1".into()),
            focused_pane_id: Some("w1:p1".into()),
            focused_pane_cwd: Some("/tmp".into()),
            ..Default::default()
        }
    }

    fn wait_for(path: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(content) = std::fs::read_to_string(path) {
                if !content.is_empty() {
                    return content;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("{} never appeared", path.display());
    }

    #[test]
    fn focus_actions_call_the_matching_methods() {
        let api = FakeApi::new()
            .ok("workspace.focus", json!({}))
            .ok("tab.focus", json!({}))
            .ok("agent.focus", json!({}));
        execute(&api, &Action::FocusWorkspace("w2".into()), &ctx(), "", "herdr").unwrap();
        execute(&api, &Action::FocusTab("w2:t1".into()), &ctx(), "", "herdr").unwrap();
        execute(&api, &Action::FocusPane("w2:p1".into()), &ctx(), "", "herdr").unwrap();
        assert_eq!(
            api.calls(),
            [
                ("workspace.focus".to_string(), json!({"workspace_id": "w2"})),
                ("tab.focus".to_string(), json!({"tab_id": "w2:t1"})),
                ("agent.focus".to_string(), json!({"target": "w2:p1"})),
            ]
        );
    }

    #[test]
    fn builtin_sends_its_request_with_input() {
        let api = FakeApi::new().ok("tab.rename", json!({}));
        execute(&api, &Action::Builtin(Builtin::RenameTab), &ctx(), "Logs", "herdr").unwrap();
        assert_eq!(api.calls(), [("tab.rename".to_string(), json!({"tab_id": "w1:t1", "label": "Logs"}))]);
    }

    #[test]
    fn builtin_validation_error_skips_the_socket() {
        let api = FakeApi::new();
        let err = execute(&api, &Action::Builtin(Builtin::RenameTab), &ctx(), "  ", "herdr").unwrap_err();
        assert_eq!(err, "label cannot be empty");
        assert!(api.calls().is_empty());
    }

    #[test]
    fn stale_target_api_error_is_returned_as_text() {
        let api = FakeApi::new().err("tab.focus", "tab_not_found", "tab w1:t9 not found");
        let err = execute(&api, &Action::FocusTab("w1:t9".into()), &ctx(), "", "herdr").unwrap_err();
        assert_eq!(err, "tab.focus: tab w1:t9 not found (tab_not_found)");
    }

    #[test]
    fn user_command_runs_detached() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran.txt");
        let cmd = UserCommand {
            title: "t".into(),
            run: Some(format!("pwd > '{}'", marker.display())),
            argv: None,
            keywords: vec![],
            cwd: None,
        };
        execute(&FakeApi::new(), &Action::RunUser(cmd), &ctx(), "", "herdr").unwrap();
        let cwd = wait_for(&marker);
        assert!(cwd.trim().ends_with("tmp"), "ran in {cwd}");
    }

    #[test]
    fn user_command_with_missing_cwd_is_an_error() {
        let cmd = UserCommand {
            title: "t".into(),
            run: None,
            argv: Some(vec!["true".into()]),
            keywords: vec![],
            cwd: Some("/definitely/not/here".into()),
        };
        let err = execute(&FakeApi::new(), &Action::RunUser(cmd), &ctx(), "", "herdr").unwrap_err();
        assert!(err.starts_with("failed to start"), "{err}");
    }

    #[test]
    fn plugin_action_is_invoked_through_herdr_bin_after_a_delay() {
        let dir = tempfile::tempdir().unwrap();
        let fake_herdr = dir.path().join("fake-herdr");
        let args_file = dir.path().join("args.txt");
        std::fs::write(&fake_herdr, format!("#!/bin/sh\necho \"$@\" > '{}'\n", args_file.display())).unwrap();
        std::fs::set_permissions(&fake_herdr, std::fs::Permissions::from_mode(0o755)).unwrap();

        let api = FakeApi::new();
        execute(&api, &Action::InvokePluginAction("a.b.c".into()), &ctx(), "", fake_herdr.to_str().unwrap()).unwrap();
        assert!(!args_file.exists(), "invocation must be delayed until the palette exits");
        assert_eq!(wait_for(&args_file).trim(), "plugin action invoke a.b.c");
        assert!(api.calls().is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p command-palette exec`
Expected: FAIL to compile with `cannot find function execute`.

- [ ] **Step 4: Implement execution**

Insert above the tests in `plugins/command-palette/src/exec.rs`:

```rust
//! Runs the selected item's action.

use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use herdr_client::{Api, PluginContext};
use serde_json::{Value, json};

use crate::item::Action;

/// `input` is the prompt text for built-ins that asked for one.
pub fn execute(api: &dyn Api, action: &Action, ctx: &PluginContext, input: &str, herdr_bin: &str) -> Result<(), String> {
    let call = |method: &str, params: Value| api.request(method, params).map(drop).map_err(|e| e.to_string());
    match action {
        Action::FocusWorkspace(id) => call("workspace.focus", json!({"workspace_id": id})),
        Action::FocusTab(id) => call("tab.focus", json!({"tab_id": id})),
        Action::FocusPane(id) => call("agent.focus", json!({"target": id})),
        Action::Builtin(builtin) => {
            let (method, params) = builtin.request(ctx, input)?;
            call(method, params)
        }
        Action::InvokePluginAction(id) => spawn_detached(&mut delayed_plugin_invoke(herdr_bin, id)),
        Action::RunUser(cmd) => spawn_detached(&mut cmd.command(ctx.focused_pane_cwd.as_deref())),
    }
}

/// The palette popup is a session singleton: invoking an action that opens its
/// own popup while the palette is still up fails with `ui_busy`. The delay lets
/// the palette exit (closing its popup) before herdr runs the action.
fn delayed_plugin_invoke(herdr_bin: &str, action_id: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "sleep 0.2; exec \"$0\" plugin action invoke \"$1\"", herdr_bin, action_id]);
    cmd
}

/// Starts a process that outlives the palette: no inherited stdio (the popup's
/// terminal goes away) and its own process group (no hangup when it does).
fn spawn_detached(cmd: &mut Command) -> Result<(), String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
    cmd.spawn()
        .map(drop)
        .map_err(|err| format!("failed to start {}: {err}", cmd.get_program().to_string_lossy()))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p command-palette exec`
Expected: PASS, 7 tests.

- [ ] **Step 6: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): execute focus, built-in, plugin and user actions

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: App state machine (keys, modes, status)

**Files:**
- Create: `plugins/command-palette/src/app.rs`
- Modify: `plugins/command-palette/src/main.rs` (add `mod app;`)

**Interfaces:**
- Consumes: `Item`, `Action` (Task 6); `rank`, `Ranked` (Task 7); `Step` (Task 4); `PluginContext`.
- Produces (in `crate::app`):
  - `enum Mode { List, Prompt { item: usize, label: String, input: String }, Confirm { item: usize, question: String } }` (`Debug, Clone, PartialEq, Eq`)
  - `enum Status { Info(String), Error(String) }` (`Debug, Clone, PartialEq, Eq`)
  - `enum Command { Continue, Quit, Run { item: usize, input: String } }` (`Debug, Clone, PartialEq, Eq`); `item` indexes `App::items`.
  - `struct App { pub items: Vec<Item>, pub query: String, pub ranked: Vec<Ranked>, pub selected: usize, pub mode: Mode, pub status: Option<Status>, scores: HashMap<String, f64> }`
  - `App::new(items: Vec<Item>, scores: HashMap<String, f64>, status: Option<Status>) -> App`
  - `App::handle_key(&mut self, key: KeyEvent, ctx: &PluginContext) -> Command`
  - `App::fail(&mut self, message: String)`

- [ ] **Step 1: Wire the module**

Add `mod app;` as the first `mod` line in `plugins/command-palette/src/main.rs`.

- [ ] **Step 2: Write the failing tests**

`plugins/command-palette/src/app.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Item, Kind};
    use crate::sources::builtins::Builtin;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    fn ctx() -> PluginContext {
        PluginContext { tab_id: Some("w1:t1".into()), tab_label: Some("Claude".into()), ..Default::default() }
    }

    fn app() -> App {
        let items = vec![
            Item::new(Kind::Workspace, "ws:w1", "CT", Action::FocusWorkspace("w1".into())),
            Item::new(Kind::Workspace, "ws:w2", "boardwalk", Action::FocusWorkspace("w2".into())),
            Item::new(Kind::Command, "cmd:rename-tab", "Rename tab", Action::Builtin(Builtin::RenameTab)),
            Item::new(Kind::Command, "cmd:close-tab", "Close tab", Action::Builtin(Builtin::CloseTab)),
            Item::new(Kind::Command, "cmd:split-right", "Split pane right", Action::Builtin(Builtin::SplitRight)),
        ];
        App::new(items, HashMap::new(), None)
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            assert_eq!(app.handle_key(ch(c), &ctx()), Command::Continue);
        }
    }

    fn selected_title(app: &App) -> &str {
        &app.items[app.ranked[app.selected].index].title
    }

    #[test]
    fn starts_with_all_items_ranked_and_first_selected() {
        let app = app();
        assert_eq!(app.ranked.len(), 5);
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, Mode::List);
    }

    #[test]
    fn typing_filters_and_resets_selection() {
        let mut app = app();
        app.handle_key(key(KeyCode::Down), &ctx());
        type_str(&mut app, "board");
        assert_eq!(app.query, "board");
        assert_eq!(app.ranked.len(), 1);
        assert_eq!(app.selected, 0);
        assert_eq!(selected_title(&app), "boardwalk");
        app.handle_key(key(KeyCode::Backspace), &ctx());
        assert_eq!(app.query, "boar");
        app.handle_key(ctrl('u'), &ctx());
        assert_eq!(app.query, "");
        assert_eq!(app.ranked.len(), 5);
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut app = app();
        app.handle_key(key(KeyCode::Up), &ctx());
        assert_eq!(app.selected, 0);
        app.handle_key(ctrl('n'), &ctx());
        app.handle_key(key(KeyCode::Down), &ctx());
        assert_eq!(app.selected, 2);
        for _ in 0..10 {
            app.handle_key(key(KeyCode::Down), &ctx());
        }
        assert_eq!(app.selected, 4);
        app.handle_key(ctrl('p'), &ctx());
        assert_eq!(app.selected, 3);
    }

    #[test]
    fn enter_on_plain_item_runs_it() {
        let mut app = app();
        type_str(&mut app, "boardwalk");
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 1, input: String::new() });
    }

    #[test]
    fn enter_on_run_step_builtin_runs_it() {
        let mut app = app();
        type_str(&mut app, "split");
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 4, input: String::new() });
    }

    #[test]
    fn enter_with_no_results_does_nothing() {
        let mut app = app();
        type_str(&mut app, "zzzzzz");
        assert!(app.ranked.is_empty());
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Continue);
    }

    #[test]
    fn prompt_flow_prefills_edits_and_submits() {
        let mut app = app();
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.mode, Mode::Prompt { item: 2, label: "Rename tab".into(), input: "Claude".into() });
        app.handle_key(ctrl('u'), &ctx());
        type_str(&mut app, "Logs");
        app.handle_key(key(KeyCode::Backspace), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 2, input: "Log".into() });
    }

    #[test]
    fn esc_in_prompt_returns_to_list_with_query_intact() {
        let mut app = app();
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Esc), &ctx()), Command::Continue);
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.query, "rename");
    }

    #[test]
    fn confirm_accepts_enter_or_y_and_rejects_esc_or_n() {
        let mut app = app();
        type_str(&mut app, "close");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.mode, Mode::Confirm { item: 3, question: "Close tab \"Claude\"?".into() });
        assert_eq!(app.handle_key(ch('x'), &ctx()), Command::Continue);
        assert_eq!(app.handle_key(ch('n'), &ctx()), Command::Continue);
        assert_eq!(app.mode, Mode::List);

        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(ch('y'), &ctx()), Command::Run { item: 3, input: String::new() });

        app.mode = Mode::List;
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Esc), &ctx()), Command::Continue);
        assert_eq!(app.mode, Mode::List);

        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(key(KeyCode::Enter), &ctx()), Command::Run { item: 3, input: String::new() });
    }

    #[test]
    fn esc_in_list_and_ctrl_c_anywhere_quit() {
        let mut app = app();
        assert_eq!(app.handle_key(key(KeyCode::Esc), &ctx()), Command::Quit);
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        assert_eq!(app.handle_key(ctrl('c'), &ctx()), Command::Quit);
    }

    #[test]
    fn failure_in_list_keeps_list_and_shows_error() {
        let mut app = app();
        app.fail("tab.focus: tab w1:t9 not found (tab_not_found)".into());
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.status, Some(Status::Error("tab.focus: tab w1:t9 not found (tab_not_found)".into())));
    }

    #[test]
    fn failure_in_confirm_returns_to_list_and_in_prompt_stays() {
        let mut app = app();
        type_str(&mut app, "close");
        app.handle_key(key(KeyCode::Enter), &ctx());
        app.fail("boom".into());
        assert_eq!(app.mode, Mode::List);

        app.handle_key(ctrl('u'), &ctx());
        type_str(&mut app, "rename");
        app.handle_key(key(KeyCode::Enter), &ctx());
        app.fail("label cannot be empty".into());
        assert!(matches!(app.mode, Mode::Prompt { .. }));
        assert_eq!(app.status, Some(Status::Error("label cannot be empty".into())));
    }

    #[test]
    fn frecency_scores_order_the_empty_query() {
        let items = vec![
            Item::new(Kind::Workspace, "ws:a", "A", Action::FocusWorkspace("a".into())),
            Item::new(Kind::Workspace, "ws:b", "B", Action::FocusWorkspace("b".into())),
        ];
        let app = App::new(items, HashMap::from([("ws:b".to_string(), 4.0)]), Some(Status::Info("hi".into())));
        assert_eq!(selected_title(&app), "B");
        assert_eq!(app.status, Some(Status::Info("hi".into())));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p command-palette app`
Expected: FAIL to compile with `cannot find struct App`.

- [ ] **Step 4: Implement the state machine**

Insert above the tests in `plugins/command-palette/src/app.rs`:

```rust
//! Palette state and key handling, independent of the terminal.

use std::collections::HashMap;

use herdr_client::PluginContext;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::item::{Action, Item};
use crate::matcher::{self, Ranked};
use crate::sources::builtins::Step;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    List,
    Prompt { item: usize, label: String, input: String },
    Confirm { item: usize, question: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Continue,
    Quit,
    /// Run `items[item]` with the given prompt input.
    Run { item: usize, input: String },
}

pub struct App {
    pub items: Vec<Item>,
    pub query: String,
    pub ranked: Vec<Ranked>,
    /// Index into `ranked`.
    pub selected: usize,
    pub mode: Mode,
    pub status: Option<Status>,
    scores: HashMap<String, f64>,
}

impl App {
    pub fn new(items: Vec<Item>, scores: HashMap<String, f64>, status: Option<Status>) -> Self {
        let mut app = Self {
            items,
            query: String::new(),
            ranked: Vec::new(),
            selected: 0,
            mode: Mode::List,
            status,
            scores,
        };
        app.refilter();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent, ctx: &PluginContext) -> Command {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Command::Quit;
        }
        match self.mode {
            Mode::List => self.list_key(key, ctrl, ctx),
            Mode::Prompt { .. } => self.prompt_key(key, ctrl),
            Mode::Confirm { .. } => self.confirm_key(key),
        }
    }

    /// Shows an action failure. A confirm step is abandoned; a prompt stays
    /// open so the input can be corrected.
    pub fn fail(&mut self, message: String) {
        if matches!(self.mode, Mode::Confirm { .. }) {
            self.mode = Mode::List;
        }
        self.status = Some(Status::Error(message));
    }

    fn list_key(&mut self, key: KeyEvent, ctrl: bool, ctx: &PluginContext) -> Command {
        match key.code {
            KeyCode::Esc => return Command::Quit,
            KeyCode::Enter => return self.activate(ctx),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Char('p') if ctrl => self.move_selection(-1),
            KeyCode::Char('n') if ctrl => self.move_selection(1),
            KeyCode::Char('u') if ctrl => {
                self.query.clear();
                self.refilter();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.refilter();
            }
            _ => {}
        }
        Command::Continue
    }

    fn prompt_key(&mut self, key: KeyEvent, ctrl: bool) -> Command {
        let Mode::Prompt { item, input, .. } = &mut self.mode else {
            return Command::Continue;
        };
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                self.status = None;
            }
            KeyCode::Enter => return Command::Run { item: *item, input: input.clone() },
            KeyCode::Char('u') if ctrl => input.clear(),
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) if !ctrl => input.push(c),
            _ => {}
        }
        Command::Continue
    }

    fn confirm_key(&mut self, key: KeyEvent) -> Command {
        let Mode::Confirm { item, .. } = self.mode else {
            return Command::Continue;
        };
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => Command::Run { item, input: String::new() },
            KeyCode::Esc | KeyCode::Char('n') => {
                self.mode = Mode::List;
                Command::Continue
            }
            _ => Command::Continue,
        }
    }

    fn activate(&mut self, ctx: &PluginContext) -> Command {
        let Some(index) = self.ranked.get(self.selected).map(|r| r.index) else {
            return Command::Continue;
        };
        let run = Command::Run { item: index, input: String::new() };
        let Action::Builtin(builtin) = &self.items[index].action else {
            return run;
        };
        match builtin.step(ctx) {
            Step::Run => run,
            Step::Prompt { label, initial } => {
                self.mode = Mode::Prompt { item: index, label, input: initial };
                self.status = None;
                Command::Continue
            }
            Step::Confirm { question } => {
                self.mode = Mode::Confirm { item: index, question };
                self.status = None;
                Command::Continue
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let last = self.ranked.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    fn refilter(&mut self) {
        let scores = &self.scores;
        self.ranked = matcher::rank(&self.query, &self.items, |id| scores.get(id).copied().unwrap_or(0.0));
        self.selected = 0;
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p command-palette app`
Expected: PASS, 13 tests.

- [ ] **Step 6: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): add key-driven app state with prompt and confirm steps

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Rendering

**Files:**
- Create: `plugins/command-palette/src/render.rs`
- Modify: `plugins/command-palette/src/main.rs` (add `mod render;`)

**Interfaces:**
- Consumes: `App`, `Mode`, `Status` (Task 10); `Item`, `Kind` (Task 6); `AgentStatus`.
- Produces: `crate::render::render(frame: &mut ratatui::Frame, app: &App)`.

Layout, top to bottom:
1. The input line: `❯ <query>` in list mode, `<label> › <input>` in prompt mode, or the question in confirm mode.
2. A dim `─` rule.
3. The list.
4. The footer: key hints on the left, and status or `N items` on the right.

Each list row is `▸ ` or two spaces, then the badge padded to 4, a space, the title with highlighted chars, `  subtitle` dimmed, and `  ● status` colored. The status dot is omitted for `Unknown`.

- [ ] **Step 1: Wire the module**

Add `mod render;` to `plugins/command-palette/src/main.rs` below `mod matcher;`.

- [ ] **Step 2: Write the failing tests**

`plugins/command-palette/src/render.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Mode, Status};
    use crate::item::{Action, Item, Kind};
    use herdr_client::models::AgentStatus;
    use ratatui::{Terminal, backend::TestBackend};
    use std::collections::HashMap;

    fn draw(app: &App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                let line: String = (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect();
                line.trim_end().to_string()
            })
            .collect()
    }

    fn sample() -> App {
        let items = vec![
            Item::new(Kind::Workspace, "ws:w5", "V9 Orchestrator", Action::FocusWorkspace("w5".into()))
                .subtitle("#4")
                .status(AgentStatus::Idle),
            Item::new(Kind::Tab, "tab:w1:t3", "CT › Claude", Action::FocusTab("w1:t3".into()))
                .status(AgentStatus::Unknown),
            Item::new(Kind::Command, "cmd:split-right", "Split pane right", Action::FocusTab("x".into())),
        ];
        App::new(items, HashMap::new(), None)
    }

    #[test]
    fn list_mode_layout() {
        let lines = draw(&sample(), 60, 8);
        assert_eq!(lines[0], "❯");
        assert!(lines[1].starts_with("────"), "{:?}", lines[1]);
        assert_eq!(lines[2], "▸ WS   V9 Orchestrator  #4  ● idle");
        assert_eq!(lines[3], "  TAB  CT › Claude");
        assert_eq!(lines[4], "  CMD  Split pane right");
        assert!(lines[7].starts_with("↑↓ move  ⏎ run  esc close"), "{:?}", lines[7]);
        assert!(lines[7].ends_with("3 items"), "{:?}", lines[7]);
    }

    #[test]
    fn query_and_no_matches() {
        let mut app = sample();
        app.query = "zzz".into();
        app.ranked.clear();
        let lines = draw(&app, 60, 8);
        assert_eq!(lines[0], "❯ zzz");
        assert_eq!(lines[2], "  no matches");
        assert!(lines[7].ends_with("0 items"));
    }

    #[test]
    fn prompt_mode_shows_label_and_input() {
        let mut app = sample();
        app.mode = Mode::Prompt { item: 1, label: "Rename tab".into(), input: "Claude".into() };
        let lines = draw(&app, 60, 8);
        assert_eq!(lines[0], "Rename tab › Claude");
        assert!(lines[7].starts_with("⏎ submit  esc back"), "{:?}", lines[7]);
    }

    #[test]
    fn confirm_mode_shows_question() {
        let mut app = sample();
        app.mode = Mode::Confirm { item: 1, question: "Close tab \"Claude\"?".into() };
        let lines = draw(&app, 60, 8);
        assert_eq!(lines[0], "Close tab \"Claude\"?");
        assert!(lines[7].starts_with("y/⏎ confirm  n/esc cancel"), "{:?}", lines[7]);
    }

    #[test]
    fn status_replaces_item_count() {
        let mut app = sample();
        app.status = Some(Status::Error("tab.focus: gone (tab_not_found)".into()));
        assert!(draw(&app, 70, 8)[7].ends_with("tab.focus: gone (tab_not_found)"));
        app.status = Some(Status::Info("agents unavailable".into()));
        assert!(draw(&app, 70, 8)[7].ends_with("agents unavailable"));
    }

    #[test]
    fn selection_scrolls_into_view() {
        let items: Vec<Item> = (0..20)
            .map(|i| Item::new(Kind::Command, format!("cmd:{i}"), format!("Item {i:02}"), Action::FocusTab("x".into())))
            .collect();
        let mut app = App::new(items, HashMap::new(), None);
        app.selected = 12;
        let lines = draw(&app, 40, 8);
        let selected: Vec<_> = lines.iter().filter(|l| l.starts_with('▸')).collect();
        assert_eq!(selected.len(), 1);
        assert!(selected[0].contains("Item 12"), "{selected:?}");
    }

    #[test]
    fn long_unicode_titles_are_clipped_without_panicking() {
        let title = "🚀 deploy ".repeat(20);
        let items = vec![Item::new(Kind::Tab, "tab:x", title, Action::FocusTab("x".into())).subtitle("ünïcødé")];
        let app = App::new(items, HashMap::new(), None);
        let lines = draw(&app, 40, 6);
        // A wide emoji occupies two cells; the buffer stores a blank in the second.
        assert!(lines[2].starts_with("▸ TAB  🚀"), "{:?}", lines[2]);
        assert!(lines[2].contains("deploy"), "{:?}", lines[2]);
        assert!(lines[2].chars().count() <= 40, "{:?}", lines[2]);
    }

    #[test]
    fn tiny_areas_do_not_panic() {
        let app = sample();
        for (w, h) in [(10, 1), (20, 2), (20, 3), (1, 1), (60, 4)] {
            draw(&app, w, h);
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p command-palette render`
Expected: FAIL to compile with `cannot find function render`.

- [ ] **Step 4: Implement rendering**

Insert above the tests in `plugins/command-palette/src/render.rs`:

```rust
//! Draws the palette. Uses the terminal's own colors plus ANSI accents so it
//! follows the user's herdr/terminal theme.

use herdr_client::models::AgentStatus;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app::{App, Mode, Status};
use crate::item::{Item, Kind};

pub fn render(frame: &mut Frame, app: &App) {
    let [input, rule, list, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_input(frame, app, input);
    frame.render_widget(Line::from("─".repeat(usize::from(rule.width))).dim(), rule);
    render_list(frame, app, list);
    render_footer(frame, app, footer);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let (prefix, text) = match &app.mode {
        Mode::List => ("❯ ".to_string(), app.query.as_str()),
        Mode::Prompt { label, input, .. } => (format!("{label} › "), input.as_str()),
        Mode::Confirm { question, .. } => {
            frame.render_widget(Line::from(question.as_str()).bold(), area);
            return;
        }
    };
    let line = Line::from(vec![Span::raw(prefix).cyan().bold(), Span::raw(text)]);
    let width = u16::try_from(line.width()).unwrap_or(u16::MAX);
    frame.render_widget(line, area);
    if area.width > 0 && area.height > 0 {
        let x = area.x.saturating_add(width).min(area.right().saturating_sub(1));
        frame.set_cursor_position((x, area.y));
    }
}

fn render_list(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    if app.ranked.is_empty() {
        frame.render_widget(Line::from("  no matches").dim(), area);
        return;
    }
    let height = usize::from(area.height);
    let offset = app.selected.saturating_sub(height - 1);
    for (row, (position, ranked)) in app.ranked.iter().enumerate().skip(offset).take(height).enumerate() {
        let selected = position == app.selected;
        let mut line = item_line(&app.items[ranked.index], &ranked.highlights, selected);
        if selected {
            line = line.style(Style::new().bg(Color::DarkGray));
        }
        let row_area = Rect { y: area.y + row as u16, height: 1, ..area };
        frame.render_widget(line, row_area);
    }
}

fn item_line<'a>(item: &'a Item, highlights: &[usize], selected: bool) -> Line<'a> {
    let mut spans = vec![
        Span::raw(if selected { "▸ " } else { "  " }),
        Span::styled(format!("{:<4}", item.kind.badge()), badge_style(item.kind)),
        Span::raw(" "),
    ];
    spans.extend(item.title.chars().enumerate().map(|(i, c)| {
        let span = Span::raw(c.to_string());
        if highlights.contains(&i) { span.yellow().bold() } else { span }
    }));
    if let Some(subtitle) = &item.subtitle {
        spans.push(Span::raw(format!("  {subtitle}")).dim());
    }
    if let Some(status) = item.status.filter(|s| *s != AgentStatus::Unknown) {
        spans.push(Span::styled(format!("  ● {}", status.as_str()), status_style(status)));
    }
    Line::from(spans)
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints = match app.mode {
        Mode::List => "↑↓ move  ⏎ run  esc close",
        Mode::Prompt { .. } => "⏎ submit  esc back",
        Mode::Confirm { .. } => "y/⏎ confirm  n/esc cancel",
    };
    let hints = Line::from(hints).dim();
    let right = match &app.status {
        Some(Status::Error(message)) => Line::from(message.as_str()).red(),
        Some(Status::Info(message)) => Line::from(message.as_str()).dim(),
        None => Line::from(format!("{} items", app.ranked.len())).dim(),
    };
    let hints_width = u16::try_from(hints.width() + 2).unwrap_or(u16::MAX);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Length(hints_width), Constraint::Min(0)]).areas(area);
    frame.render_widget(hints, left_area);
    frame.render_widget(right.right_aligned(), right_area);
}

fn badge_style(kind: Kind) -> Style {
    let color = match kind {
        Kind::Workspace => Color::Magenta,
        Kind::Tab => Color::Blue,
        Kind::Agent => Color::Green,
        Kind::Command => Color::Cyan,
        Kind::Plugin => Color::Yellow,
        Kind::User => Color::Red,
    };
    Style::new().fg(color).bold()
}

fn status_style(status: AgentStatus) -> Style {
    let color = match status {
        AgentStatus::Working => Color::Yellow,
        AgentStatus::Blocked => Color::Red,
        AgentStatus::Done => Color::Green,
        AgentStatus::Idle => Color::Blue,
        AgentStatus::Unknown => Color::DarkGray,
    };
    Style::new().fg(color)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p command-palette render`
Expected: PASS, 8 tests. If `list_mode_layout` or `status_replaces_item_count` fails only because of the footer split, adjust the footer layout, not the assertions. The assertions encode the spec'd layout.

- [ ] **Step 6: Commit**

```bash
git add plugins/command-palette/src
git commit -m "feat(palette): render input, ranked list with highlights, and footer

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: Wire the `ui` entrypoint, verify live, and document setup

**Files:**
- Create: `plugins/command-palette/src/log.rs`
- Modify: `plugins/command-palette/src/main.rs` (full entrypoint)
- Modify: `README.md` (palette section)
- User config (ask first; see Step 6): `~/.config/herdr/config.toml`, `~/.config/ghostty/config`

**Interfaces:**
- Consumes: everything above.
- Produces: `command-palette ui`, the popup entrypoint referenced by the manifest; and `crate::log::append(dir: &Path, message: &str)`.

- [ ] **Step 1: Write the failing log test**

`plugins/command-palette/src/log.rs` (tests only for now):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_timestamped_lines_creating_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        append(&state, "first");
        append(&state, "second");
        let log = std::fs::read_to_string(state.join("palette.log")).unwrap();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" first"));
        assert!(lines[1].ends_with(" second"));
    }
}
```

Add `mod log;` to `main.rs` below `mod item;`.

Run: `cargo test -p command-palette log`
Expected: FAIL to compile with `cannot find function append`.

- [ ] **Step 2: Implement the log**

Insert above the tests in `plugins/command-palette/src/log.rs`:

```rust
//! Diagnostics go to a file: the palette owns the terminal while it runs.

use std::io::Write;
use std::path::Path;

use crate::frecency::now_unix;

/// Best effort: logging must never break the palette.
pub fn append(dir: &Path, message: &str) {
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("palette.log"))?;
        writeln!(file, "{} {message}", now_unix())
    };
    let _ = write();
}
```

Run: `cargo test -p command-palette log`
Expected: PASS.

- [ ] **Step 3: Implement the entrypoint**

Replace `plugins/command-palette/src/main.rs` with:

```rust
mod app;
mod exec;
mod frecency;
mod item;
mod log;
mod matcher;
mod render;
mod sources;
#[cfg(test)]
mod testing;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use herdr_client::{Client, PluginContext};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::{App, Command, Status};
use frecency::{Frecency, now_unix};

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("ui") => match run_ui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("command-palette: {err}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: command-palette ui");
            ExitCode::from(2)
        }
    }
}

fn env_dir(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("herdr-command-palette"))
}

fn run_ui() -> std::io::Result<()> {
    let ctx = PluginContext::from_env();
    let state_dir = env_dir("HERDR_PLUGIN_STATE_DIR");
    let config_dir = env_dir("HERDR_PLUGIN_CONFIG_DIR");
    let herdr_bin = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());

    let (mut frecency, warning) = Frecency::load(state_dir.join("frecency.json"));
    if let Some(warning) = warning {
        log::append(&state_dir, &warning);
    }

    let client = Client::from_env();
    let (items, status) = match &client {
        Ok(client) => {
            let loaded = sources::load_all(client, &ctx, &config_dir);
            for notice in &loaded.notices {
                log::append(&state_dir, notice);
            }
            (loaded.items, loaded.notices.into_iter().next().map(Status::Info))
        }
        Err(err) => {
            log::append(&state_dir, &err.to_string());
            (Vec::new(), Some(Status::Error(err.to_string())))
        }
    };

    let now = now_unix();
    let scores: HashMap<String, f64> = items.iter().map(|item| (item.id.clone(), frecency.score(&item.id, now))).collect();
    let mut app = App::new(items, scores, status);

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app, client.as_ref().ok(), &ctx, &herdr_bin, &mut frecency, &state_dir);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    client: Option<&Client>,
    ctx: &PluginContext,
    herdr_bin: &str,
    frecency: &mut Frecency,
    state_dir: &Path,
) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| render::render(frame, app))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle_key(key, ctx) {
            Command::Continue => {}
            Command::Quit => return Ok(()),
            Command::Run { item, input } => {
                let Some(client) = client else {
                    app.fail("not connected to herdr".to_string());
                    continue;
                };
                let (id, action) = (app.items[item].id.clone(), app.items[item].action.clone());
                match exec::execute(client, &action, ctx, &input, herdr_bin) {
                    Ok(()) => {
                        let now = now_unix();
                        frecency.record(&id, now);
                        if let Err(err) = frecency.save(now) {
                            log::append(state_dir, &format!("saving frecency failed: {err}"));
                        }
                        return Ok(());
                    }
                    Err(err) => {
                        log::append(state_dir, &err);
                        app.fail(err);
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run the full suite and build**

Run: `cargo test --workspace && cargo build -p command-palette 2>&1 | grep -c warning || true`
Expected: all tests pass. There should be no `dead_code` warnings any more. If one remains, remove the unused item instead of silencing it.

Run: `just`
Expected: `[jacob.command-palette] linked plugins/command-palette`.

- [ ] **Step 5: Smoke-test the popup through herdr (no keybinding yet)**

Run: `herdr plugin action invoke jacob.command-palette.open`
Expected: the palette popup appears over the current pane and lists workspaces, tabs, agents, commands and the plugin actions. Esc closes it.

If it doesn't appear, run `herdr plugin log list --plugin jacob.command-palette` and `cat ~/.local/state/herdr/plugins/jacob.command-palette/palette.log` and fix the cause before continuing. Use superpowers:systematic-debugging.

Check visually whether herdr draws its own frame around the popup. If it does, the palette's layout is fine as is. Don't add a second border.

- [ ] **Step 6: Set up the `cmd+k` keybinding (ask the user before editing their configs)**

These are the user's global config files. Show the exact edits and get a yes before writing them.

Append to `~/.config/herdr/config.toml`:

```toml
# Command palette (jacob.command-palette plugin from ~/jacob/git/herdr-plugins).
[[keys.command]]
key = "cmd+k"
type = "plugin_action"
command = "jacob.command-palette.open"
description = "command palette"
```

Append to `~/.config/ghostty/config`:

```
# Let cmd+k reach herdr for the command palette (Ghostty binds it to clear_screen).
keybind = cmd+k=unbind
```

Run: `herdr server reload-config`. Then reload the Ghostty config with cmd+shift+, and press `cmd+k` inside herdr.
Expected: the palette opens.

If it doesn't open, fall back in this order, testing after each:
1. Ghostty `keybind = cmd+k=csi:27;9;107~` in place of the unbind. That's the modifyOtherKeys encoding with the super modifier, the same technique as the user's existing `ctrl+tab=csi:27;5;9~`.
2. Change the herdr binding to `key = "ctrl+alt+k"` and use Ghostty `keybind = cmd+k=text:\x1b\x0b`, which is the legacy ESC-prefixed ctrl+k. The `ctrl+alt` family is what herdr's keyboard docs recommend as reliably transmitted.

Record which variant worked in the README (Step 8).

- [ ] **Step 7: Exercise every item kind live**

With the palette open via `cmd+k`, check each of these and note any failure:

- [ ] Empty query hides the current workspace and tab. After Step 7's selections, frecently used items float to the top on the next open.
- [ ] Type `boa` to filter to boardwalk with highlighted letters. Enter focuses that workspace and the popup closes.
- [ ] Select a tab (`Workspace › Tab`) and it focuses.
- [ ] Select an agent row. It focuses that agent's pane, even in another workspace. If `agent.focus` doesn't switch workspaces, switch `FocusPane` in `exec.rs` to `tab.focus` on the agent's tab followed by `agent.focus`, and add a test for the two calls.
- [ ] "Split pane right" splits the pane that was under the popup.
- [ ] "Rename tab" pre-fills the current label. Edit it and press Enter, and the tab is renamed. Esc in the prompt goes back to the list with the query kept.
- [ ] "Close tab" asks for confirmation, and `n` cancels. Use a throwaway tab to test `y`.
- [ ] "Reload herdr config" succeeds.
- [ ] Link a second plugin that has an action, such as the probe from brainstorming or any installed plugin. Its action shows as `PLG` and runs after the palette closes.
- [ ] Create `~/.config/herdr/plugins/config/jacob.command-palette/commands.toml` with `[[commands]]\ntitle = "Touch marker"\nrun = "touch /tmp/palette-marker"`. It shows as `USR` and running it creates the file.
- [ ] Break `commands.toml` with `title =`. The footer shows `commands.toml line …` and the other items still load. Restore the file afterwards.
- [ ] Run `herdr tab close <id>` on a tab from another pane while the palette is open, then select that tab. The footer shows a red `tab.focus: …` error and the palette stays open.

- [ ] **Step 8: Document the palette in the README**

Append to `README.md`:

````markdown
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
````

- [ ] **Step 9: Commit**

```bash
git add plugins/command-palette/src README.md
git commit -m "feat(palette): wire ui entrypoint, logging, and setup docs

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

## Spec coverage check (for the executor's reference)

| Spec requirement | Task |
|---|---|
| Repo as multi-plugin home, cargo workspace, shared crate | 1, 2, 3 |
| `just` single build+link command, idempotent link, no edits for new plugins | 1 |
| Manifest with `open` action + popup pane | 1 |
| Socket client, typed models, context parsing, method-named errors | 2, 3 |
| Workspaces / tabs / agents / plugin actions / user commands / built-ins sources | 4, 5, 8 |
| Parallel loading; failing source → notice, palette still opens | 8, 12 |
| nucleo matching, smart case, fzf syntax, title highlights | 7, 11 |
| Frecency boost with cap, empty-query frecency order, hide current | 6, 7 |
| Frecency file, atomic write, 90-day prune, corrupt → empty + log | 6, 12 |
| Prompt and confirm steps | 4, 10, 11 |
| Keys: type, ↑↓, ctrl-p/n, enter, esc, ctrl-u, backspace | 10 |
| Action failure keeps palette open with red footer error | 9, 10, 11, 12 |
| Socket unreachable shows error, esc closes | 12 |
| User commands detached, `run`/`argv`, cwd default | 5, 9 |
| Plugin actions delayed to avoid `ui_busy` | 9 |
| `cmd+k` keybinding + Ghostty unbind, with verified fallback | 12 |
| Out of scope: mouse, preview, multi-select, themes, Windows, marketplace | — |
