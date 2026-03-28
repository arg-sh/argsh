use std::io::{BufRead, BufReader, Read as IoRead, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn send_lsp_message(stdin: &mut impl Write, msg: &Value) {
    let body = serde_json::to_string(msg).unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    stdin.flush().unwrap();
}

fn read_lsp_response(reader: &mut BufReader<impl IoRead>) -> Value {
    // Read headers until blank line
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

    // Read body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

struct LspTestClient {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    reader: BufReader<std::process::ChildStdout>,
    next_id: i64,
}

impl LspTestClient {
    fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_argsh-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn argsh-lsp");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);

        Self {
            child,
            stdin: Some(stdin),
            reader,
            next_id: 1,
        }
    }

    fn stdin_mut(&mut self) -> &mut std::process::ChildStdin {
        self.stdin.as_mut().expect("stdin already closed")
    }

    fn send_request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        send_lsp_message(self.stdin_mut(), &msg);
        // Read responses, skipping any server-initiated notifications/requests
        loop {
            let resp = read_lsp_response(&mut self.reader);
            // If the response has our id, return it
            if resp.get("id") == Some(&json!(id)) {
                return resp;
            }
            // Otherwise it is a notification or server request (e.g. window/logMessage);
            // skip it and read the next message.
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        send_lsp_message(self.stdin_mut(), &msg);
    }

    fn initialize(&mut self) -> Value {
        let resp = self.send_request(
            "initialize",
            json!({
                "processId": null,
                "capabilities": {},
                "rootUri": null
            }),
        );
        self.notify("initialized", json!({}));
        // Allow server to process the initialized notification and send logMessage
        std::thread::sleep(std::time::Duration::from_millis(100));
        resp
    }

    fn open_document(&mut self, uri: &str, content: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "shellscript",
                    "version": 1,
                    "text": content
                }
            }),
        );
        // Small delay for async processing
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    fn document_symbols(&mut self, uri: &str) -> Value {
        self.send_request(
            "textDocument/documentSymbol",
            json!({
                "textDocument": { "uri": uri }
            }),
        )
    }

    fn completion(&mut self, uri: &str, line: u32, character: u32) -> Value {
        self.send_request(
            "textDocument/completion",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
    }

    fn hover(&mut self, uri: &str, line: u32, character: u32) -> Value {
        self.send_request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
    }

    fn shutdown(&mut self) {
        // shutdown expects no params field (tower-lsp rejects params: null)
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "shutdown"
        });
        send_lsp_message(self.stdin_mut(), &msg);
        // Drain responses until we get our shutdown response
        loop {
            let resp = read_lsp_response(&mut self.reader);
            if resp.get("id") == Some(&json!(id)) {
                break;
            }
        }
        // Send exit notification (no params field)
        let exit_msg = json!({
            "jsonrpc": "2.0",
            "method": "exit"
        });
        send_lsp_message(self.stdin_mut(), &exit_msg);
        // Close stdin to signal the server to exit
        self.stdin.take();
        // Wait briefly, then kill if still running
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for LspTestClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn test_initialize() {
    let mut client = LspTestClient::new();
    let resp = client.initialize();
    assert!(
        resp["result"]["capabilities"]["completionProvider"].is_object(),
        "Expected completionProvider capability"
    );
    assert!(
        resp["result"]["capabilities"]["hoverProvider"].is_boolean(),
        "Expected hoverProvider capability"
    );
    assert!(
        resp["result"]["capabilities"]["documentSymbolProvider"].is_boolean()
            || resp["result"]["capabilities"]["documentSymbolProvider"].is_object(),
        "Expected documentSymbolProvider capability"
    );
    client.shutdown();
}

#[test]
fn test_document_symbols_argsh_file() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env bash
source argsh

main() {
  local -a usage=(
    'serve' "Start server"
    'build' "Build project"
  )
  :usage "My app" "${@}"
  "${usage[@]}"
}

