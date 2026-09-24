use serde::Deserialize;

/// Where the user was when a plugin was invoked, from `HERDR_PLUGIN_CONTEXT_JSON`.
/// For popup panes this describes the tiled pane underneath the popup.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct PluginContext {
    pub workspace_id: Option<String>,
    pub workspace_label: Option<String>,
    pub workspace_cwd: Option<String>,
    pub tab_id: Option<String>,
    pub tab_label: Option<String>,
    pub focused_pane_id: Option<String>,
    pub focused_pane_cwd: Option<String>,
    pub focused_pane_agent: Option<String>,
}

impl PluginContext {
    pub fn from_json(raw: &str) -> serde_json::Result<Self> {
        serde_json::from_str(raw)
    }

    /// Missing or unparseable context yields an empty context rather than an
    /// error, so a plugin opened outside herdr still starts.
    pub fn from_env_value(raw: Option<&str>) -> Self {
        raw.and_then(|raw| Self::from_json(raw).ok()).unwrap_or_default()
    }

    pub fn from_env() -> Self {
        Self::from_env_value(std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok().as_deref())
    }
}
