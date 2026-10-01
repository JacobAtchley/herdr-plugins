//! Draws the zen configure popup.

use herdr_client::models::AgentStatus;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app::{App, Mode, Row, tab_label, workspace_label};
use crate::state::ZenState;

pub fn render(frame: &mut Frame, app: &App) {
    let [header, rule, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(frame.area());
    render_header(frame, app, header);
    frame.render_widget(Line::from("─".repeat(usize::from(rule.width))).dim(), rule);
    match app.mode {
        Mode::Overview => render_overview(frame, app, body),
        Mode::PickWorkspace { .. } => render_workspace_picker(frame, app, body),
        Mode::PickTab { .. } => render_tab_picker(frame, app, body),
    }
    render_footer(frame, app, footer);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let active = if app.state.active { "on" } else { "off" };
    let title = match app.mode {
        Mode::Overview => format!("Zen Mode · {active}"),
        Mode::PickWorkspace { .. } => format!("Pick workspace · filter: {}", app.query),
        Mode::PickTab { .. } => format!("Pick tab · filter: {}", app.query),
    };
    frame.render_widget(Line::from(title).cyan().bold(), area);
}

fn render_overview(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    if app.rows.is_empty() {
        frame.render_widget(Line::from("  no rows").dim(), area);
        return;
    }
    let height = usize::from(area.height);
    let offset = app.selected.saturating_sub(height.saturating_sub(1));
    for (row, (position, item)) in app.rows.iter().enumerate().skip(offset).take(height).enumerate() {
        let selected = position == app.selected;
        let line = overview_line(&app.state, &app.workspaces, &app.tabs, item, selected);
        frame.render_widget(line, Rect { y: area.y + row as u16, height: 1, ..area });
    }
}

fn overview_line(
    state: &ZenState,
    workspaces: &[herdr_client::models::Workspace],
    tabs: &[herdr_client::models::Tab],
    row: &Row,
    selected: bool,
) -> Line<'static> {
    let marker = if selected { "▸ " } else { "  " };
    let spans = match row {
        Row::Workspace { index } => {
            let slot = &state.workspaces[*index];
            let workspace = workspaces.iter().find(|workspace| workspace.workspace_id == slot.workspace_id);
            let label = workspace.map(workspace_label).unwrap_or_else(|| slot.workspace_id.clone());
            let mut spans = vec![Span::raw(format!("{marker}Workspace {label}"))];
            if let Some(workspace) = workspace {
                push_status(&mut spans, workspace.agent_status);
            }
            spans
        }
        Row::Tab { workspace_index, tab_index } => {
            let tab_id = &state.workspaces[*workspace_index].tab_ids[*tab_index];
            let tab = tabs.iter().find(|tab| tab.tab_id == *tab_id);
            let label = tab.map(tab_label).unwrap_or_else(|| tab_id.clone());
            let mut spans = vec![Span::raw(format!("{marker}  Tab {label}"))];
            if let Some(tab) = tab {
                push_status(&mut spans, tab.agent_status);
            }
            spans
        }
        Row::AddWorkspace => vec![Span::raw(format!("{marker}+ Add workspace"))],
        Row::AddTab { .. } => vec![Span::raw(format!("{marker}  + Add tab"))],
    };
    let mut line = Line::from(spans);
    if selected {
        line = line.style(Style::new().bg(Color::DarkGray));
    }
    line
}

fn render_workspace_picker(frame: &mut Frame, app: &App, area: Rect) {
    render_picker_list(frame, area, app.selected, app.picker_indexes.len(), |position| {
        let workspace = &app.workspaces[app.picker_indexes[position]];
        let mut spans = vec![Span::raw(format!("{} {}", workspace_label(workspace), workspace.workspace_id))];
        push_status(&mut spans, workspace.agent_status);
        spans
    });
}

fn render_tab_picker(frame: &mut Frame, app: &App, area: Rect) {
    render_picker_list(frame, area, app.selected, app.picker_indexes.len(), |position| {
        let tab = &app.tabs[app.picker_indexes[position]];
        let mut spans = vec![Span::raw(format!("{} {}", tab_label(tab), tab.tab_id))];
        push_status(&mut spans, tab.agent_status);
        spans
    });
}