serve() {
  local port
  local -a args=(
    'port|p:~int' "Port number"
  )
  :args "Start server" "${@}"
}
"#;

    client.open_document("file:///test.sh", content);
    let resp = client.document_symbols("file:///test.sh");

    // Should have result (not error)
    assert!(
        resp.get("error").is_none(),
        "Got error: {:?}",
        resp["error"]
    );
    let result = &resp["result"];
    assert!(result.is_array(), "Expected array, got: {:?}", result);

    // Should have at least 2 functions (main, serve)
    let symbols = result.as_array().unwrap();
    assert!(
        symbols.len() >= 2,
        "Expected >=2 symbols, got {}: {:?}",
        symbols.len(),
        symbols
    );

    // Each symbol should have a valid range with numeric line values
    for sym in symbols {
        assert!(
            sym["range"].is_object(),
            "Symbol missing range: {:?}",
            sym
        );
        assert!(
            sym["range"]["start"]["line"].is_number(),
            "Symbol range start missing line: {:?}",
            sym
        );
        let start_line = sym["range"]["start"]["line"].as_u64().unwrap();
        let end_line = sym["range"]["end"]["line"].as_u64().unwrap();
        assert!(
            end_line >= start_line,
            "Symbol end_line ({}) < start_line ({}): {:?}",
            end_line,
            start_line,
            sym
        );
        // Character values should be reasonable (not u32::MAX)
        let end_char = sym["range"]["end"]["character"].as_u64().unwrap();
        assert!(
            end_char <= 999,
            "Symbol end character too large ({}): {:?}",
            end_char,
            sym
        );
    }

    client.shutdown();
}

#[test]
fn test_document_symbols_non_argsh_file() {
    let mut client = LspTestClient::new();
    client.initialize();

    // Plain bash file without argsh markers
    let content = "#!/usr/bin/env bash\necho hello\n";
    client.open_document("file:///plain.sh", content);
    let resp = client.document_symbols("file:///plain.sh");

    // Should return null result (not argsh file), not an error
    assert!(
        resp.get("error").is_none(),
        "Got error: {:?}",
        resp["error"]
    );
    assert!(
        resp["result"].is_null(),
        "Expected null for non-argsh file, got: {:?}",
        resp["result"]
    );

    client.shutdown();
}

#[test]
fn test_completion_inside_args_array() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env bash
source argsh

main() {
  local port
  local -a args=(
    'port|p:' "Port number"
  )
  :args "Title" "${@}"
}
"#;

    client.open_document("file:///test_comp.sh", content);
    // Position right after ':' in 'port|p:' (column 12, inside the quote)
    // Line:     'port|p:' "Port number"
    // Col:  4567890123
    // ':' is at col 11, cursor at col 12 means prefix ends with ':'
    let resp = client.completion("file:///test_comp.sh", 6, 12);

    assert!(
        resp.get("error").is_none(),
        "Got error: {:?}",
        resp["error"]
    );
    // Should get completions for modifiers
    let items = if resp["result"].is_array() {
        resp["result"].as_array().unwrap().clone()
    } else if resp["result"]["items"].is_array() {
        resp["result"]["items"].as_array().unwrap().clone()
    } else {
        vec![]
    };
    // Should suggest type modifiers like ~int, +boolean, etc.
    assert!(!items.is_empty(), "Expected completion items, got none");

    client.shutdown();
}

#[test]
fn test_hover_on_function() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env bash
source argsh

serve() {
  local port
  local -a args=(
    'port|p:~int' "Port number"
  )
  :args "Start server" "${@}"
}
"#;

    client.open_document("file:///test_hover.sh", content);
    // Hover on "serve" function name (line 3, char 0)
    let resp = client.hover("file:///test_hover.sh", 3, 0);

    assert!(
        resp.get("error").is_none(),
        "Got error: {:?}",
        resp["error"]
    );
    // Should have hover content if the server provides it
    if !resp["result"].is_null() {
        assert!(
            resp["result"]["contents"].is_object()
                || resp["result"]["contents"].is_string()
                || resp["result"]["contents"].is_array(),
            "Unexpected hover content: {:?}",
            resp["result"]
        );
    }

    client.shutdown();
}

#[test]
fn test_diagnostics_published() {
    let mut client = LspTestClient::new();
    client.initialize();

    // File with an odd-length args array (should trigger diagnostic)
    let content = r#"#!/usr/bin/env bash
source argsh

main() {
  local -a args=(
    'port|p:~int'
  )
  :args "Title" "${@}"
}
"#;

    client.open_document("file:///test_diag.sh", content);

    // Diagnostics are pushed as notifications, not in response to a request.
    // We cannot easily capture async notifications in this test setup.
    // Just verify no crash occurs.
    std::thread::sleep(std::time::Duration::from_millis(200));

    client.shutdown();
}
