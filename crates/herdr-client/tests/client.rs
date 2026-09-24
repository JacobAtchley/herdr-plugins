use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use herdr_client::{Api, Client, Error};
use serde_json::{Value, json};

/// A fake herdr server: answers one connection per canned response, in order,
/// and returns the requests it received.
fn serve(responses: Vec<String>) -> (tempfile::TempDir, PathBuf, JoinHandle<Vec<Value>>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("herdr.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let handle = thread::spawn(move || {
        let mut received = Vec::new();
        for response in responses {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            received.push(serde_json::from_str(&line).unwrap());
            let mut stream = stream;
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(b"\n").unwrap();
        }
        received
    });
    (dir, path, handle)
}

fn compact_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    serde_json::to_string(&value).unwrap()
}

#[test]
fn request_sends_method_and_params_and_returns_result() {
    let (_dir, path, server) = serve(vec![r#"{"id":"x","result":{"type":"ok"}}"#.into()]);
    let client = Client::new(&path);
    let result = client.request("tab.focus", json!({"tab_id": "w1:t2"})).unwrap();
    assert_eq!(result, json!({"type": "ok"}));

    let received = server.join().unwrap();
    assert_eq!(received[0]["method"], "tab.focus");
    assert_eq!(received[0]["params"], json!({"tab_id": "w1:t2"}));
    assert!(received[0]["id"].as_str().unwrap().starts_with("req_"));
}

#[test]
fn api_error_maps_to_error_api() {
    let (_dir, path, _server) =
        serve(vec![r#"{"id":"x","error":{"code":"pane_not_found","message":"pane bogus not found"}}"#.into()]);
    let err = Client::new(&path).request("pane.get", json!({"pane_id": "bogus"})).unwrap_err();
    match &err {
        Error::Api { method, code, message } => {
            assert_eq!(method, "pane.get");
            assert_eq!(code, "pane_not_found");
            assert_eq!(message, "pane bogus not found");
        }
        other => panic!("expected Api error, got {other:?}"),
    }
    assert_eq!(err.to_string(), "pane.get: pane bogus not found (pane_not_found)");
}

#[test]
fn invalid_json_is_protocol_error() {
    let (_dir, path, _server) = serve(vec!["not json".into()]);
    let err = Client::new(&path).request("ping", json!({})).unwrap_err();
    assert!(matches!(err, Error::Protocol { ref method, .. } if method == "ping"), "{err:?}");
}

#[test]
fn missing_result_is_protocol_error() {
    let (_dir, path, _server) = serve(vec![r#"{"id":"x"}"#.into()]);
    let err = Client::new(&path).request("ping", json!({})).unwrap_err();
    assert!(matches!(err, Error::Protocol { .. }), "{err:?}");
}

#[test]
fn unreachable_socket_is_io_error_naming_the_method() {
    let dir = tempfile::tempdir().unwrap();
    let err = Client::new(dir.path().join("nope.sock")).request("workspace.list", json!({})).unwrap_err();
    assert!(matches!(err, Error::Io { ref method, .. } if method == "workspace.list"), "{err:?}");
    assert!(err.to_string().starts_with("workspace.list: "));
}

#[test]
fn typed_list_helpers_parse_fixtures() {
    let (_dir, path, server) = serve(vec![
        compact_fixture("workspace_list.json"),
        compact_fixture("tab_list.json"),
        compact_fixture("agent_list.json"),
        compact_fixture("plugin_action_list.json"),
    ]);
    let client = Client::new(&path);
    assert_eq!(client.workspace_list().unwrap().len(), 3);
    assert_eq!(client.tab_list().unwrap().len(), 3);
    assert_eq!(client.agent_list().unwrap().len(), 2);
    assert_eq!(client.plugin_action_list().unwrap()[1].action_id, "refresh");

    let methods: Vec<String> =
        server.join().unwrap().iter().map(|r| r["method"].as_str().unwrap().to_string()).collect();
    assert_eq!(methods, ["workspace.list", "tab.list", "agent.list", "plugin.action.list"]);
}

#[test]
fn list_helper_reports_missing_field() {
    let (_dir, path, _server) = serve(vec![r#"{"id":"x","result":{"type":"workspace_list"}}"#.into()]);
    let err = Client::new(&path).workspace_list().unwrap_err();
    match err {
        Error::Protocol { method, detail } => {
            assert_eq!(method, "workspace.list");
            assert!(detail.contains("workspaces"), "{detail}");
        }
        other => panic!("expected Protocol error, got {other:?}"),
    }
}
