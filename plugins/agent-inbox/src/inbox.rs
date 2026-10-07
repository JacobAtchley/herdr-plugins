//! Turns herdr's agent, workspace and tab lists into the inbox rows.

use herdr_client::models::{Agent, AgentStatus, Tab, Workspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub pane_id: String,
    pub agent: String,
    pub status: AgentStatus,
    /// `workspace › tab`.
    pub location: String,
    pub seq: u64,
    pub title: Option<String>,
    /// One line of the agent's output; filled in by the caller after reading the pane.
    pub preview: Option<String>,
}

/// Agents waiting on the user, and agents still working.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inbox {
    /// Blocked first, then done; within each, the longest waiting first.
    pub needs: Vec<Entry>,
    pub working: Vec<Entry>,
}

impl Inbox {
    pub fn build(agents: &[Agent], workspaces: &[Workspace], tabs: &[Tab]) -> Self {
        let mut inbox = Inbox::default();
        for agent in agents {
            let entry = Entry {
                pane_id: agent.pane_id.clone(),
                agent: agent_name(agent),
                status: agent.agent_status,
                location: location(agent, workspaces, tabs),
                seq: agent.state_change_seq.unwrap_or(0),
                title: agent.title.clone().filter(|title| !title.trim().is_empty()),
                preview: None,
            };
            match agent.agent_status {
                AgentStatus::Blocked | AgentStatus::Done => inbox.needs.push(entry),
                AgentStatus::Working => inbox.working.push(entry),
                AgentStatus::Idle | AgentStatus::Unknown => {}
            }
        }
        inbox.needs.sort_by_key(|entry| (entry.status != AgentStatus::Blocked, entry.seq));
        inbox.working.sort_by(|a, b| a.location.cmp(&b.location));
        inbox
    }

    pub fn len(&self) -> usize {
        self.needs.len() + self.working.len()
    }

    /// Rows in display order: needs first, then working.
    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.needs.get(index).or_else(|| self.working.get(index.checked_sub(self.needs.len())?))
    }

    pub fn position(&self, pane_id: &str) -> Option<usize> {
        self.needs.iter().chain(&self.working).position(|entry| entry.pane_id == pane_id)
    }
}

fn agent_name(agent: &Agent) -> String {
    agent.display_agent.clone().or_else(|| agent.agent.clone()).unwrap_or_else(|| "agent".into())
}

fn location(agent: &Agent, workspaces: &[Workspace], tabs: &[Tab]) -> String {
    let workspace = workspaces.iter().find(|w| w.workspace_id == agent.workspace_id).map_or_else(
        || agent.workspace_id.clone(),
        |w| if w.label.is_empty() { format!("Workspace {}", w.number) } else { w.label.clone() },
    );
    let tab = tabs.iter().find(|t| t.tab_id == agent.tab_id).map_or_else(
        || agent.tab_id.clone(),
        |t| if t.label.is_empty() { format!("Tab {}", t.number) } else { t.label.clone() },
    );
    format!("{workspace} › {tab}")
}

/// Picks the line worth showing under an agent from its terminal snapshot.
///
/// Blocked agents show their question (the last line ending in `?`). Done
/// agents show the first line of their last message (the last line starting
/// with a message bullet such as `⏺` or `•`). Returns `None` when neither is
/// found, so the caller can fall back to the terminal title.
pub fn preview(status: AgentStatus, snapshot: &str) -> Option<String> {
    let lines: Vec<String> = snapshot.lines().map(clean).filter(|line| !line.is_empty()).collect();
    match status {
        AgentStatus::Blocked => lines.iter().rev().find(|line| line.ends_with('?')).cloned(),
        AgentStatus::Done => lines.iter().rev().find_map(|line| {
            let rest = line.strip_prefix('⏺').or_else(|| line.strip_prefix('•'))?;
            let rest = rest.trim();
            (!rest.is_empty()).then(|| rest.to_string())
        }),
        _ => None,
    }
}

