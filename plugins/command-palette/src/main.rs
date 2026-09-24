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
                match exec::execute(client, &action, ctx, &input, herdr_bin, state_dir) {
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
