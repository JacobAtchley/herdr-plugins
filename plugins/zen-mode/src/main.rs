mod activate;
mod app;
mod log;
mod render;
mod resolve;
mod state;
#[cfg(test)]
mod testing;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use herdr_client::{Api, Client, PluginContext};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use serde_json::json;

use activate::ToggleOutcome;
use app::{App, Command};
use state::ZenState;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("ui") => match run_ui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("zen-mode: {err}");
                ExitCode::FAILURE
            }
        },
        Some("toggle") => match run_toggle() {
            Ok(message) => {
                if !message.is_empty() {
                    eprintln!("{message}");
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("zen-mode: {err}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: zen-mode ui|toggle");
            ExitCode::from(2)
        }
    }
}

fn state_dir() -> PathBuf {
    std::env::var_os("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("herdr-zen-mode"))
}

fn run_toggle() -> Result<String, String> {
    let dir = state_dir();
    let mut state = ZenState::load(&dir)?;
    let ctx = PluginContext::from_env();
    let client = Client::from_env().map_err(|err| err.to_string())?;
    let outcome = activate::toggle(&client, &mut state, ctx.tab_id.as_deref())?;
    state.save(&dir)?;
    Ok(toggle_summary(&outcome))
}

fn toggle_summary(outcome: &ToggleOutcome) -> String {
    if outcome.active {
        format!(
            "zen on · {} tabs available · {} missing",
            outcome.resolution.available_tab_ids.len(),
            outcome.resolution.missing.len()
        )
    } else {
        "zen off".into()
    }
}

fn run_ui() -> std::io::Result<()> {
    let dir = state_dir();
    let ctx = PluginContext::from_env();
    let client = match Client::from_env() {
        Ok(client) => client,
        Err(err) => {
            log::append(&dir, &err.to_string());
            return message_screen_err(&err.to_string());
        }
    };

    let loaded = load_app(&client, &dir);
    let mut terminal = ratatui::init();
    let result = match loaded {
        Ok(mut app) => event_loop(&mut terminal, &mut app, &client, &dir),
        Err(err) => {
            log::append(&dir, &err);
            let _ = ctx;
            message_screen(&mut terminal, &err)
        }
    };
    ratatui::restore();
    result
}

fn load_app(api: &impl Api, dir: &Path) -> Result<App, String> {
    let state = ZenState::load(dir)?;
    let workspaces = api.workspace_list().map_err(|err| err.to_string())?;
    let tabs = api.tab_list().map_err(|err| err.to_string())?;
    Ok(App::new(state, workspaces, tabs))
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App, api: &impl Api, state_dir: &Path) -> std::io::Result<()> {
    loop {
        if app.take_dirty()
            && let Err(err) = app.state.save(state_dir)
        {
            log::append(state_dir, &err);
            app.status = Some(err);
        }
        terminal.draw(|frame| render::render(frame, app))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle_key(key) {
            Command::Continue => {}
            Command::Quit => {
                if let Err(err) = app.state.save(state_dir) {
                    log::append(state_dir, &err);
                }
                return Ok(());
            }
            Command::FocusTab { tab_id } => {
                if let Err(err) = app.state.save(state_dir) {
                    log::append(state_dir, &err);
                }
                match api.request("tab.focus", json!({ "tab_id": tab_id })) {
                    Ok(_) => return Ok(()),
                    Err(err) => {
                        log::append(state_dir, &err.to_string());
                        app.status = Some(err.to_string());
                    }
                }
            }
            Command::FocusWorkspace { workspace_id } => {
                if let Err(err) = app.state.save(state_dir) {
                    log::append(state_dir, &err);
                }
                match api.request("workspace.focus", json!({ "workspace_id": workspace_id })) {
                    Ok(_) => return Ok(()),
                    Err(err) => {
                        log::append(state_dir, &err.to_string());
                        app.status = Some(err.to_string());
                    }
                }
            }
        }
    }
}

fn message_screen_err(message: &str) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = message_screen(&mut terminal, message);
    ratatui::restore();
    result
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
    use crate::resolve::Resolution;

    #[test]
    fn toggle_summary_reports_counts() {
        let on = ToggleOutcome {
            active: true,
            resolution: Resolution {
                available_tab_ids: vec!["w1:t1".into()],
                missing: vec!["tab w1:t9 unavailable".into()],
                focus_tab_id: Some("w1:t1".into()),
            },
        };
        assert_eq!(toggle_summary(&on), "zen on · 1 tabs available · 1 missing");
        assert_eq!(
            toggle_summary(&ToggleOutcome {
                active: false,
                resolution: Resolution { available_tab_ids: Vec::new(), missing: Vec::new(), focus_tab_id: None },
            }),
            "zen off"
        );
    }
}
