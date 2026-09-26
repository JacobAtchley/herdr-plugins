//! Draws the glance. Uses the terminal's own colors plus ANSI accents so it
//! follows the user's herdr/terminal theme.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};

use crate::app::{App, Mode, Purpose, Status, View};
use crate::git::{Branch, Stash};
use crate::status::{BranchInfo, Entry, Section};

pub fn render(frame: &mut Frame, app: &App) {
    let [header, rule, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(frame.area());
    render_header(frame, app, header);
    frame.render_widget(Line::from("─".repeat(usize::from(rule.width))).dim(), rule);
    match &app.view {
        View::Status => render_status(frame, app, body),
        View::Diff { lines, scroll, .. } => render_diff(frame, lines, *scroll, body),
        View::Branches { branches, matches, selected, .. } => {
            render_branches(frame, branches, matches, *selected, body)
        }
        View::Stashes { stashes, selected } => render_stashes(frame, stashes, *selected, body),
    }
    render_footer(frame, app, footer);
}

/// Body rows for a frame of the given height: everything but header, rule and footer.
pub fn body_height(frame_height: u16) -> usize {
    usize::from(frame_height.saturating_sub(3))
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let line = match (&app.mode, &app.view) {
        (Mode::Prompt { purpose, input }, _) => {
            let label = match purpose {
                Purpose::Commit => "Commit message › ",
                Purpose::Stash => "Stash message (optional) › ",
            };
            let line = Line::from(vec![Span::raw(label).cyan().bold(), Span::raw(input.as_str())]);
            set_cursor_after(frame, &line, area);
            line
        }
        (Mode::Confirm { question, .. }, _) => Line::from(question.as_str()).bold(),
        (Mode::Normal, View::Branches { query, .. }) => {
            let line = Line::from(vec![Span::raw("Switch branch › ").cyan().bold(), Span::raw(query.as_str())]);
            set_cursor_after(frame, &line, area);
            line
        }
        (Mode::Normal, View::Diff { title, .. }) => {
            Line::from(vec![Span::raw("Diff ").cyan().bold(), Span::raw(title.as_str())])
        }
        (Mode::Normal, _) => branch_line(&app.repo_name, &app.snapshot.branch, app.snapshot.stashes),
    };
    frame.render_widget(line, area);
}

fn set_cursor_after(frame: &mut Frame, line: &Line, area: Rect) {
    if area.width > 0 && area.height > 0 {
        let width = u16::try_from(line.width()).unwrap_or(u16::MAX);
        frame.set_cursor_position((area.x.saturating_add(width).min(area.right().saturating_sub(1)), area.y));
    }
}

fn branch_line<'a>(repo: &'a str, branch: &'a BranchInfo, stashes: usize) -> Line<'a> {
    let mut spans = vec![Span::raw(repo).bold(), Span::raw("  ")];
    match (&branch.head, &branch.oid) {
        (Some(head), _) => spans.push(Span::raw(format!("⎇ {head}")).magenta().bold()),
        (None, Some(oid)) => spans.push(Span::raw(format!("detached @ {oid}")).red().bold()),
        (None, None) => spans.push(Span::raw("detached").red().bold()),
    }
    if branch.oid.is_none() && branch.head.is_some() {
        spans.push(Span::raw("  no commits yet").dim());
    }
    if let Some(upstream) = &branch.upstream {
        spans.push(Span::raw(format!(" → {upstream}")).dim());
        if branch.ahead > 0 {
            spans.push(Span::raw(format!(" ↑{}", branch.ahead)).green());
        }
        if branch.behind > 0 {
            spans.push(Span::raw(format!(" ↓{}", branch.behind)).yellow());
        }
    }
    if stashes > 0 {
        spans.push(Span::raw(format!("  ≡ {stashes} stash{}", if stashes == 1 { "" } else { "es" })).dim());
    }
    Line::from(spans)
}

/// A row in the status body: a section heading or an entry.
enum Row<'a> {
    Heading(Section, usize),
    Entry(usize, &'a Entry),
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let entries = &app.snapshot.entries;
    if entries.is_empty() {
        frame.render_widget(Line::from("  nothing to commit, working tree clean").dim(), area);
        return;
    }
    let mut rows = Vec::with_capacity(entries.len() + 4);
    for (index, entry) in entries.iter().enumerate() {
        if index == 0 || entries[index - 1].section != entry.section {
            let count = entries[index..].iter().take_while(|e| e.section == entry.section).count();
            rows.push(Row::Heading(entry.section, count));
        }
        rows.push(Row::Entry(index, entry));
    }
    let selected_row = rows.iter().position(|r| matches!(r, Row::Entry(i, _) if *i == app.selected)).unwrap_or(0);
    let height = usize::from(area.height);
    let offset = selected_row.saturating_sub(height - 1);
    for (y, row) in rows.iter().skip(offset).take(height).enumerate() {
        let line = match row {
            Row::Heading(section, count) => Line::from(format!("{} ({count})", section.title())).bold(),
            Row::Entry(index, entry) => entry_line(entry, *index == app.selected),
        };
        frame.render_widget(line, Rect { y: area.y + y as u16, height: 1, ..area });
    }
}

fn entry_line(entry: &Entry, selected: bool) -> Line<'_> {
    let mut spans = vec![
        Span::raw(if selected { "▸ " } else { "  " }),
        Span::styled(entry.code.to_string(), section_style(entry.section)),
        Span::raw("  "),
    ];
    if let Some(orig) = &entry.orig {
        spans.push(Span::raw(format!("{orig} → ")).dim());
    }
    spans.push(Span::raw(entry.path.as_str()));
    let line = Line::from(spans);
    if selected { line.style(Style::new().bg(Color::DarkGray)) } else { line }
}