fn render_picker_list(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    len: usize,
    label: impl Fn(usize) -> Vec<Span<'static>>,
) {
    if area.height == 0 {
        return;
    }
    if len == 0 {
        frame.render_widget(Line::from("  no matches").dim(), area);
        return;
    }
    let height = usize::from(area.height);
    let offset = selected.saturating_sub(height.saturating_sub(1));
    for (row, position) in (offset..len).take(height).enumerate() {
        let is_selected = position == selected;
        let marker = if is_selected { "▸ " } else { "  " };
        let mut spans = vec![Span::raw(marker)];
        spans.extend(label(position));
        let mut line = Line::from(spans);
        if is_selected {
            line = line.style(Style::new().bg(Color::DarkGray));
        }
        frame.render_widget(line, Rect { y: area.y + row as u16, height: 1, ..area });
    }
}

fn push_status(spans: &mut Vec<Span<'static>>, status: AgentStatus) {
    if status == AgentStatus::Unknown {
        return;
    }
    spans.push(Span::styled(format!("  ● {}", status.as_str()), status_style(status)));
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
    let hints = match app.mode {
        Mode::Overview => "⏎ go  z toggle  r replace  a add ws  t add tab  d remove  esc",
        Mode::PickWorkspace { .. } | Mode::PickTab { .. } => "type to filter  ⏎ select  esc back",
    };
    let left = Line::from(hints).dim();
    let right = match &app.status {
        Some(message) => Line::from(Span::raw(message.clone()).yellow()),
        None => {
            Line::from(format!("{} ws · {} tabs max", app.state.workspaces.len(), crate::state::MAX_TABS_PER_WORKSPACE))
                .dim()
        }
    };
    let hints_width = u16::try_from(left.width() + 2).unwrap_or(u16::MAX);
    let [left_area, right_area] = Layout::horizontal([Constraint::Length(hints_width), Constraint::Min(0)]).areas(area);
    frame.render_widget(left, left_area);
    frame.render_widget(right.right_aligned(), right_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WorkspaceSlot;
    use herdr_client::models::{AgentStatus, Tab, Workspace};
    use ratatui::{Terminal, backend::TestBackend};

    fn workspace(id: &str, number: u32, label: &str) -> Workspace {
        Workspace {
            workspace_id: id.into(),
            number,
            label: label.into(),
            focused: false,
            agent_status: AgentStatus::Unknown,
        }
    }

    fn tab(id: &str, workspace_id: &str, number: u32, label: &str, agent_status: AgentStatus) -> Tab {
        Tab {
            tab_id: id.into(),
            workspace_id: workspace_id.into(),
            number,
            label: label.into(),
            focused: false,
            agent_status,
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

    #[test]
    fn overview_lists_grouped_selection() {
        let app = App::new(
            ZenState {
                active: true,
                workspaces: vec![WorkspaceSlot { workspace_id: "w1".into(), tab_ids: vec!["w1:t1".into()] }],
            },
            vec![workspace("w1", 1, "Core")],
            vec![tab("w1:t1", "w1", 1, "Claude", AgentStatus::Unknown)],
        );
        let lines = draw(&app, 70, 8);
        assert!(lines[0].contains("Zen Mode · on"));
        assert!(lines.iter().any(|line| line.contains("Workspace Core")));
        assert!(lines.iter().any(|line| line.contains("Tab Claude")));
    }

    #[test]
    fn overview_shows_tab_agent_status() {
        let app = App::new(
            ZenState {
                active: false,
                workspaces: vec![WorkspaceSlot {
                    workspace_id: "w1".into(),
                    tab_ids: vec!["w1:t1".into(), "w1:t2".into()],
                }],
            },
            vec![workspace("w1", 1, "Core")],
            vec![
                tab("w1:t1", "w1", 1, "Claude", AgentStatus::Working),
                tab("w1:t2", "w1", 2, "Idle", AgentStatus::Unknown),
            ],
        );
        let lines = draw(&app, 70, 8);
        assert!(lines.iter().any(|line| line.contains("Tab Claude") && line.contains("● working")));
        assert!(lines.iter().any(|line| line.contains("Tab Idle") && !line.contains("●")));
    }
}
