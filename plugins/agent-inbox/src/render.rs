//! Draws the inbox popup.

use herdr_client::models::AgentStatus;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app::{App, Mode};
use crate::inbox::Entry;

pub fn render(frame: &mut Frame, app: &App) {
    let [header, rule, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(frame.area());
    render_header(frame, app, header);
    frame.render_widget(Line::from("─".repeat(usize::from(rule.width))).dim(), rule);
    render_body(frame, app, body);
    render_footer(frame, app, footer);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Line::from("Agent Inbox").cyan().bold(), area);
    let count = match app.inbox.needs.len() {
        0 => "all quiet".to_string(),
        n => format!("{n} need you"),
    };
    frame.render_widget(Line::from(count).dim().right_aligned(), area);
}

/// One rendered row, tagged with the inbox row it belongs to.
struct Row {
    line: Line<'static>,
    item: Option<usize>,
}

fn render_body(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let rows = layout_rows(app, usize::from(area.width));
    let height = usize::from(area.height);
    let first = rows.iter().position(|row| row.item == Some(app.selected)).unwrap_or(0);
    let last = rows.iter().rposition(|row| row.item == Some(app.selected)).unwrap_or(0);
    // Scroll just enough to keep every line of the selected agent in view.
    let offset = (last + 1).saturating_sub(height).min(first);
    for (y, row) in rows.into_iter().skip(offset).take(height).enumerate() {
        frame.render_widget(row.line, Rect { y: area.y + y as u16, height: 1, ..area });
    }
}

fn layout_rows(app: &App, width: usize) -> Vec<Row> {
    let inbox = &app.inbox;
    let agent_width = inbox.needs.iter().chain(&inbox.working).map(|e| e.agent.chars().count()).max().unwrap_or(0);
    let mut rows = Vec::new();
    if inbox.needs.is_empty() {
        rows.push(Row { line: Line::from("  No agents need you.").dim(), item: None });
    }
    for (index, entry) in inbox.needs.iter().enumerate() {
        if index > 0 {
            rows.push(Row { line: Line::from("┄".repeat(width)).dark_gray(), item: None });
        }
        let selected = index == app.selected;
        rows.push(Row { line: highlight(entry_line(entry, agent_width, selected), selected), item: Some(index) });
        let preview = entry.preview.as_deref().or(entry.title.as_deref()).unwrap_or("");
        let preview = Line::from(format!("    {}", quote(preview))).dim();
        rows.push(Row { line: highlight(preview, selected), item: Some(index) });
    }
    if !inbox.working.is_empty() {
        rows.push(Row { line: Line::from(""), item: None });
        let label = format!("── working ({}) ", inbox.working.len());
        let fill = width.saturating_sub(label.chars().count());
        rows.push(Row { line: Line::from(format!("{label}{}", "─".repeat(fill))).dim(), item: None });
        for (offset, entry) in inbox.working.iter().enumerate() {
            let index = inbox.needs.len() + offset;
            let selected = index == app.selected;
            rows.push(Row { line: highlight(entry_line(entry, agent_width, selected), selected), item: Some(index) });
        }
    }
    rows
}

fn entry_line(entry: &Entry, agent_width: usize, selected: bool) -> Line<'static> {
    let marker = if selected { "▸ " } else { "  " };
    Line::from(vec![
        Span::raw(marker),
        Span::styled(format!("{:<9}", entry.status.as_str()), status_style(entry.status)),
        Span::raw(format!("{:<width$}  ", entry.agent, width = agent_width)),
        Span::raw(entry.location.clone()),
    ])
}

fn quote(text: &str) -> String {
    if text.is_empty() { String::new() } else { format!("\"{text}\"") }
}