fn section_style(section: Section) -> Style {
    let color = match section {
        Section::Conflict => Color::Red,
        Section::Staged => Color::Green,
        Section::Unstaged => Color::Yellow,
        Section::Untracked => Color::Blue,
    };
    Style::new().fg(color).bold()
}

fn render_diff(frame: &mut Frame, lines: &[String], scroll: usize, area: Rect) {
    for (y, text) in lines.iter().skip(scroll).take(usize::from(area.height)).enumerate() {
        let span = Span::raw(text.as_str());
        let span = if text.starts_with("+++") || text.starts_with("---") || text.starts_with("diff ") {
            span.bold()
        } else if text.starts_with('+') {
            span.green()
        } else if text.starts_with('-') {
            span.red()
        } else if text.starts_with("@@") {
            span.cyan()
        } else {
            span
        };
        frame.render_widget(Line::from(span), Rect { y: area.y + y as u16, height: 1, ..area });
    }
}

fn render_branches(frame: &mut Frame, branches: &[Branch], matches: &[usize], selected: usize, area: Rect) {
    if area.height == 0 {
        return;
    }
    if matches.is_empty() {
        frame.render_widget(Line::from("  no matching branch — ⏎ creates it").dim(), area);
        return;
    }
    let height = usize::from(area.height);
    let offset = selected.saturating_sub(height - 1);
    for (y, (position, &index)) in matches.iter().enumerate().skip(offset).take(height).enumerate() {
        let branch = &branches[index];
        let mut spans = vec![
            Span::raw(if position == selected { "▸ " } else { "  " }),
            Span::raw(if branch.current { "* " } else { "  " }).green().bold(),
            Span::raw(branch.name.as_str()),
            Span::raw(format!("  {}", branch.age)).dim(),
        ];
        if !branch.track.is_empty() {
            spans.push(Span::raw(format!("  {}", branch.track)).yellow());
        }
        let line = Line::from(spans);
        let line = if position == selected { line.style(Style::new().bg(Color::DarkGray)) } else { line };
        frame.render_widget(line, Rect { y: area.y + y as u16, height: 1, ..area });
    }
}

