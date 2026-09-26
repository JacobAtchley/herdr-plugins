mod app;
mod git;
mod log;
mod ops;
mod render;
mod status;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use herdr_client::PluginContext;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

use app::{App, Command};
use git::Repo;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("ui") => match run_ui() {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("git-glance: {err}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: git-glance ui");
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
        .unwrap_or_else(|| std::env::temp_dir().join("herdr-git-glance"));
    let dir = target_dir(&ctx);

    let opened = Repo::discover(&dir).and_then(|repo| repo.snapshot().map(|snapshot| (repo, snapshot)));
    let mut terminal = ratatui::init();
    let result = match opened {
        Ok((repo, snapshot)) => {
            let mut app = App::new(repo.name(), snapshot);
            event_loop(&mut terminal, &mut app, &repo, &state_dir)
        }
        Err(err) => {
            log::append(&state_dir, &format!("{}: {err}", dir.display()));
            message_screen(&mut terminal, &format!("{}: {err}", dir.display()))
        }
    };
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App, repo: &Repo, state_dir: &Path) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| {
            app.page = render::body_height(frame.area().height);
            render::render(frame, app);
        })?;
        let Event::Key(key) = event::read()? else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle_key(key) {
            Command::Continue => {}
            Command::Quit => return Ok(()),
            Command::Run(op) => match ops::run(repo, &op) {
                Ok((outcome, notice)) => app.apply(outcome, notice),
                Err(err) => {
                    log::append(state_dir, &format!("{op:?}: {err}"));
                    app.fail(err);
                }
            },
        }
    }
}

/// Shown when there's no repository to glance at; any key closes the popup.
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
        assert_eq!(target_dir(&PluginContext::default()), std::env::current_dir().unwrap());
    }
}
