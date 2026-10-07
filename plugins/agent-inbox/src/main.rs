mod app;
mod inbox;
mod log;
mod render;
#[cfg(test)]
mod testing;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use herdr_client::{Api, Client};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use serde_json::json;

use app::{App, Command};
use inbox::Inbox;

/// How often the open inbox re-reads agent state from herdr.
const REFRESH: Duration = Duration::from_secs(1);

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("ui") => match run_ui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("agent-inbox: {err}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: agent-inbox ui");
            ExitCode::from(2)
        }
    }
}

fn state_dir() -> PathBuf {
    std::env::var_os("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("herdr-agent-inbox"))
}

/// Previews keyed by pane and state, so an agent's pane is read once per state change.
type Previews = HashMap<(String, u64), Option<String>>;

fn load(api: &impl Api, previews: &mut Previews) -> Result<Inbox, String> {
    let agents = api.agent_list().map_err(|err| err.to_string())?;
    let workspaces = api.workspace_list().map_err(|err| err.to_string())?;
    let tabs = api.tab_list().map_err(|err| err.to_string())?;
    let mut inbox = Inbox::build(&agents, &workspaces, &tabs);
    previews.retain(|(pane_id, seq), _| inbox.needs.iter().any(|e| e.pane_id == *pane_id && e.seq == *seq));
    for entry in &mut inbox.needs {
        entry.preview = previews
            .entry((entry.pane_id.clone(), entry.seq))
            .or_insert_with(|| read_snapshot(api, &entry.pane_id).and_then(|text| inbox::preview(entry.status, &text)))
            .clone();
    }
    Ok(inbox)
}

/// A failed read only costs the preview line, so it is not reported.
fn read_snapshot(api: &impl Api, pane_id: &str) -> Option<String> {
    let result = api.request("agent.read", json!({ "target": pane_id, "source": "detection" })).ok()?;
    result["read"]["text"].as_str().map(String::from)
}

fn run_ui() -> std::io::Result<()> {
    let dir = state_dir();
    let mut previews = Previews::new();
    let loaded = Client::from_env().map_err(|err| err.to_string()).and_then(|client| {
        let inbox = load(&client, &mut previews)?;
        Ok((client, inbox))
    });

    let mut terminal = ratatui::init();
    let result = match loaded {
        Ok((client, inbox)) => event_loop(&mut terminal, &mut App::new(inbox), &client, &mut previews, &dir),
        Err(err) => {
            log::append(&dir, &err);
            message_screen(&mut terminal, &err)
        }
    };
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    api: &impl Api,
    previews: &mut Previews,
    dir: &Path,
) -> std::io::Result<()> {
    let mut last_refresh = Instant::now();
    loop {
        if last_refresh.elapsed() >= REFRESH {
            last_refresh = Instant::now();
            match load(api, previews) {
                Ok(inbox) => app.set_inbox(inbox),
                Err(err) => {
                    log::append(dir, &err);
                    app.status = Some(err);
                }
            }
        }
        terminal.draw(|frame| render::render(frame, app))?;
        if !event::poll(REFRESH.saturating_sub(last_refresh.elapsed()))? {
            continue;
        }
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle_key(key) {
            Command::Continue => {}
            Command::Quit => return Ok(()),
            Command::Focus { pane_id } => match api.request("agent.focus", json!({ "target": pane_id })) {
                Ok(_) => return Ok(()),
                Err(err) => fail(app, dir, err.to_string()),
            },
            Command::Keys { pane_id, keys } => {
                match api.request("agent.send_keys", json!({ "target": pane_id, "keys": keys })) {
                    Ok(_) => app.answered(&pane_id),
                    Err(err) => fail(app, dir, err.to_string()),
                }
            }
            Command::Prompt { pane_id, text } => {
                match api.request("agent.prompt", json!({ "target": pane_id, "text": text })) {
                    Ok(_) => app.answered(&pane_id),
                    Err(err) => fail(app, dir, err.to_string()),
                }
            }
        }
    }
}

fn fail(app: &mut App, dir: &Path, message: String) {
    log::append(dir, &message);
    app.status = Some(message);
}

fn message_screen(terminal: &mut DefaultTerminal, message: &str) -> std::io::Result<()> {
    use ratatui::text::{Line, Text};
    use ratatui::widgets::{Paragraph, Wrap};
    loop {
        terminal.draw(|frame| {
            let text = Text::from(vec![Line::from(message), Line::from(""), Line::from("press any key")]);
            frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), frame.area());
        })?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeApi;

    fn api() -> FakeApi {
        FakeApi::new()
            .ok(
                "agent.list",
                json!({ "agents": [
                    { "pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1", "agent": "claude",
                      "agent_status": "blocked", "state_change_seq": 4, "terminal_title_stripped": "Fix auth" },
                    { "pane_id": "w1:p2", "workspace_id": "w1", "tab_id": "w1:t1", "agent": "claude",
                      "agent_status": "idle", "state_change_seq": 2 },
                ]}),
            )
            .ok("workspace.list", json!({ "workspaces": [{ "workspace_id": "w1", "number": 1, "label": "api" }] }))
            .ok(
                "tab.list",
                json!({ "tabs": [{ "tab_id": "w1:t1", "workspace_id": "w1", "number": 1, "label": "dev" }] }),
            )
            .ok("agent.read", json!({ "read": { "text": "│ Run npm install? │\n│ ❯ 1. Yes │\n" } }))
    }

    #[test]
    fn load_builds_rows_with_previews_read_once_per_state() {
        let api = api();
        let mut previews = Previews::new();
        let inbox = load(&api, &mut previews).unwrap();
        assert_eq!(inbox.needs.len(), 1);
        assert_eq!(inbox.needs[0].location, "api › dev");
        assert_eq!(inbox.needs[0].title.as_deref(), Some("Fix auth"));
        assert_eq!(inbox.needs[0].preview.as_deref(), Some("Run npm install?"));

        load(&api, &mut previews).unwrap();
        let reads = api.calls().iter().filter(|(method, _)| method == "agent.read").count();
        assert_eq!(reads, 1);
        let (_, params) = api.calls().into_iter().find(|(method, _)| method == "agent.read").unwrap();
        assert_eq!(params, json!({ "target": "w1:p1", "source": "detection" }));
    }

    #[test]
    fn failed_read_leaves_the_preview_empty() {
        let api = api().err("agent.read", "agent_not_found", "gone");
        let inbox = load(&api, &mut Previews::new()).unwrap();
        assert_eq!(inbox.needs[0].preview, None);
    }

    #[test]
    fn list_failure_is_an_error() {
        let api = api().err("agent.list", "server_error", "boom");
        assert!(load(&api, &mut Previews::new()).unwrap_err().contains("boom"));
    }
}
