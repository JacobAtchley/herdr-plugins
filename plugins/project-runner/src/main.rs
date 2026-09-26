mod app;
mod launch;
mod log;
mod matcher;
mod project;
mod render;
#[cfg(test)]
mod testing;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use herdr_client::{Client, PluginContext};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

use app::{App, Command};
use project::Project;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("ui") => match run_ui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("project-runner: {err}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: project-runner ui");
            ExitCode::from(2)
        }
    }
}

/// The focused pane's directory is the one the user means; the workspace
/// root and the process cwd are fallbacks for panes that don't report one.
fn target_dir(ctx: &PluginContext) -> PathBuf {
    ctx.focused_pane_cwd
        .as_deref()
        .or(ctx.workspace_cwd.as_deref())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn run_ui() -> std::io::Result<()> {
    let ctx = PluginContext::from_env();
    let state_dir = std::env::var_os("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("herdr-project-runner"));
    let discovered = Project::discover(&target_dir(&ctx));
    let mut terminal = ratatui::init();
    let result = match discovered {
        Ok(project) => event_loop(&mut terminal, &mut App::new(project), &ctx, &state_dir),
        Err(err) => {
            log::append(&state_dir, &err);
            message_screen(&mut terminal, &err)
        }
    };
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    ctx: &PluginContext,
    state_dir: &Path,
) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| render::render(frame, app))?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle_key(key) {
            Command::Continue => {}
            Command::Quit => return Ok(()),
            Command::Run { script, target } => {
                let script = &app.project.scripts[script];
                let line = app.project.manager.run_line(&script.name);
                let result = Client::from_env()
                    .map_err(|e| e.to_string())
                    .and_then(|client| launch::launch(&client, ctx, target, &app.project.dir, &script.name, &line));
                match result {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        log::append(state_dir, &format!("{line}: {err}"));
                        app.fail(err);
                    }
                }
            }
        }
    }
}

/// Shown when there's no package.json to run from; any key closes the popup.
fn message_screen(terminal: &mut DefaultTerminal, message: &str) -> std::io::Result<()> {
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

    #[test]
    fn target_dir_prefers_the_focused_pane() {
        let ctx = PluginContext {
            focused_pane_cwd: Some("/pane".into()),
            workspace_cwd: Some("/ws".into()),
            ..Default::default()
        };
        assert_eq!(target_dir(&ctx), PathBuf::from("/pane"));
        let ctx = PluginContext { workspace_cwd: Some("/ws".into()), ..Default::default() };
        assert_eq!(target_dir(&ctx), PathBuf::from("/ws"));
    }
}