fn highlight(line: Line<'static>, selected: bool) -> Line<'static> {
    if selected { line.style(Style::new().bg(Color::DarkGray)) } else { line }
}

fn status_style(status: AgentStatus) -> Style {
    Style::new().fg(match status {
        AgentStatus::Working => Color::Yellow,
        AgentStatus::Blocked => Color::Red,
        AgentStatus::Done => Color::Green,
        AgentStatus::Idle => Color::Blue,
        AgentStatus::Unknown => Color::DarkGray,
    })
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if let Mode::Reply { input, .. } = &app.mode {
        let line =
            Line::from(vec![Span::raw("reply › ").cyan(), Span::raw(format!("{input}▏")), "  ⏎ send  esc".dim()]);
        frame.render_widget(line, area);
        return;
    }
    let hints = match app.selected_status() {
        Some(AgentStatus::Blocked) => "⏎ jump  y yes  n no  esc",
        Some(AgentStatus::Done) => "⏎ jump  c continue  r reply  esc",
        Some(_) => "⏎ jump  esc",
        None => "esc close",
    };
    let left = Line::from(hints).dim();
    let hints_width = u16::try_from(left.width() + 2).unwrap_or(u16::MAX);
    let [left_area, right_area] = Layout::horizontal([Constraint::Length(hints_width), Constraint::Min(0)]).areas(area);
    frame.render_widget(left, left_area);
    if let Some(message) = &app.status {
        frame.render_widget(Line::from(message.clone()).yellow().right_aligned(), right_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::Inbox;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    fn entry(pane: &str, agent: &str, status: AgentStatus, preview: Option<&str>) -> Entry {
        Entry {
            pane_id: pane.into(),
            agent: agent.into(),
            status,
            location: format!("api › {pane}"),
            seq: 0,
            title: Some("session title".into()),
            preview: preview.map(String::from),
        }
    }

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
        App::new(Inbox {
            needs: vec![
                entry("t1", "claude", AgentStatus::Blocked, Some("Allow edit?")),
                entry("t2", "codex", AgentStatus::Done, None),
            ],
            working: vec![entry("t3", "claude", AgentStatus::Working, None)],
        })
    }

    #[test]
    fn needs_rows_have_previews_and_dotted_separators() {
        let lines = draw(&sample(), 50, 14);
        assert!(lines[0].starts_with("Agent Inbox") && lines[0].ends_with("2 need you"));
        assert!(lines[2].starts_with("▸ blocked  claude  api › t1"));
        assert_eq!(lines[3], "    \"Allow edit?\"");
        assert!(lines[4].starts_with("┄┄┄"));
        assert!(lines[5].starts_with("  done     codex   api › t2"));
        assert_eq!(lines[6], "    \"session title\"", "falls back to the title");
        assert_eq!(lines[7], "", "blank row above working");
        assert!(lines[8].starts_with("── working (1) ──"));
        assert!(lines[9].starts_with("  working  claude  api › t3"));
        assert!(lines[13].contains("y yes  n no"));
    }

    #[test]
    fn footer_follows_the_selected_state() {
        let mut app = sample();
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(draw(&app, 50, 14)[13].contains("c continue  r reply"));
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert!(draw(&app, 50, 14)[13].starts_with("reply › o▏"));
    }

    #[test]
    fn empty_inbox_says_all_quiet() {
        let lines = draw(&App::new(Inbox::default()), 40, 6);
        assert!(lines[0].ends_with("all quiet"));
        assert_eq!(lines[2], "  No agents need you.");
    }

    #[test]
    fn scrolls_to_keep_the_selected_agent_visible() {
        let mut app = App::new(Inbox {
            needs: (1..=5).map(|n| entry(&format!("t{n}"), "claude", AgentStatus::Done, Some("ok"))).collect(),
            working: Vec::new(),
        });
        for _ in 0..4 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        let lines = draw(&app, 40, 8);
        assert!(lines.iter().any(|line| line.starts_with("▸ done") && line.contains("t5")));
        assert!(lines.iter().any(|line| line.contains("\"ok\"")));
    }
}
