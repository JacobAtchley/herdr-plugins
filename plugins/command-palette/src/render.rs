//! Draws the palette. Uses the terminal's own colors plus ANSI accents so it
//! follows the user's herdr/terminal theme.

use herdr_client::models::AgentStatus;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

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
    spans.extend(item.title.graphemes(true).enumerate().map(|(i, g)| {
        let span = Span::raw(g.to_string());
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

    #[test]
    fn zwj_and_variation_selector_emoji_render_as_one_cell_symbol() {
        let items = vec![
            Item::new(Kind::Command, "cmd:dev", "👨‍💻 dev", Action::FocusTab("x".into())),
            Item::new(Kind::Command, "cmd:prod", "⚠️ prod", Action::FocusTab("y".into())),
        ];
        let app = App::new(items, HashMap::new(), None);
        let lines = draw(&app, 40, 6);
        let rows = lines[2..4].join("\n");
        assert!(rows.contains("👨‍💻"), "{rows:?}");
        assert!(rows.contains("⚠️"), "{rows:?}");
    }
}

