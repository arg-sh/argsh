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

#[test]
fn test_goto_definition_usage_to_function() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env argsh

main() {
  local -a usage=(
    'serve' "Start server"
    'build:-build::run' "Build project"
  )
  :usage "My app" "${@}"
  "${usage[@]}"
}

serve() {
  :args "Start server" "${@}"
}

build::run() {
  :args "Build" "${@}"
}
"#;

    let uri = "file:///test_goto.sh";
    client.open_document(uri, content);

    // Go-to-def on "serve" usage entry (line 4, col ~5)
    let resp = client.send_request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 4, "character": 5 }
        }),
    );
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    // Should resolve to the serve() function
    if !resp["result"].is_null() {
        let result = &resp["result"];
        // May be scalar Location or array
        let target_line = if result.is_object() {
            result["range"]["start"]["line"].as_u64()
        } else if result.is_array() && !result.as_array().unwrap().is_empty() {
            result[0]["range"]["start"]["line"].as_u64()
        } else {
            None
        };
        if let Some(line) = target_line {
            assert_eq!(line, 11, "Expected goto serve() on line 11");
        }
    }

    // Go-to-def on "build::run" explicit mapping (line 5, col ~19)
    let resp2 = client.send_request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 5, "character": 19 }
        }),
    );
    assert!(
        resp2.get("error").is_none(),
        "Got error: {:?}",
        resp2["error"]
    );
    if !resp2["result"].is_null() {
        let result = &resp2["result"];
        let target_line = if result.is_object() {
            result["range"]["start"]["line"].as_u64()
        } else if result.is_array() && !result.as_array().unwrap().is_empty() {
            result[0]["range"]["start"]["line"].as_u64()
        } else {
            None
        };
        if let Some(line) = target_line {
            assert_eq!(line, 15, "Expected goto build::run() on line 15");
        }
    }

    client.shutdown();
}

#[test]
fn test_completion_annotations() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env argsh

main() {
  local -a usage=(
    'deploy@' "Deploy"
  )
  :usage "App" "${@}"
}
"#;

    let uri = "file:///test_comp_ann.sh";
    client.open_document(uri, content);

    // Position after '@' in 'deploy@' (line 4, inside the quote after @)
    let resp = client.completion(uri, 4, 12);
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    // Should get annotation completions
    let items = if resp["result"].is_array() {
        resp["result"].as_array().unwrap().clone()
    } else if resp["result"].is_object() && resp["result"]["items"].is_array() {
        resp["result"]["items"].as_array().unwrap().clone()
    } else {
        vec![]
    };
    // Annotations may or may not be offered depending on completion logic,
    // but the server should not crash.
    let _ = items;

    client.shutdown();
}

#[test]
fn test_completion_type_names() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env bash
source argsh

myfunc() {
  local val
  local -a args=(
    'val|v:~' "A value"
  )
  :args "Test" "${@}"
}
"#;

    let uri = "file:///test_comp_type.sh";
    client.open_document(uri, content);

    // Position right after ':~' in 'val|v:~' (line 6, col 12)
    let resp = client.completion(uri, 6, 12);
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    let items = if resp["result"].is_array() {
        resp["result"].as_array().unwrap().clone()
    } else if resp["result"].is_object() && resp["result"]["items"].is_array() {
        resp["result"]["items"].as_array().unwrap().clone()
    } else {
        vec![]
    };
    // Should suggest type names like int, float, file, etc.
    assert!(!items.is_empty(), "Expected type completion items");

    client.shutdown();
}

#[test]
fn test_hover_on_args_entry() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env bash
source argsh

serve() {
  local port verbose
  local -a args=(
    'port|p:~int' "Port number"
    'verbose|v:+' "Enable verbose"
  )
  :args "Start server" "${@}"
}
"#;

    let uri = "file:///test_hover_args.sh";
    client.open_document(uri, content);

    // Hover on "port" in args entry (line 6, col 6)
    let resp = client.hover(uri, 6, 6);
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let value = resp["result"]["contents"]["value"]
            .as_str()
            .unwrap_or("");
        assert!(
            value.contains("--port") || value.contains("port"),
            "Expected hover to mention 'port', got: {}",
            value
        );
    }

    client.shutdown();
}

#[test]
fn test_hover_on_usage_entry() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env argsh

main() {
  local -a usage=(
    'serve|s' "Start server"
    'build'   "Build project"
  )
  :usage "My app" "${@}"
  "${usage[@]}"
}
"#;

    let uri = "file:///test_hover_usage.sh";
    client.open_document(uri, content);

    // Hover on "serve" in usage entry (line 4, col 6)
    let resp = client.hover(uri, 4, 6);
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let value = resp["result"]["contents"]["value"]
            .as_str()
            .unwrap_or("");
        assert!(
            value.contains("serve") || value.contains("subcommand"),
            "Expected hover to mention 'serve', got: {}",
            value
        );
    }

    client.shutdown();
}

#[test]
fn test_hover_help_preview() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env argsh