fn render_stashes(frame: &mut Frame, stashes: &[Stash], selected: usize, area: Rect) {
    if area.height == 0 {
        return;
    }
    if stashes.is_empty() {
        frame.render_widget(Line::from("  no stashes").dim(), area);
        return;
    }
    let height = usize::from(area.height);
    let offset = selected.saturating_sub(height - 1);
    for (y, (position, stash)) in stashes.iter().enumerate().skip(offset).take(height).enumerate() {
        let line = Line::from(vec![
            Span::raw(if position == selected { "▸ " } else { "  " }),
            Span::raw(format!("stash@{{{}}}", stash.index)).magenta(),
            Span::raw(format!("  {}", stash.age)).dim(),
            Span::raw(format!("  {}", stash.subject)),
        ]);
        let line = if position == selected { line.style(Style::new().bg(Color::DarkGray)) } else { line };
        frame.render_widget(line, Rect { y: area.y + y as u16, height: 1, ..area });
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints = match (&app.mode, &app.view) {
        (Mode::Prompt { .. }, _) => "⏎ submit  esc cancel",
        (Mode::Confirm { .. }, _) => "y/⏎ confirm  n/esc cancel",
        (Mode::Normal, View::Status) => "␣ stage  a/u all  c commit  ⏎ diff  b branch  z stashes  S stash  r refresh",
        (Mode::Normal, View::Diff { .. }) => "↑↓ scroll  ␣/b page  g/G ends  esc back",
        (Mode::Normal, View::Branches { .. }) => "type to filter  ⏎ switch/create  esc back",
        (Mode::Normal, View::Stashes { .. }) => "⏎/p pop  a apply  x drop  esc back",
    };
    let hints = Line::from(hints).dim();
    let right = match &app.status {
        Some(Status::Error(message)) => Line::from(first_line(message)).red(),
        Some(Status::Info(message)) => Line::from(first_line(message)).green(),
        None => Line::default(),
    };
    // A notice wins the width: it's what the user needs to read right now.
    let right_width = u16::try_from(right.width() + 2).unwrap_or(u16::MAX).min(area.width);
    let [left_area, right_area] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_width)]).areas(area);
    frame.render_widget(hints, left_area);
    frame.render_widget(right.right_aligned(), right_area);
}

