//! Integration tests for the `argsh-dap` Debug Adapter Protocol binary.
//!
//! Each test spawns the release binary as a subprocess and sends/receives
//! DAP messages (Content-Length framed JSON, same as LSP).

use std::io::{BufRead, BufReader, Read as IoRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_argsh-dap"))
}

fn send_dap_message(stdin: &mut impl Write, msg: &Value) {
    let body = serde_json::to_string(msg).unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    stdin.flush().unwrap();
}

fn read_dap_message(reader: &mut BufReader<impl IoRead>) -> Value {
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.parse().unwrap();
        }
    }
    assert!(content_length > 0, "Content-Length header missing or zero");
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Spawn argsh-dap, send initialize, return (stdin, reader, seq_counter)
fn start_session() -> (
    std::process::ChildStdin,
    BufReader<std::process::ChildStdout>,
    std::process::Child,
) {
    let mut child = Command::new(bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn argsh-dap");

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    (stdin, reader, child)
}

// ---------------------------------------------------------------------------

#[test]
fn version_flag() {
    let output = Command::new(bin())
        .arg("--version")
        .output()
        .expect("run argsh-dap --version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("argsh-dap "), "stdout: {}", stdout);
}

#[test]
fn help_flag() {
    let output = Command::new(bin())
        .arg("--help")
        .output()
        .expect("run argsh-dap --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Debug Adapter Protocol"));
}

#[test]
fn unknown_flag_exits_two() {
    let output = Command::new(bin())
        .arg("--bad")
        .output()
        .expect("run argsh-dap --bad");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn initialize_returns_capabilities() {
    let (mut stdin, mut reader, mut child) = start_session();

    send_dap_message(&mut stdin, &json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {
            "clientID": "test",
            "adapterID": "argsh",
        }
    }));

    // Should get an initialize response
    let resp = read_dap_message(&mut reader);
    assert_eq!(resp["type"], "response");
    assert_eq!(resp["command"], "initialize");
    assert_eq!(resp["success"], true);
    assert!(resp["body"]["supportsConfigurationDoneRequest"].as_bool().unwrap());
    assert!(resp["body"]["supportsFunctionBreakpoints"].as_bool().unwrap());
    assert!(resp["body"]["supportsEvaluateForHovers"].as_bool().unwrap());

    // Should also get an initialized event
    let evt = read_dap_message(&mut reader);
    assert_eq!(evt["type"], "event");
    assert_eq!(evt["event"], "initialized");

    // Disconnect
    send_dap_message(&mut stdin, &json!({
        "seq": 2,
        "type": "request",
        "command": "disconnect",
    }));

    let _ = read_dap_message(&mut reader); // disconnect response
    let _ = child.wait();
}

#[test]
fn threads_returns_main_thread() {
    let (mut stdin, mut reader, mut child) = start_session();

    send_dap_message(&mut stdin, &json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {}
    }));
    let _ = read_dap_message(&mut reader); // response
    let _ = read_dap_message(&mut reader); // initialized event

    send_dap_message(&mut stdin, &json!({
        "seq": 2,
        "type": "request",
        "command": "threads",
    }));

    let resp = read_dap_message(&mut reader);
    assert_eq!(resp["type"], "response");
    assert_eq!(resp["command"], "threads");
    let threads = resp["body"]["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["id"], 1);
    assert_eq!(threads[0]["name"], "main");

    send_dap_message(&mut stdin, &json!({
        "seq": 3,
        "type": "request",
        "command": "disconnect",
    }));
    let _ = read_dap_message(&mut reader);
    let _ = child.wait();
}

#[test]
fn set_breakpoints_returns_verified() {
    let (mut stdin, mut reader, mut child) = start_session();

    send_dap_message(&mut stdin, &json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {}
    }));
    let _ = read_dap_message(&mut reader);
    let _ = read_dap_message(&mut reader);

    send_dap_message(&mut stdin, &json!({
        "seq": 2,
        "type": "request",
        "command": "setBreakpoints",
        "arguments": {
            "source": { "path": "/tmp/test.sh" },
            "breakpoints": [
                { "line": 5 },
                { "line": 10 },
            ]
        }
    }));

    let resp = read_dap_message(&mut reader);
    assert_eq!(resp["command"], "setBreakpoints");
    assert_eq!(resp["success"], true);
    let bps = resp["body"]["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 2);
    assert!(bps[0]["verified"].as_bool().unwrap());
    assert_eq!(bps[0]["line"], 5);
    assert!(bps[1]["verified"].as_bool().unwrap());
    assert_eq!(bps[1]["line"], 10);

    send_dap_message(&mut stdin, &json!({
        "seq": 3,
        "type": "request",
        "command": "disconnect",
    }));
    let _ = read_dap_message(&mut reader);
    let _ = child.wait();
}

#[test]
fn configuration_done_succeeds() {
    let (mut stdin, mut reader, mut child) = start_session();

    send_dap_message(&mut stdin, &json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {}
    }));
    let _ = read_dap_message(&mut reader);
    let _ = read_dap_message(&mut reader);

    send_dap_message(&mut stdin, &json!({
        "seq": 2,
        "type": "request",
        "command": "configurationDone",
    }));

    let resp = read_dap_message(&mut reader);
    assert_eq!(resp["command"], "configurationDone");
    assert_eq!(resp["success"], true);

    send_dap_message(&mut stdin, &json!({
        "seq": 3,
        "type": "request",
        "command": "disconnect",
    }));
    let _ = read_dap_message(&mut reader);
    let _ = child.wait();
}

#[test]
fn scopes_includes_argsh_args_scope() {
    let (mut stdin, mut reader, mut child) = start_session();

    // Create a temp script with :args
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("test.sh");
    std::fs::write(&script, "#!/usr/bin/env argsh\nmain() {\n  local name\n  local -a args=(\n    'name' \"Name\"\n  )\n  :args \"Test\" \"${@}\"\n}\nmain \"${@}\"\n").unwrap();

    send_dap_message(&mut stdin, &json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {}
    }));
    let _ = read_dap_message(&mut reader);
    let _ = read_dap_message(&mut reader);

    send_dap_message(&mut stdin, &json!({
        "seq": 2,
        "type": "request",
        "command": "launch",
        "arguments": {
            "program": script.to_str().unwrap(),
            "args": ["test"],
            "stopOnEntry": true,
        }
    }));
    let resp = read_dap_message(&mut reader);
    assert_eq!(resp["success"], true, "launch failed: {:?}", resp);

    // Request scopes — should include "argsh Args" because analysis found :args.
    // frameId:0 is used without a prior stackTrace request because this test
    // only verifies that scopes are returned correctly based on static analysis,
    // not runtime state.
    send_dap_message(&mut stdin, &json!({
        "seq": 3,
        "type": "request",
        "command": "scopes",
        "arguments": { "frameId": 0 }
    }));
    let resp = read_dap_message(&mut reader);
    let scopes = resp["body"]["scopes"].as_array().unwrap();
    assert!(scopes.len() >= 2, "expected Locals + argsh Args, got: {:?}", scopes);
    assert_eq!(scopes[0]["name"], "Locals");
    assert_eq!(scopes[1]["name"], "argsh Args");

    send_dap_message(&mut stdin, &json!({
        "seq": 4,
        "type": "request",
        "command": "disconnect",
    }));
    // Read responses/events until disconnect response
    loop {
        let msg = read_dap_message(&mut reader);
        if msg["command"] == "disconnect" { break; }
    }
    let _ = child.wait();
}