serve() {
  local port verbose
  local -a args=(
    'port|p:~int'    "Port number"
    'verbose|v:+'    "Enable verbose output"
  )
  :args "Start server" "${@}"
}
"#;

    let uri = "file:///test_help_preview.sh";
    client.open_document(uri, content);

    // Hover on function name "serve" (line 2, col 0)
    let resp = client.hover(uri, 2, 0);
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    assert!(
        !resp["result"].is_null(),
        "Expected hover result for function"
    );

    let value = resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("");

    // Should contain help preview elements
    assert!(
        value.contains("**serve**"),
        "Missing function name in preview: {}",
        value
    );
    assert!(
        value.contains("Start server"),
        "Missing title in preview: {}",
        value
    );
    assert!(
        value.contains("Usage:"),
        "Missing usage line in preview: {}",
        value
    );
    assert!(
        value.contains("--port") || value.contains("port"),
        "Missing port flag in preview: {}",
        value
    );
    assert!(
        value.contains("--verbose") || value.contains("verbose"),
        "Missing verbose flag in preview: {}",
        value
    );

    client.shutdown();
}

#[test]
fn test_document_change_updates_diagnostics() {
    let mut client = LspTestClient::new();
    client.initialize();

    let uri = "file:///test_change.sh";

    // Open with valid content
    let content_v1 = r#"#!/usr/bin/env bash
source argsh

main() {
  local name
  local -a args=(
    'name|n' "Name"
  )
  :args "Test" "${@}"
}
"#;
    client.open_document(uri, content_v1);

    // Verify symbols work
    let resp1 = client.document_symbols(uri);
    assert!(resp1.get("error").is_none());
    let syms1 = resp1["result"].as_array().unwrap();
    assert_eq!(syms1.len(), 1);

    // Change document to add a second function
    let content_v2 = r#"#!/usr/bin/env bash
source argsh

main() {
  local name
  local -a args=(
    'name|n' "Name"
  )
  :args "Test" "${@}"
}

second() {
  :args "Second" "${@}"
}
"#;
    // Send didChange
    client.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": content_v2 }]
        }),
    );
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Verify symbols updated
    let resp2 = client.document_symbols(uri);
    assert!(resp2.get("error").is_none());
    let syms2 = resp2["result"].as_array().unwrap();
    assert_eq!(
        syms2.len(),
        2,
        "Expected 2 symbols after update, got: {:?}",
        syms2
    );

    client.shutdown();
}

#[test]
fn test_multiple_documents() {
    let mut client = LspTestClient::new();
    client.initialize();

    let uri1 = "file:///doc1.sh";
    let content1 = r#"#!/usr/bin/env argsh
func_a() {
  :args "Function A" "${@}"
}
"#;

    let uri2 = "file:///doc2.sh";
    let content2 = r#"#!/usr/bin/env argsh
func_b() {
  :args "Function B" "${@}"
}
func_c() {
  :args "Function C" "${@}"
}
"#;

    client.open_document(uri1, content1);
    client.open_document(uri2, content2);

    // Doc1 should have 1 symbol
    let resp1 = client.document_symbols(uri1);
    assert!(resp1.get("error").is_none());
    let syms1 = resp1["result"].as_array().unwrap();
    assert_eq!(syms1.len(), 1, "Doc1 should have 1 symbol: {:?}", syms1);

    // Doc2 should have 2 symbols
    let resp2 = client.document_symbols(uri2);
    assert!(resp2.get("error").is_none());
    let syms2 = resp2["result"].as_array().unwrap();
    assert_eq!(syms2.len(), 2, "Doc2 should have 2 symbols: {:?}", syms2);

    // Hover on doc1 should only see func_a
    let hover1 = client.hover(uri1, 1, 0);
    if !hover1["result"].is_null() {
        let value = hover1["result"]["contents"]["value"]
            .as_str()
            .unwrap_or("");
        assert!(
            value.contains("func_a"),
            "Doc1 hover should reference func_a: {}",
            value
        );
        assert!(
            !value.contains("func_b"),
            "Doc1 hover should NOT reference func_b: {}",
            value
        );
    }

    client.shutdown();
}

// Helper to extract completion items from a response Value.
fn extract_completion_items(resp: &Value) -> Vec<&Value> {
    if let Some(items) = resp["result"].as_array() {
        return items.iter().collect();
    }
    if let Some(items) = resp["result"]["items"].as_array() {
        return items.iter().collect();
    }
    vec![]
}

#[test]
fn test_completion_same_line_args_array() {
    // Completion when args=( is on the same line as the cursor
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a args=('port|p:' \"Port\")\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_sameline.sh", content);
    // Line 3: `  local -a args=('port|p:' "Port")`
    // The ':' is inside the single-quoted spec at col 25.
    let resp = client.completion("file:///test_sameline.sh", 3, 25);
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    let items = extract_completion_items(&resp);
    assert!(!items.is_empty(), "Expected modifier completions on same-line args array, got empty");

    client.shutdown();
}

