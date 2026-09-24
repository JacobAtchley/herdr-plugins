use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::models::{Agent, PluginAction, Tab, Workspace};

const READ_TIMEOUT: Duration = Duration::from_secs(5);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HERDR_SOCKET_PATH is not set")]
    NoSocket,
    #[error("{method}: {source}")]
    Io {
        method: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{method}: {message} ({code})")]
    Api { method: String, code: String, message: String },
    #[error("{method}: invalid response: {detail}")]
    Protocol { method: String, detail: String },
}

/// The herdr socket API. `Client` talks to a real server; tests substitute a fake.
pub trait Api: Send + Sync {
    /// Sends one request and returns the response's `result` object.
    fn request(&self, method: &str, params: Value) -> Result<Value, Error>;

    fn workspace_list(&self) -> Result<Vec<Workspace>, Error> {
        list(self.request("workspace.list", json!({}))?, "workspace.list", "workspaces")
    }

    fn tab_list(&self) -> Result<Vec<Tab>, Error> {
        list(self.request("tab.list", json!({}))?, "tab.list", "tabs")
    }

    fn agent_list(&self) -> Result<Vec<Agent>, Error> {
        list(self.request("agent.list", json!({}))?, "agent.list", "agents")
    }

    fn plugin_action_list(&self) -> Result<Vec<PluginAction>, Error> {
        list(self.request("plugin.action.list", json!({}))?, "plugin.action.list", "actions")
    }
}

fn list<T: DeserializeOwned>(mut result: Value, method: &str, field: &str) -> Result<Vec<T>, Error> {
    let protocol = |detail: String| Error::Protocol { method: method.to_string(), detail };
    let items = result.get_mut(field).map(Value::take).ok_or_else(|| protocol(format!("missing `{field}`")))?;
    serde_json::from_value(items).map_err(|e| protocol(e.to_string()))
}

/// One newline-delimited JSON request per connection over herdr's Unix socket.
#[derive(Debug, Clone)]
pub struct Client {
    path: PathBuf,
}

impl Client {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_env() -> Result<Self, Error> {
        std::env::var_os("HERDR_SOCKET_PATH").map(Self::new).ok_or(Error::NoSocket)
    }
}

impl Api for Client {
    fn request(&self, method: &str, params: Value) -> Result<Value, Error> {
        let io = |source: std::io::Error| Error::Io { method: method.to_string(), source };
        let id = format!("req_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');

        let mut stream = UnixStream::connect(&self.path).map_err(io)?;
        stream.set_read_timeout(Some(READ_TIMEOUT)).map_err(io)?;
        stream.write_all(line.as_bytes()).map_err(io)?;

        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).map_err(io)?;
        parse_response(method, &reply)
    }
}

fn parse_response(method: &str, reply: &str) -> Result<Value, Error> {
    let protocol = |detail: String| Error::Protocol { method: method.to_string(), detail };
    let mut value: Value = serde_json::from_str(reply.trim()).map_err(|e| protocol(e.to_string()))?;
    if let Some(error) = value.get("error") {
        return Err(Error::Api {
            method: method.to_string(),
            code: error["code"].as_str().unwrap_or("unknown").to_string(),
            message: error["message"].as_str().unwrap_or_default().to_string(),
        });
    }
    value.get_mut("result").map(Value::take).ok_or_else(|| protocol("missing `result`".to_string()))
}
