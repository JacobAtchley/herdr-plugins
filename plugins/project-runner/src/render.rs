//! Draws the runner. Uses the terminal's own colors plus ANSI accents so it
//! follows the user's herdr/terminal theme.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

use crate::app::App;

/// Long script names don't push every command off screen.
const MAX_NAME_WIDTH: usize = 28;

pub fn render(frame: &mut Frame, app: &App) {
    let [input, rule, list, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(frame.area());
    render_input(frame, app, input);
    frame.render_widget(Line::from("─".repeat(usize::from(rule.width))).dim(), rule);
    render_list(frame, app, list);
    render_footer(frame, app, footer);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let line = Line::from(vec![Span::raw("❯ ").cyan().bold(), Span::raw(app.query.as_str())]);
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
    let scripts = &app.project.scripts;
    if app.ranked.is_empty() {
        let message = if scripts.is_empty() { "  no scripts in package.json" } else { "  no matches" };
        frame.render_widget(Line::from(message).dim(), area);
        return;
    }
    let name_width = scripts.iter().map(|s| s.name.graphemes(true).count()).max().unwrap_or(0).min(MAX_NAME_WIDTH);
    let height = usize::from(area.height);
    let offset = app.selected.saturating_sub(height - 1);
    for (row, (position, ranked)) in app.ranked.iter().enumerate().skip(offset).take(height).enumerate() {
        let selected = position == app.selected;
        let script = &scripts[ranked.index];
        let mut spans = vec![Span::raw(if selected { "▸ " } else { "  " })];
        let graphemes = script.name.graphemes(true).count();
        spans.extend(script.name.graphemes(true).enumerate().map(|(i, g)| {
            let span = Span::raw(g);
            if ranked.highlights.contains(&i) { span.yellow().bold() } else { span }
        }));
        spans.push(Span::raw(" ".repeat(name_width.saturating_sub(graphemes) + 2)));
        spans.push(Span::raw(script.command.as_str()).dim());
        let mut line = Line::from(spans);
        if selected {
            line = line.style(Style::new().bg(Color::DarkGray));
        }
        frame.render_widget(line, Rect { y: area.y + row as u16, height: 1, ..area });
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints = Line::from("⏎ tab  ^v split right  ^x split down  esc close").dim();
    let right = match &app.error {
        Some(message) => Line::from(message.as_str()).red(),
        None => Line::from(format!("{} · {}", app.project.name, app.project.manager.as_str())).dim(),
    };
    let hints_width = u16::try_from(hints.width() + 2).unwrap_or(u16::MAX);
    let [left_area, right_area] = Layout::horizontal([Constraint::Length(hints_width), Constraint::Min(0)]).areas(area);
    frame.render_widget(hints, left_area);
    frame.render_widget(right.right_aligned(), right_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::project;
    use ratatui::{Terminal, backend::TestBackend};

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

    #[test]
    fn lists_scripts_with_aligned_commands() {
        let lines = draw(&App::new(project()), 80, 7);
        assert_eq!(lines[0], "❯");
        assert!(lines[1].starts_with("────"), "{:?}", lines[1]);
        assert_eq!(lines[2], "▸ start:web  nx serve web");
        assert_eq!(lines[3], "  start:api  nx serve api");
        assert_eq!(lines[4], "  lint       nx run-many -t lint");
        assert!(lines[6].starts_with("⏎ tab  ^v split right"), "{:?}", lines[6]);
        assert!(lines[6].ends_with("mono · pnpm"), "{:?}", lines[6]);
    }

    #[test]
    fn empty_states() {
        let mut app = App::new(project());
        app.query = "zzz".into();
        app.ranked.clear();
        assert_eq!(draw(&app, 80, 7)[2], "  no matches");
        app.project.scripts.clear();
        assert_eq!(draw(&app, 80, 7)[2], "  no scripts in package.json");
    }

    #[test]
    fn error_replaces_project_label() {
        let mut app = App::new(project());
        app.fail("tab.create: gone (workspace_not_found)".into());
        assert!(draw(&app, 100, 7)[6].ends_with("tab.create: gone (workspace_not_found)"));
    }

    #[test]
    fn selection_scrolls_into_view() {
        let mut app = App::new(project());
        app.selected = 2;
        let lines = draw(&app, 80, 5);
        assert_eq!(lines[3], "▸ lint       nx run-many -t lint");
    }
}