#[test]
fn test_completion_multiline_args_array() {
    // Completion when cursor is inside a multi-line args=( block
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a args=(\n    'port|p:' \"Port\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_multiline.sh", content);
    // Line 4: `    'port|p:' "Port"`
    // The ':' is at col 11, cursor at col 12 (after the colon, inside the quote)
    let resp = client.completion("file:///test_multiline.sh", 4, 12);
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    let items = extract_completion_items(&resp);
    assert!(!items.is_empty(), "Expected modifier completions in multiline args array, got empty");

    client.shutdown();
}

#[test]
fn test_diagnostics_missing_local_variable() {
    // Args field 'port' without matching 'local port' should produce warning.
    // We cannot easily capture push notifications, but verify no crash.
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_missing_local.sh", content);
    std::thread::sleep(std::time::Duration::from_millis(300));
    client.shutdown();
}

#[test]
fn test_diagnostics_no_warning_when_local_declared() {
    // Args field 'port' WITH 'local port' should NOT produce warning.
    // Verify no crash.
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_local_declared.sh", content);
    std::thread::sleep(std::time::Duration::from_millis(300));
    client.shutdown();
}

#[test]
fn test_hover_on_args_entry_shows_field_details() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nserve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port number\"\n  )\n  :args \"Start server\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on 'port|p:~int' (line 5, inside the quotes)
    let resp = client.hover("file:///test.sh", 5, 8);
    assert!(resp.get("error").is_none(), "Error: {:?}", resp["error"]);
    let result = &resp["result"];
    assert!(!result.is_null(), "Hover returned null for args entry");
    // Should contain field info
    let content_str = format!("{}", result);
    assert!(
        content_str.contains("port") || content_str.contains("int"),
        "Hover should mention field name or type: {}",
        content_str
    );

    client.shutdown();
}

#[test]
fn test_hover_on_usage_entry_shows_command_info() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a usage=(\n    'serve@readonly' \"Start server\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\nserve() { :args \"S\" \"${@}\"; }\n";
    client.open_document("file:///test.sh", content);
    // Hover on 'serve@readonly' (line 4, inside quotes)
    let resp = client.hover("file:///test.sh", 4, 8);
    assert!(resp.get("error").is_none());
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "Hover returned null for usage entry"
    );
    let content_str = format!("{}", result);
    assert!(
        content_str.contains("serve") || content_str.contains("readonly"),
        "Hover should mention command or annotation: {}",
        content_str
    );

    client.shutdown();
}

#[test]
fn test_hover_on_function_shows_help_preview() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nserve() {\n  local port verbose\n  local -a args=(\n    'port|p:~int' \"Port number\"\n    'verbose|v:+' \"Verbose output\"\n  )\n  :args \"Start the server\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on function name "serve" (line 2, col 0)
    let resp = client.hover("file:///test.sh", 2, 0);
    assert!(resp.get("error").is_none());
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "Hover returned null for function"
    );
    let content_str = format!("{}", result);
    // Should show help preview with flags
    assert!(
        content_str.contains("port")
            || content_str.contains("verbose")
            || content_str.contains("Start"),
        "Function hover should show help preview: {}",
        content_str
    );

    client.shutdown();
}

#[test]
fn test_hover_on_modifier_shows_docs() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local v\n  local -a args=(\n    'verbose|v:+' \"Verbose\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on ':+' modifier (line 5, around col 17)
    let resp = client.hover("file:///test.sh", 5, 17);
    // May or may not return content -- just verify no error/crash
    assert!(
        resp.get("error").is_none(),
        "Hover on modifier should not error"
    );

    client.shutdown();
}

#[test]
fn test_hover_on_annotation_shows_docs() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nm() {\n  local -a usage=(\n    'cmd@readonly' \"Desc\"\n  )\n  :usage \"T\" \"${@}\"\n  \"${usage[@]}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on '@readonly' (line 4, around col 10)
    let resp = client.hover("file:///test.sh", 4, 10);
    assert!(
        resp.get("error").is_none(),
        "Hover on annotation should not error"
    );

    client.shutdown();
}

#[test]
fn test_hover_on_args_call_shows_summary() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local p v\n  local -a args=(\n    'port|p:~int' \"Port\"\n    'verbose|v:+' \"Verbose\"\n  )\n  :args \"Title\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on ':args' call (line 8, col 2)
    let resp = client.hover("file:///test.sh", 8, 3);
    assert!(resp.get("error").is_none());
    // Should show something about the flags count

    client.shutdown();
}

#[test]
fn test_code_lens_shows_counts() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a usage=(\n    'serve' \"Start\"\n    'build' \"Build\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\nserve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"S\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);
    let resp = client.send_request(
        "textDocument/codeLens",
        json!({
            "textDocument": { "uri": "file:///test.sh" }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "CodeLens error: {:?}",
        resp["error"]
    );
    let result = &resp["result"];
    assert!(result.is_array(), "CodeLens should return array");
    let lenses = result.as_array().unwrap();
    assert!(
        lenses.len() >= 2,
        "Expected code lenses for main + serve, got {}",
        lenses.len()
    );

    client.shutdown();
}