/// Strips box-drawing borders and prompt markers that agent TUIs draw around text.
fn clean(line: &str) -> String {
    const FRAME: &[char] = &['│', '┃', '╭', '╮', '╰', '╯', '─', '━', '❯', '>', ' ', '\t'];
    line.trim_matches(FRAME).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(pane: &str, tab: &str, status: AgentStatus, seq: u64) -> Agent {
        Agent {
            pane_id: pane.into(),
            workspace_id: "w1".into(),
            tab_id: tab.into(),
            agent: Some("claude".into()),
            name: None,
            display_agent: None,
            agent_status: status,
            focused: false,
            cwd: None,
            state_change_seq: Some(seq),
            title: Some("Fix auth".into()),
        }
    }

    fn workspace() -> Workspace {
        Workspace {
            workspace_id: "w1".into(),
            number: 1,
            label: "api".into(),
            focused: false,
            agent_status: AgentStatus::Unknown,
        }
    }

    fn tab(id: &str, number: u32, label: &str) -> Tab {
        Tab {
            tab_id: id.into(),
            workspace_id: "w1".into(),
            number,
            label: label.into(),
            focused: false,
            agent_status: AgentStatus::Unknown,
        }
    }

    #[test]
    fn blocked_comes_before_done_and_older_changes_first() {
        let agents = vec![
            agent("w1:p1", "w1:t1", AgentStatus::Done, 5),
            agent("w1:p2", "w1:t1", AgentStatus::Blocked, 9),
            agent("w1:p3", "w1:t1", AgentStatus::Blocked, 3),
            agent("w1:p4", "w1:t1", AgentStatus::Done, 2),
            agent("w1:p5", "w1:t1", AgentStatus::Idle, 1),
            agent("w1:p6", "w1:t1", AgentStatus::Working, 1),
        ];
        let inbox = Inbox::build(&agents, &[workspace()], &[tab("w1:t1", 1, "")]);
        let order: Vec<_> = inbox.needs.iter().map(|e| e.pane_id.as_str()).collect();
        assert_eq!(order, ["w1:p3", "w1:p2", "w1:p4", "w1:p1"]);
        assert_eq!(inbox.working.len(), 1);
        assert_eq!(inbox.len(), 5);
        assert_eq!(inbox.get(4).unwrap().pane_id, "w1:p6");
        assert_eq!(inbox.position("w1:p6"), Some(4));
    }

    #[test]
    fn location_uses_labels_and_falls_back_to_numbers_or_ids() {
        let agents = vec![agent("w1:p1", "w1:t1", AgentStatus::Done, 1), agent("w1:p2", "w1:t9", AgentStatus::Done, 2)];
        let inbox = Inbox::build(&agents, &[workspace()], &[tab("w1:t1", 2, "")]);
        assert_eq!(inbox.needs[0].location, "api › Tab 2");
        assert_eq!(inbox.needs[1].location, "api › w1:t9");
    }

    #[test]
    fn blocked_preview_is_the_last_question() {
        let snapshot = "\
⏺ Update(src/auth/token.rs)
╭──────────────────────────────────────────╮
│ Do you want to make this edit to token.rs? │
│ ❯ 1. Yes                                   │
│   2. No, and tell Claude what to do (esc)  │
╰──────────────────────────────────────────╯
";
        assert_eq!(
            preview(AgentStatus::Blocked, snapshot).as_deref(),
            Some("Do you want to make this edit to token.rs?")
        );
    }

    #[test]
    fn done_preview_is_the_last_message_bullet() {
        let snapshot = "\
⏺ Looking at the tests.
  Ran 2 shell commands
⏺ Fixed. Ran just ci, all green.
  - details
────────────
❯
────────────
";
        assert_eq!(preview(AgentStatus::Done, snapshot).as_deref(), Some("Fixed. Ran just ci, all green."));
        assert_eq!(preview(AgentStatus::Done, "• Codex finished.\n").as_deref(), Some("Codex finished."));
    }

    #[test]
    fn preview_is_none_without_a_match() {
        assert_eq!(preview(AgentStatus::Blocked, "no question here\n"), None);
        assert_eq!(preview(AgentStatus::Done, "plain output\n"), None);
        assert_eq!(preview(AgentStatus::Working, "⏺ busy\n"), None);
    }
}
