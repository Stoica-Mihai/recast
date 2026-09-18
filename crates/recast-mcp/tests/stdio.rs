//! Transport-level tests: spawn the real `recast-mcp` binary and speak
//! JSON-RPC over its stdio, the way an MCP client does.
//!
//! The in-process tests in `server_tests.rs` call the handler methods
//! directly. That is faster and covers the engine, but it serializes
//! nothing — so it is structurally blind to a tool that never got
//! registered, an argument schema that does not match the documented
//! one, and an error payload that is correct as a Rust value but wrong
//! on the wire. Every test here is one of those cases.
//!
//! No JSON-RPC client crate: `CARGO_BIN_EXE_recast-mcp` is the binary
//! Cargo just built, and the protocol is newline-delimited JSON.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

/// Generous: CI machines are slow, and a wrong answer fails on content
/// rather than on the clock.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_recast-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {}: {e}", env!("CARGO_BIN_EXE_recast-mcp")));

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        // Reader thread + recv_timeout, so a server that never answers
        // fails the test instead of hanging CI forever.
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        let mut server = Server { child, stdin, lines, next_id: 0 };
        let init = server.request(
            "initialize",
            json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "stdio-test", "version": "0"}
            }),
        );
        assert!(init.get("result").is_some(), "initialize failed: {init}");
        server.notify("notifications/initialized", json!({}));
        server
    }

    fn send(&mut self, msg: &Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.send(&msg);
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(REPLY_TIMEOUT)
                .unwrap_or_else(|e| panic!("no reply to {method} within {REPLY_TIMEOUT:?}: {e}"));
            let msg: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if msg.get("id").and_then(Value::as_u64) == Some(id) {
                return msg;
            }
        }
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn tool_names(server: &mut Server) -> Vec<String> {
    let reply = server.request("tools/list", json!({}));
    let mut names: Vec<String> = reply["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("no tools array in {reply}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

fn schema_for(server: &mut Server, tool: &str) -> Value {
    let reply = server.request("tools/list", json!({}));
    reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == tool)
        .unwrap_or_else(|| panic!("{tool} not advertised"))["inputSchema"]
        .clone()
}

/// Text blocks of a successful `tools/call`.
fn contents(reply: &Value) -> Vec<String> {
    reply["result"]["content"]
        .as_array()
        .unwrap_or_else(|| panic!("no content in {reply}"))
        .iter()
        .filter_map(|c| c["text"].as_str().map(str::to_owned))
        .collect()
}

fn error_of(reply: &Value) -> (String, String, Vec<String>) {
    let err = reply.get("error").unwrap_or_else(|| panic!("expected an error, got {reply}"));
    let message = err["message"].as_str().unwrap_or_default().to_owned();
    let kind = err["data"]["kind"].as_str().unwrap_or_default().to_owned();
    let remedies = err["data"]["remedies"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    (kind, message, remedies)
}

#[test]
fn handshake_succeeds_and_every_tool_is_advertised() {
    let mut server = Server::start();
    assert_eq!(
        tool_names(&mut server),
        [
            "recast_apply",
            "recast_preview",
            "recast_recover",
            "recast_rename",
            "recast_search",
            "recast_structural",
        ]
    );
}

/// The docs promise these argument names. In-process tests construct the
/// args struct directly and so cannot catch a schema that disagrees.
#[test]
fn the_advertised_schema_matches_the_documented_arguments() {
    let mut server = Server::start();

    let rewrite = schema_for(&mut server, "recast_apply");
    for arg in ["pattern", "replacement", "word", "literal", "at_least", "allow_non_convergent"] {
        assert!(rewrite["properties"].get(arg).is_some(), "recast_apply lost `{arg}`: {rewrite}");
    }
    assert!(
        rewrite["properties"].get("force").is_none(),
        "recast_apply must not offer a lock override: {rewrite}"
    );

    let rename = schema_for(&mut server, "recast_rename");
    assert!(rename["properties"].get("renames").is_some(), "recast_rename lost `renames`");
    assert_eq!(rename["required"], json!(["renames"]), "renames must be required: {rename}");
}

#[test]
fn an_error_carries_kind_and_remedies_over_the_wire() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "let x = Outcome;\n").unwrap();

    let mut server = Server::start();
    let reply = server.call_tool(
        "recast_apply",
        json!({
            "pattern": "Outcome",
            "replacement": "ReadOutcome",
            "paths": [dir.path()],
            "literal": true
        }),
    );

    let (kind, message, remedies) = error_of(&reply);
    assert_eq!(kind, "non_convergent_replacement", "{message}");
    assert_eq!(remedies, ["word", "allow_non_convergent"], "{message}");
    assert!(message.contains("set word or allow_non_convergent"), "{message}");
}

/// The regression this file exists for. `recast-core` is shared with the
/// CLI, and its `locked` message used to say "use --force to override" —
/// an argument this surface does not have. Nothing in-process saw it,
/// because the message is only assembled on the way out.
#[test]
fn a_held_lock_offers_no_remedy_and_names_no_cli_flag() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old\n").unwrap();

    let lock_path =
        recast_core::workspace_lock_path(&recast_core::workspace_root(&[dir.path().to_path_buf()]));
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let held =
        fs::OpenOptions::new().write(true).create(true).truncate(false).open(&lock_path).unwrap();
    fs2::FileExt::try_lock_exclusive(&held).unwrap();

    let mut server = Server::start();
    let reply = server.call_tool(
        "recast_apply",
        json!({"pattern": "old", "replacement": "new", "paths": [dir.path()]}),
    );

    let (kind, message, remedies) = error_of(&reply);
    assert_eq!(kind, "locked", "{message}");
    assert!(remedies.is_empty(), "a live holder cannot be argued past: {remedies:?}");
    assert!(!message.contains("--"), "CLI flag leaked to the MCP surface: {message}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n", "wrote despite the lock");
}

#[test]
fn an_apply_writes_through_the_transport() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old line\n").unwrap();

    let mut server = Server::start();
    let reply = server.call_tool(
        "recast_apply",
        json!({"pattern": "old", "replacement": "new", "paths": [dir.path()]}),
    );

    assert!(reply.get("error").is_none(), "{reply}");
    assert!(contents(&reply).iter().any(|c| c.contains(r#""files_written":1"#)), "{reply}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new line\n");
}

#[test]
fn dependent_renames_stay_apart_through_the_transport() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("c.rs");
    fs::write(&target, "use Foo;\nuse Bar;\n").unwrap();

    let mut server = Server::start();
    let reply = server.call_tool(
        "recast_rename",
        json!({"renames": {"Foo": "Bar", "Bar": "Baz"}, "paths": [dir.path()], "apply": true}),
    );

    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "use Bar;\nuse Baz;\n");
    assert!(
        contents(&reply).iter().any(|c| c.contains("correct exactly once")),
        "the once-only note never reached the wire: {reply}"
    );
}

#[test]
fn a_diverging_rename_map_is_refused_over_the_wire() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "Foo\n").unwrap();

    let mut server = Server::start();
    let reply = server
        .call_tool("recast_rename", json!({"renames": {"Foo": "Foo Bar"}, "paths": [dir.path()]}));

    let (kind, message, _) = error_of(&reply);
    assert_eq!(kind, "rename_map_diverges", "{message}");
}
