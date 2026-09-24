//! One row in the palette, whatever its source.

use herdr_client::models::AgentStatus;

use crate::sources::builtins::Builtin;
use crate::sources::user::UserCommand;

/// Declaration order is the tiebreak order when scores are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Workspace,
    Tab,
    Agent,
    Command,
    Plugin,
    User,
}

impl Kind {
    pub fn badge(self) -> &'static str {
        match self {
            Kind::Workspace => "WS",
            Kind::Tab => "TAB",
            Kind::Agent => "AGT",
            Kind::Command => "CMD",
            Kind::Plugin => "PLG",
            Kind::User => "USR",
        }
    }
}

// Variant names mirror the herdr operations they call (workspace.focus,
// tab.focus, plugin.action.invoke, ...), not one another; keep them as-is.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    FocusWorkspace(String),
    FocusTab(String),
    FocusPane(String),
    Builtin(Builtin),
    InvokePluginAction(String),
    RunUser(UserCommand),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: Kind,
    /// Stable frecency key, e.g. `tab:w5:t2` or `cmd:split-right`.
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub keywords: Vec<String>,
    pub status: Option<AgentStatus>,
    /// The workspace/tab/pane the user is already in; hidden when the query is empty.
    pub current: bool,
    pub action: Action,
}

impl Item {
    pub fn new(kind: Kind, id: impl Into<String>, title: impl Into<String>, action: Action) -> Self {
        Self {
            kind,
            id: id.into(),
            title: title.into(),
            subtitle: None,
            keywords: Vec::new(),
            status: None,
            current: false,
            action,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    pub fn status(mut self, status: AgentStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// The text the fuzzy matcher sees. The title comes first so match
    /// positions below the title's length index into the title.
    pub fn haystack(&self) -> String {
        let mut parts = vec![self.title.as_str()];
        parts.extend(self.subtitle.as_deref());
        parts.extend(self.keywords.iter().map(String::as_str));
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haystack_puts_title_first_then_subtitle_and_keywords() {
        let item = Item::new(Kind::Plugin, "plugin:a.b", "Refresh radar", Action::InvokePluginAction("a.b".into()))
            .subtitle("hhdebb.herdr-radar")
            .keywords(vec!["rescan".into(), "agents".into()]);
        assert_eq!(item.haystack(), "Refresh radar hhdebb.herdr-radar rescan agents");
    }

    #[test]
    fn haystack_without_extras_is_title() {
        let item = Item::new(Kind::Workspace, "ws:w1", "CT", Action::FocusWorkspace("w1".into()));
        assert_eq!(item.haystack(), "CT");
        assert!(!item.current);
        assert_eq!(item.status, None);
    }

    #[test]
    fn kind_order_and_badges() {
        assert!(Kind::Workspace < Kind::Tab && Kind::Tab < Kind::Agent && Kind::Agent < Kind::Command);
        assert!(Kind::Command < Kind::Plugin && Kind::Plugin < Kind::User);
        let badges: Vec<_> = [Kind::Workspace, Kind::Tab, Kind::Agent, Kind::Command, Kind::Plugin, Kind::User]
            .iter()
            .map(|k| k.badge())
            .collect();
        assert_eq!(badges, ["WS", "TAB", "AGT", "CMD", "PLG", "USR"]);
    }
}