/// git errors can span lines ("error: …\nhint: …"); the first carries the gist.
fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Outcome};
    use crate::status::Snapshot;
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

    fn entry(section: Section, code: char, path: &str) -> Entry {
        Entry { section, code, path: path.into(), orig: None }
    }

    fn sample() -> App {
        App::new(
            "herdr-plugins".into(),
            Snapshot {
                branch: BranchInfo {
                    head: Some("main".into()),
                    oid: Some("abc1234".into()),
                    upstream: Some("origin/main".into()),
                    ahead: 2,
                    behind: 1,
                },
                stashes: 1,
                entries: vec![
                    Entry { orig: Some("old.rs".into()), ..entry(Section::Staged, 'R', "new.rs") },
                    entry(Section::Staged, 'M', "src/app.rs"),
                    entry(Section::Unstaged, 'M', "README.md"),
                    entry(Section::Untracked, '?', "notes.txt"),
                ],
            },
        )
    }

    #[test]
    fn status_layout_groups_entries_under_headings() {
        let lines = draw(&sample(), 110, 10);
        assert_eq!(lines[0], "herdr-plugins  ⎇ main → origin/main ↑2 ↓1  ≡ 1 stash");
        assert!(lines[1].starts_with("────"));
        assert_eq!(lines[2], "Staged (2)");
        assert_eq!(lines[3], "▸ R  old.rs → new.rs");
        assert_eq!(lines[4], "  M  src/app.rs");
        assert_eq!(lines[5], "Changes (1)");
        assert_eq!(lines[6], "  M  README.md");
        assert_eq!(lines[7], "Untracked (1)");
        assert_eq!(lines[8], "  ?  notes.txt");
        assert!(lines[9].starts_with("␣ stage"), "{:?}", lines[9]);
    }

    #[test]
    fn detached_unborn_and_clean_states() {
        let mut app = App::new("repo".into(), Snapshot::default());
        assert_eq!(draw(&app, 60, 5)[0], "repo  detached");
        assert_eq!(draw(&app, 60, 5)[2], "  nothing to commit, working tree clean");
        app.snapshot.branch.head = Some("main".into());
        assert_eq!(draw(&app, 60, 5)[0], "repo  ⎇ main  no commits yet");
        app.snapshot.branch = BranchInfo { oid: Some("abc1234".into()), ..Default::default() };
        assert_eq!(draw(&app, 60, 5)[0], "repo  detached @ abc1234");
    }

    #[test]
    fn selection_scrolls_into_view() {
        let entries = (0..20).map(|i| entry(Section::Unstaged, 'M', &format!("file{i:02}.rs"))).collect();
        let mut app = App::new("r".into(), Snapshot { entries, ..Default::default() });
        app.selected = 15;
        let lines = draw(&app, 40, 8);
        let selected: Vec<_> = lines.iter().filter(|l| l.starts_with('▸')).collect();
        assert_eq!(selected.len(), 1);
        assert!(selected[0].contains("file15.rs"), "{lines:?}");
    }

    #[test]
    fn diff_view_scrolls_and_shows_title() {
        let mut app = sample();
        let text = "diff --git a/x b/x\n@@ -1 +1 @@\n-old\n+new\n context";
        app.apply(Outcome::Diff { title: "x".into(), text: text.into() }, None);
        let lines = draw(&app, 60, 8);
        assert_eq!(lines[0], "Diff x");
        assert_eq!(&lines[2..7], ["diff --git a/x b/x", "@@ -1 +1 @@", "-old", "+new", " context"]);
        if let View::Diff { scroll, .. } = &mut app.view {
            *scroll = 2;
        }
        assert_eq!(draw(&app, 60, 8)[2], "-old");
    }

    #[test]
    fn branch_view_marks_current_and_empty_filter() {
        let mut app = sample();
        app.apply(
            Outcome::Branches(vec![
                Branch { name: "main".into(), current: true, age: "2 hours ago".into(), track: "[ahead 2]".into() },
                Branch { name: "topic".into(), current: false, age: "3 days ago".into(), track: String::new() },
            ]),
            None,
        );
        let lines = draw(&app, 70, 6);
        assert_eq!(lines[0], "Switch branch ›");
        assert_eq!(lines[2], "▸ * main  2 hours ago  [ahead 2]");
        assert_eq!(lines[3], "    topic  3 days ago");
        if let View::Branches { matches, .. } = &mut app.view {
            matches.clear();
        }
        assert_eq!(draw(&app, 70, 6)[2], "  no matching branch — ⏎ creates it");
    }

    #[test]
    fn stash_view_rows() {
        let mut app = sample();
        app.apply(
            Outcome::Stashes(vec![Stash { index: 0, age: "5 minutes ago".into(), subject: "On main: wip".into() }]),
            None,
        );
        assert_eq!(draw(&app, 70, 5)[2], "▸ stash@{0}  5 minutes ago  On main: wip");
        app.apply(Outcome::Stashes(vec![]), None);
        assert_eq!(draw(&app, 70, 5)[2], "  no stashes");
    }

    #[test]
    fn prompt_and_confirm_replace_the_header() {
        let mut app = sample();
        app.mode = Mode::Prompt { purpose: Purpose::Commit, input: "fix it".into() };
        let lines = draw(&app, 70, 5);
        assert_eq!(lines[0], "Commit message › fix it");
        assert!(lines[4].starts_with("⏎ submit"));
        app.mode = Mode::Confirm { question: "Drop stash@{0}?".into(), op: crate::app::Op::Refresh };
        assert_eq!(draw(&app, 70, 5)[0], "Drop stash@{0}?");
    }

    #[test]
    fn footer_shows_first_line_of_a_notice() {
        let mut app = sample();
        app.status = Some(Status::Error("error: pathspec 'x' did not match\nhint: try again".into()));
        let footer = &draw(&app, 120, 5)[4];
        assert!(footer.ends_with("error: pathspec 'x' did not match"), "{footer:?}");
        assert!(!footer.contains("hint"));
    }

    #[test]
    fn tiny_areas_do_not_panic() {
        let mut app = sample();
        for (w, h) in [(1, 1), (10, 2), (20, 3), (20, 4), (80, 4)] {
            draw(&app, w, h);
        }
        app.status = Some(Status::Error("a very long error message that is wider than the popup".into()));
        draw(&app, 10, 4);
        assert_eq!(body_height(2), 0);
        assert_eq!(body_height(10), 7);
    }
}
