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

/// Issue #12: Test that setFunctionBreakpoints resolves a :usage command name
/// to a file:line breakpoint.
#[test]
fn set_function_breakpoints_resolves_command() {
    let (mut stdin, mut reader, mut child) = start_session();

    // Create a temp script with :usage dispatch
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("cli.sh");
    std::fs::write(&script, r#"#!/usr/bin/env argsh
main() {
  local -a usage=(
    'deploy' "Deploy the app"
  )
  :usage "${@}"
}
main::deploy() {
  echo "deploying"
}
main "${@}"
"#).unwrap();

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
        "command": "launch",
        "arguments": {
            "program": script.to_str().unwrap(),
            "args": ["deploy"],
            "stopOnEntry": true,
        }
    }));
    let resp = read_dap_message(&mut reader);
    assert_eq!(resp["success"], true, "launch failed: {:?}", resp);

    // Set a function breakpoint by command name "deploy"
    send_dap_message(&mut stdin, &json!({
        "seq": 3,
        "type": "request",
        "command": "setFunctionBreakpoints",
        "arguments": {
            "breakpoints": [
                { "name": "deploy" }
            ]
        }
    }));
    let resp = read_dap_message(&mut reader);
    assert_eq!(resp["command"], "setFunctionBreakpoints");
    assert_eq!(resp["success"], true);
    let bps = resp["body"]["breakpoints"].as_array().unwrap();
    assert_eq!(bps.len(), 1);
    assert!(bps[0]["verified"].as_bool().unwrap(),
        "function breakpoint should be verified: {:?}", bps[0]);
    assert!(bps[0]["line"].as_i64().unwrap() > 0,
        "function breakpoint should have a line number: {:?}", bps[0]);

    send_dap_message(&mut stdin, &json!({
        "seq": 4,
        "type": "request",
        "command": "disconnect",
    }));
    loop {
        let msg = read_dap_message(&mut reader);
        if msg["command"] == "disconnect" { break; }
    }
    let _ = child.wait();
}

// ---------------------------------------------------------------------------
// --trace mode tests
// ---------------------------------------------------------------------------

#[test]
fn trace_writes_markdown_output() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("hello.sh");
    std::fs::write(&script, "#!/usr/bin/env bash\ngreet() {\n  local name=\"${1:-world}\"\n  echo \"Hello, ${name}!\"\n}\ngreet \"$@\"\n").unwrap();

    let output = dir.path().join("trace.md");

    let result = Command::new(bin())
        .args(["--trace", output.to_str().unwrap(), "--", script.to_str().unwrap(), "Alice"])
        .output()
        .expect("run argsh-dap --trace");

    assert!(
        result.status.success(),
        "trace command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    assert!(output.exists(), "trace output file should exist");

    let content = std::fs::read_to_string(&output).unwrap();

    // Verify markdown structure
    assert!(content.starts_with("# Process Trace:"), "should start with header, got: {}", &content[..80.min(content.len())]);
    assert!(content.contains("hello.sh"), "should reference the script name");
    assert!(content.contains("Alice"), "should include the script args");
    assert!(content.contains("## Execution"), "should have Execution section");
    assert!(content.contains("## Summary"), "should have Summary section");
    assert!(content.contains("Exit code"), "should report exit code");
    assert!(content.contains("Wall time"), "should report wall time");
}

#[test]
fn trace_captures_function_calls() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("funcs.sh");
    std::fs::write(&script, "#!/usr/bin/env bash\ninner() {\n  echo \"inner\"\n}\nouter() {\n  inner\n  echo \"outer\"\n}\nouter\n").unwrap();

    let output = dir.path().join("trace.md");

    let result = Command::new(bin())
        .args(["--trace", output.to_str().unwrap(), "--", script.to_str().unwrap()])
        .output()
        .expect("run argsh-dap --trace");

    assert!(
        result.status.success(),
        "trace command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let content = std::fs::read_to_string(&output).unwrap();

    // Should capture function entries
    assert!(content.contains("outer"), "should trace outer function");
    assert!(content.contains("inner"), "should trace inner function");
    assert!(content.contains("Functions called"), "should report function call count");
}

#[test]
fn trace_missing_script_fails() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("trace.md");
    let missing = dir.path().join("nonexistent.sh");

    let result = Command::new(bin())
        .args(["--trace", output.to_str().unwrap(), "--", missing.to_str().unwrap()])
        .output()
        .expect("run argsh-dap --trace");

    assert!(!result.status.success(), "should fail for missing script");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("not found"), "should report script not found: {}", stderr);
}

#[test]
fn trace_no_separator_fails() {
    let result = Command::new(bin())
        .args(["--trace", "out.md", "script.sh"])
        .output()
        .expect("run argsh-dap --trace");

    assert!(!result.status.success(), "should fail without -- separator");
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn trace_reports_nonzero_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fail.sh");
    std::fs::write(&script, "#!/usr/bin/env bash\nexit 42\n").unwrap();

    let output = dir.path().join("trace.md");

    let result = Command::new(bin())
        .args(["--trace", output.to_str().unwrap(), "--", script.to_str().unwrap()])
        .output()
        .expect("run argsh-dap --trace");

    // The trace itself should succeed even if the script exits non-zero
    assert!(result.status.success(), "trace command should succeed");

    let content = std::fs::read_to_string(&output).unwrap();
    assert!(content.contains("42"), "should report exit code 42 in trace");
}
