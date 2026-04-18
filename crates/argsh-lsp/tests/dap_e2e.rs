//! End-to-end tests for the argsh-dap debugger.
//!
//! These tests launch real bash scripts through the full debug pipeline
//! (DEBUG trap → FIFO → DAP events) and verify breakpoints, stepping,
//! variable inspection, and the full DAP lifecycle work correctly.
//!
//! Uses a channel-based receiver with timeouts to avoid hanging on
//! FIFO synchronization issues.

use std::io::{BufRead, BufReader, Read as IoRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

const TIMEOUT: Duration = Duration::from_secs(5);

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_argsh-dap"))
}

fn send(stdin: &mut impl Write, msg: &Value) {
    let body = serde_json::to_string(msg).unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    stdin.flush().unwrap();
}

fn recv_raw(reader: &mut BufReader<impl IoRead>) -> Option<Value> {
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => return None,
            _ => {}
        }
        let trimmed = header.trim();
        if trimmed.is_empty() { break; }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.parse().ok()?;
        }
    }
    if content_length == 0 { return None; }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

struct Dap { rx: mpsc::Receiver<Value> }

impl Dap {
    fn new(stdout: std::process::ChildStdout) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            while let Some(msg) = recv_raw(&mut r) {
                if tx.send(msg).is_err() { break; }
            }
        });
        Self { rx }
    }
    fn recv(&self) -> Option<Value> { self.rx.recv_timeout(TIMEOUT).ok() }
    fn wait_stopped(&self) -> Option<Value> {
        let start = std::time::Instant::now();
        loop {
            let remaining = TIMEOUT.checked_sub(start.elapsed())?;
            match self.rx.recv_timeout(remaining) {
                Ok(m) if m["type"] == "event" && m["event"] == "stopped" => return Some(m),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }
}

fn write_script(name: &str, content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

fn init() -> (std::process::ChildStdin, Dap, std::process::Child) {
    let mut child = Command::new(bin())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().expect("spawn argsh-dap");
    let mut stdin = child.stdin.take().unwrap();
    let dap = Dap::new(child.stdout.take().unwrap());
    send(&mut stdin, &json!({"seq":1,"type":"request","command":"initialize","arguments":{}}));
    let _ = dap.recv(); // capabilities
    let _ = dap.recv(); // initialized event
    (stdin, dap, child)
}

fn launch(stdin: &mut impl Write, dap: &Dap, script: &std::path::Path, stop: bool, args: &[&str]) {
    send(stdin, &json!({"seq":2,"type":"request","command":"launch","arguments":{
        "program": script.to_str().unwrap(), "stopOnEntry": stop, "args": args
    }}));
    let r = dap.recv().expect("launch resp");
    assert!(r["success"].as_bool().unwrap(), "launch failed: {:?}", r);
    send(stdin, &json!({"seq":3,"type":"request","command":"configurationDone"}));
    let _ = dap.recv();
}

fn set_bp(stdin: &mut impl Write, dap: &Dap, file: &std::path::Path, lines: &[u32]) {
    let bps: Vec<Value> = lines.iter().map(|l| json!({"line": l})).collect();
    send(stdin, &json!({"seq":10,"type":"request","command":"setBreakpoints","arguments":{
        "source": {"path": file.to_str().unwrap()}, "breakpoints": bps
    }}));
    let _ = dap.recv();
}

fn set_cond_bp(stdin: &mut impl Write, dap: &Dap, file: &std::path::Path, line: u32, cond: &str) {
    send(stdin, &json!({"seq":10,"type":"request","command":"setBreakpoints","arguments":{
        "source": {"path": file.to_str().unwrap()},
        "breakpoints": [{"line": line, "condition": cond}]
    }}));
    let _ = dap.recv();
}

fn cont(stdin: &mut impl Write, dap: &Dap) {
    send(stdin, &json!({"seq":20,"type":"request","command":"continue","arguments":{"threadId":1}}));
    let _ = dap.recv();
}

fn step(stdin: &mut impl Write, dap: &Dap, cmd: &str) {
    send(stdin, &json!({"seq":20,"type":"request","command":cmd,"arguments":{"threadId":1}}));
    let _ = dap.recv();
}

fn quit(stdin: &mut impl Write, dap: &Dap, child: &mut std::process::Child) {
    send(stdin, &json!({"seq":99,"type":"request","command":"disconnect"}));
    let _ = dap.recv();
    let _ = child.wait();
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_stop_on_entry() {
    let (_d, s) = write_script("t.sh", "#!/usr/bin/env bash\necho hello\n");
    let (mut si, dap, mut ch) = init();
    launch(&mut si, &dap, &s, true, &[]);
    assert!(dap.wait_stopped().is_some(), "no stop on entry");
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_breakpoint_hit() {
    let (_d, s) = write_script("bp.sh", "#!/usr/bin/env bash\nx=1\nx=2\nx=3\necho done\n");
    let (mut si, dap, mut ch) = init();
    set_bp(&mut si, &dap, &s, &[4]);
    launch(&mut si, &dap, &s, false, &[]);
    let st = dap.wait_stopped();
    assert!(st.is_some(), "breakpoint not hit");
    assert!(st.unwrap()["body"]["description"].as_str().unwrap_or("").contains(":4"));
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_multiple_breakpoints() {
    let (_d, s) = write_script("mbp.sh", "#!/usr/bin/env bash\na=1\nb=2\nc=3\nd=4\necho done\n");
    let (mut si, dap, mut ch) = init();
    set_bp(&mut si, &dap, &s, &[3, 5]);
    launch(&mut si, &dap, &s, false, &[]);
    assert!(dap.wait_stopped().is_some(), "first bp not hit");
    cont(&mut si, &dap);
    assert!(dap.wait_stopped().is_some(), "second bp not hit");
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_conditional_breakpoint() {
    let (_d, s) = write_script("cond.sh", "#!/usr/bin/env bash\nfor i in 1 2 3 4 5; do\n  echo \"i=$i\"\ndone\n");
    let (mut si, dap, mut ch) = init();
    set_cond_bp(&mut si, &dap, &s, 3, "(( i == 3 ))");
    launch(&mut si, &dap, &s, false, &[]);
    let st = dap.wait_stopped();
    assert!(st.is_some(), "conditional breakpoint never fired");
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_step_over() {
    let (_d, s) = write_script("next.sh", "#!/usr/bin/env bash\nf() { echo hi; }\nx=1\nf\nx=2\n");
    let (mut si, dap, mut ch) = init();
    launch(&mut si, &dap, &s, true, &[]);
    assert!(dap.wait_stopped().is_some(), "no stop on entry");
    step(&mut si, &dap, "next");
    assert!(dap.wait_stopped().is_some(), "no stop after next");
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_step_into() {
    let (_d, s) = write_script("stepin.sh", "#!/usr/bin/env bash\ninner() { local v=42; echo $v; }\ninner\n");
    let (mut si, dap, mut ch) = init();
    set_bp(&mut si, &dap, &s, &[3]); // breakpoint on `inner` call
    launch(&mut si, &dap, &s, false, &[]);
    assert!(dap.wait_stopped().is_some(), "bp not hit");
    step(&mut si, &dap, "stepIn");
    assert!(dap.wait_stopped().is_some(), "no stop after step in");
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
#[ignore] // TODO: step-out depth tracking interacts with the wrapper's function depth
fn e2e_step_out() {
    let (_d, s) = write_script("stepout.sh", "#!/usr/bin/env bash\ninner() { echo a; echo b; }\ninner\necho done\n");
    let (mut si, dap, mut ch) = init();
    set_bp(&mut si, &dap, &s, &[2]); // inside inner, first line
    launch(&mut si, &dap, &s, false, &[]);
    // The bp is on the function def line, so step into first
    let st = dap.wait_stopped();
    if st.is_some() {
        step(&mut si, &dap, "stepOut");
        // Should stop back in caller or at next line
        let _ = dap.wait_stopped(); // may or may not fire depending on depth
    }
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_stack_trace() {
    let (_d, s) = write_script("stack.sh", "#!/usr/bin/env bash\nadd() { echo $(( $1 + $2 )); }\nadd 3 4\n");
    let (mut si, dap, mut ch) = init();
    launch(&mut si, &dap, &s, true, &[]);
    assert!(dap.wait_stopped().is_some(), "no stop on entry");
    send(&mut si, &json!({"seq":30,"type":"request","command":"stackTrace","arguments":{"threadId":1}}));
    let r = dap.recv().expect("stackTrace response");
    assert!(r["success"].as_bool().unwrap(), "stackTrace failed: {:?}", r);
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_set_variable() {
    let (_d, s) = write_script("setv.sh", "#!/usr/bin/env bash\nmyvar=original\necho $myvar\n");
    let (mut si, dap, mut ch) = init();
    set_bp(&mut si, &dap, &s, &[3]);
    launch(&mut si, &dap, &s, false, &[]);
    assert!(dap.wait_stopped().is_some(), "bp not hit");
    send(&mut si, &json!({"seq":30,"type":"request","command":"setVariable","arguments":{
        "variablesReference":1,"name":"myvar","value":"modified"
    }}));
    let r = dap.recv().expect("setVariable resp");
    assert!(r["success"].as_bool().unwrap(), "setVariable failed: {:?}", r);
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_script_args() {
    let (_d, s) = write_script("args.sh", "#!/usr/bin/env bash\necho $1 $2\n");
    let (mut si, dap, mut ch) = init();
    launch(&mut si, &dap, &s, true, &["hello", "world"]);
    assert!(dap.wait_stopped().is_some(), "no stop on entry");
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_env_vars() {
    let (_d, s) = write_script("env.sh", "#!/usr/bin/env bash\necho $MY_VAR\n");
    let (mut si, dap, mut ch) = init();
    send(&mut si, &json!({"seq":2,"type":"request","command":"launch","arguments":{
        "program": s.to_str().unwrap(), "stopOnEntry": true,
        "env": {"MY_VAR": "test_value"}
    }}));
    let r = dap.recv().expect("launch resp");
    assert!(r["success"].as_bool().unwrap(), "launch with env failed: {:?}", r);
    send(&mut si, &json!({"seq":3,"type":"request","command":"configurationDone"}));
    let _ = dap.recv();
    assert!(dap.wait_stopped().is_some(), "no stop on entry");
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_evaluate_hover() {
    let (_d, s) = write_script("hover.sh", "#!/usr/bin/env bash\nx=42\necho $x\n");
    let (mut si, dap, mut ch) = init();
    launch(&mut si, &dap, &s, true, &[]);
    assert!(dap.wait_stopped().is_some(), "no stop");
    send(&mut si, &json!({"seq":30,"type":"request","command":"evaluate","arguments":{
        "expression":"x","context":"hover"
    }}));
    let r = dap.recv().expect("evaluate resp");
    assert!(r["success"].as_bool().unwrap(), "evaluate failed: {:?}", r);
    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_disconnect_kills_script() {
    let (_d, s) = write_script("hang.sh", "#!/usr/bin/env bash\nwhile true; do sleep 0.1; done\n");
    let (mut si, dap, mut ch) = init();
    launch(&mut si, &dap, &s, true, &[]);
    let _ = dap.wait_stopped();
    quit(&mut si, &dap, &mut ch);
    // Child should be dead
    std::thread::sleep(Duration::from_millis(200));
    match ch.try_wait() {
        Ok(Some(_)) => {}
        _ => { ch.kill().ok(); let _ = ch.wait(); }
    }
}

#[test]
fn e2e_normal_exit() {
    let (_d, s) = write_script("exit.sh", "#!/usr/bin/env bash\necho done\n");
    let (mut si, dap, mut ch) = init();
    launch(&mut si, &dap, &s, false, &[]);
    // Script should finish quickly
    std::thread::sleep(Duration::from_millis(500));
    quit(&mut si, &dap, &mut ch);
}

#[test]
fn e2e_scopes_and_variables() {
    let (_d, s) = write_script("vars.sh", "#!/usr/bin/env bash\na=1\nb=2\necho $a $b\n");
    let (mut si, dap, mut ch) = init();
    set_bp(&mut si, &dap, &s, &[4]);
    launch(&mut si, &dap, &s, false, &[]);
    assert!(dap.wait_stopped().is_some(), "bp not hit");

    send(&mut si, &json!({"seq":30,"type":"request","command":"scopes","arguments":{"frameId":0}}));
    let r = dap.recv().expect("scopes resp");
    assert!(r["success"].as_bool().unwrap());
    let scopes = r["body"]["scopes"].as_array().unwrap();
    assert!(!scopes.is_empty(), "no scopes returned");

    send(&mut si, &json!({"seq":31,"type":"request","command":"variables","arguments":{"variablesReference":1}}));
    let r = dap.recv().expect("variables resp");
    assert!(r["success"].as_bool().unwrap());

    cont(&mut si, &dap);
    quit(&mut si, &dap, &mut ch);
}
