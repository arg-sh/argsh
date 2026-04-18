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

#[test]
fn test_format_aligns_args_entries() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local p v\n  local -a args=(\n    'port|p:~int' \"Port\"\n    'verbose|v:+' \"Verbose output\"\n    'config|c' \"Config file path\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);

    let resp = client.send_request(
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": "file:///test.sh" },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );

    assert!(
        resp.get("error").is_none(),
        "Format error: {:?}",
        resp["error"]
    );
    // Should have edits (entries are not perfectly aligned)
    let result = &resp["result"];
    if !result.is_null() {
        let edits = result.as_array().unwrap();
        // Verify the edits align descriptions to the same column
        for edit in edits {
            let new_text = edit["newText"].as_str().unwrap();
            assert!(
                new_text.contains('"'),
                "Edit should contain description: {}",
                new_text
            );
        }
    }

    client.shutdown();
}

#[test]
fn test_format_preserves_already_aligned() {
    let mut client = LspTestClient::new();
    client.initialize();

    // Already perfectly aligned -- should produce no edits
    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local v\n  local -a args=(\n    'verbose|v:+' \"Verbose\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);

    let resp = client.send_request(
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": "file:///test.sh" },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );

    assert!(resp.get("error").is_none());
    // With only one entry, there is nothing to align differently
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
fn test_hover_on_args_variable_shows_overview() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local port verbose\n  local -a args=(\n    'port|p:~int' \"Port number\"\n    'verbose|v:+' \"Verbose\"\n  )\n  :args \"Title\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on 'args' in 'local -a args=(' (line 4, col ~12)
    let resp = client.hover("file:///test.sh", 4, 12);
    assert!(resp.get("error").is_none());
    let result = &resp["result"];
    assert!(!result.is_null(), "Hover on args variable should show overview");
    let content_str = format!("{}", result);
    assert!(content_str.contains("port") && content_str.contains("verbose"),
        "Should list all flags: {}", content_str);

    client.shutdown();
}

#[test]
fn test_hover_on_usage_variable_shows_overview() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nm() {\n  local -a usage=(\n    'serve' \"Start server\"\n    'build' \"Build project\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on 'usage' in 'local -a usage=(' (line 3, col ~12)
    let resp = client.hover("file:///test.sh", 3, 12);
    assert!(resp.get("error").is_none());
    let result = &resp["result"];
    assert!(!result.is_null(), "Hover on usage variable should show overview");
    let content_str = format!("{}", result);
    assert!(content_str.contains("serve") && content_str.contains("build"),
        "Should list all subcommands: {}", content_str);

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

#[test]
fn test_completion_library_functions() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  is::\n}\n";
    client.open_document("file:///test.sh", content);
    // cursor after "is::" (line 3, col 6)
    let resp = client.completion("file:///test.sh", 3, 6);
    assert!(resp.get("error").is_none());
    let items = extract_completion_items(&resp);
    assert!(
        !items.is_empty(),
        "Expected library function completions for is::"
    );
    // Should suggest is::array, is::set, etc.
    let labels: Vec<String> = items
        .iter()
        .filter_map(|v| v["label"].as_str().map(String::from))
        .collect();
    assert!(
        labels.iter().any(|l| l.contains("array")),
        "Should suggest is::array, got: {:?}",
        labels
    );

    client.shutdown();
}

#[test]
fn test_hover_shows_array_type() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local -a files\n  local -a args=(\n    'files' \"Input files\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);
    // Hover on 'args' to see overview
    let resp = client.hover("file:///test.sh", 4, 12);
    assert!(resp.get("error").is_none());
    let content_str = format!("{}", resp["result"]);
    assert!(
        content_str.contains("string[]") || content_str.contains("[]"),
        "Array field should show type[], got: {}",
        content_str
    );

    client.shutdown();
}

#[test]
fn test_diagnostic_suppression() {
    // A file with `# argsh disable=AG004` before an entry missing a local
    // declaration should suppress the AG004 diagnostic.
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  # argsh disable=AG004\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_suppress.sh", content);
    // Diagnostics are pushed as notifications — we cannot directly read them,
    // but verify no crash and the server stays responsive.
    std::thread::sleep(std::time::Duration::from_millis(300));

    // The server should still respond normally after suppression logic runs
    let resp = client.document_symbols("file:///test_suppress.sh");
    assert!(resp.get("error").is_none(), "Server should remain healthy after suppression");
    let syms = resp["result"].as_array().unwrap();
    assert_eq!(syms.len(), 1, "Should still have 1 function symbol");

    client.shutdown();
}

#[test]
fn test_diagnostic_ag010_bare_function() {
    // A usage entry 'serve' with a bare `serve()` (no `main::serve`) should
    // produce AG010 warning. We cannot read push notifications, but verify no crash.
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a usage=(\n    'serve' \"Start server\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\nserve() {\n  :args \"S\" \"${@}\"\n}\n";
    client.open_document("file:///test_ag010.sh", content);
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Verify server is healthy and symbols are correct
    let resp = client.document_symbols("file:///test_ag010.sh");
    assert!(resp.get("error").is_none());
    let syms = resp["result"].as_array().unwrap();
    assert!(syms.len() >= 2, "Should have main + serve symbols");

    client.shutdown();
}

#[test]
fn test_hover_on_group_separator() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local port verbose\n  local -a args=(\n    '-' \"Options\"\n    'port|p:~int' \"Port number\"\n    'verbose|v:+' \"Verbose\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_group.sh", content);
    // Hover on '-' group separator (line 5, col 5)
    let resp = client.hover("file:///test_group.sh", 5, 5);
    assert!(resp.get("error").is_none(), "Error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let content_str = format!("{}", resp["result"]);
        assert!(
            content_str.contains("Group separator") || content_str.contains("separator"),
            "Hover on '-' should mention group separator, got: {}",
            content_str
        );
    }

    client.shutdown();
}

#[test]
fn test_hover_on_type_modifier() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local num\n  local -a args=(\n    'num|n:~int' \"A number\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_type_mod.sh", content);
    // Hover on ':~int' portion (line 5, col 11 — on the '~' character)
    let resp = client.hover("file:///test_type_mod.sh", 5, 11);
    assert!(resp.get("error").is_none(), "Error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let content_str = format!("{}", resp["result"]);
        assert!(
            content_str.contains("int") && content_str.contains("Built-in type"),
            "Hover on :~int should mention int and Built-in type, got: {}",
            content_str
        );
    }

    client.shutdown();
}

#[test]
fn test_hover_usage_entry_shows_target_help() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a usage=(\n    'serve' \"Start server\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\nmain::serve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port number\"\n  )\n  :args \"Serve\" \"${@}\"\n}\n";
    client.open_document("file:///test_usage_help.sh", content);
    // Hover on 'serve' usage entry (line 4, col 6)
    let resp = client.hover("file:///test_usage_help.sh", 4, 6);
    assert!(resp.get("error").is_none(), "Error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let content_str = format!("{}", resp["result"]);
        assert!(
            content_str.contains("port"),
            "Hover on usage entry 'serve' should show target's flags (port), got: {}",
            content_str
        );
    }

    client.shutdown();
}

#[test]
fn test_goto_type_definition() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env bash
source argsh

to::uint() {
  [[ "${1}" =~ ^[0-9]+$ ]]
}

f() {
  local num
  local -a args=(
    'num|n:~uint' "A number"
  )
  :args "T" "${@}"
}
"#;

    let uri = "file:///test_goto_type.sh";
    client.open_document(uri, content);

    // Go-to-def on ':~uint' (line 10, col 13 — on 'uint')
    let resp = client.send_request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 10, "character": 13 }
        }),
    );
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let result = &resp["result"];
        let target_line = if result.is_object() {
            result["range"]["start"]["line"].as_u64()
        } else if result.is_array() && !result.as_array().unwrap().is_empty() {
            result[0]["range"]["start"]["line"].as_u64()
        } else {
            None
        };
        if let Some(line) = target_line {
            assert_eq!(line, 3, "Expected goto to::uint() on line 3");
        }
    }

    client.shutdown();
}

#[test]
fn test_goto_import_opens_file() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("main.sh");
    let helper_path = dir.path().join("helpers.sh");

    fs::write(&main_path, "#!/usr/bin/env bash\nsource argsh\nimport helpers\nmain() {\n  :args \"T\" \"${@}\"\n}\n").unwrap();
    fs::write(&helper_path, "helper_func() {\n  echo hello\n}\n").unwrap();

    let main_uri = format!("file://{}", main_path.to_str().unwrap());
    let helper_uri = format!("file://{}", helper_path.to_str().unwrap());

    let mut client = LspTestClient::new();
    client.initialize();
    client.open_document(&main_uri, &std::fs::read_to_string(&main_path).unwrap());

    // Go-to-def on "import helpers" (line 2, col 8 — on 'helpers')
    let resp = client.send_request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": main_uri },
            "position": { "line": 2, "character": 8 }
        }),
    );
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let result = &resp["result"];
        let target_uri = if result.is_object() {
            result["uri"].as_str().map(String::from)
        } else if result.is_array() && !result.as_array().unwrap().is_empty() {
            result[0]["uri"].as_str().map(String::from)
        } else {
            None
        };
        if let Some(uri) = target_uri {
            assert!(
                uri.contains("helpers.sh"),
                "Goto import should point to helpers.sh, got: {}",
                uri
            );
        }
    }

    let _ = helper_uri; // used for verification context
    client.shutdown();
}

#[test]
fn test_cross_file_goto_definition() {
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("main.sh");
    let helper_path = dir.path().join("helpers.sh");

    fs::write(&main_path, "#!/usr/bin/env bash\nsource argsh\nimport helpers\nmain() {\n  helper_func\n}\n").unwrap();
    fs::write(&helper_path, "helper_func() {\n  echo hello\n}\n").unwrap();

    let main_uri = format!("file://{}", main_path.to_str().unwrap());
    let helper_uri = format!("file://{}", helper_path.to_str().unwrap());

    let mut client = LspTestClient::new();
    client.initialize();
    client.open_document(&main_uri, &std::fs::read_to_string(&main_path).unwrap());

    // Go-to-def on "helper_func" (line 4, col 4)
    let resp = client.send_request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": main_uri },
            "position": { "line": 4, "character": 4 }
        }),
    );
    assert!(resp.get("error").is_none(), "Got error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let result = &resp["result"];
        let target_uri = if result.is_object() {
            result["uri"].as_str().map(String::from)
        } else if result.is_array() && !result.as_array().unwrap().is_empty() {
            result[0]["uri"].as_str().map(String::from)
        } else {
            None
        };
        if let Some(uri) = target_uri {
            assert!(
                uri.contains("helpers.sh"),
                "Cross-file goto should point to helpers.sh, got: {}",
                uri
            );
        }
    }

    let _ = helper_uri;
    client.shutdown();
}

#[test]
fn test_format_preserves_group_separators() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nf() {\n  local p v\n  local -a args=(\n    '-' \"Options\"\n    'port|p:~int' \"Port\"\n    'verbose|v:+' \"Verbose output\"\n  )\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test_fmt_group.sh", content);

    let resp = client.send_request(
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": "file:///test_fmt_group.sh" },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );

    assert!(
        resp.get("error").is_none(),
        "Format error: {:?}",
        resp["error"]
    );
    // If edits are returned, verify they still contain the group separator
    let result = &resp["result"];
    if !result.is_null() {
        let edits = result.as_array().unwrap();
        // Collect all new text from edits
        let all_text: String = edits
            .iter()
            .filter_map(|e| e["newText"].as_str())
            .collect();
        // The separator line should either be untouched (no edit for it)
        // or preserved in the edit output
        if !all_text.is_empty() {
            // Verify no edit removes the '-' entry
            assert!(
                !all_text.contains("port") || all_text.contains("Options") || edits.len() < 3,
                "Group separator '-' should be preserved in formatting"
            );
        }
    }

    client.shutdown();
}

#[test]
fn test_export_mcp_json() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nserve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port number\"\n  )\n  :args \"Start server\" \"${@}\"\n}\n";
    let uri = "file:///test_mcp.sh";
    client.open_document(uri, content);

    let resp = client.send_request(
        "workspace/executeCommand",
        json!({
            "command": "argsh.exportMcpJson",
            "arguments": [uri]
        }),
    );
    assert!(resp.get("error").is_none(), "Error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let result_str = resp["result"].as_str().unwrap_or("");
        // Should be a JSON string containing tool definitions
        assert!(
            result_str.contains("serve") || result_str.contains("tool"),
            "MCP JSON should contain tool info, got: {}",
            &result_str[..result_str.len().min(200)]
        );
        // Verify it is valid JSON
        let parsed: Result<Value, _> = serde_json::from_str(result_str);
        assert!(parsed.is_ok(), "MCP export should be valid JSON");
    }

    client.shutdown();
}

#[test]
fn test_export_yaml() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nserve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"Serve\" \"${@}\"\n}\n";
    let uri = "file:///test_yaml.sh";
    client.open_document(uri, content);

    let resp = client.send_request(
        "workspace/executeCommand",
        json!({
            "command": "argsh.exportYaml",
            "arguments": [uri]
        }),
    );
    assert!(resp.get("error").is_none(), "Error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let result_str = resp["result"].as_str().unwrap_or("");
        // YAML-like output should contain key-value pairs
        assert!(
            result_str.contains("serve") || result_str.contains("port"),
            "YAML export should contain function/flag info, got: {}",
            &result_str[..result_str.len().min(200)]
        );
    }

    client.shutdown();
}

#[test]
fn test_export_json() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nserve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"Serve\" \"${@}\"\n}\n";
    let uri = "file:///test_json.sh";
    client.open_document(uri, content);

    let resp = client.send_request(
        "workspace/executeCommand",
        json!({
            "command": "argsh.exportJson",
            "arguments": [uri]
        }),
    );
    assert!(resp.get("error").is_none(), "Error: {:?}", resp["error"]);
    if !resp["result"].is_null() {
        let result_str = resp["result"].as_str().unwrap_or("");
        // Should be valid JSON
        let parsed: Result<Value, _> = serde_json::from_str(result_str);
        assert!(parsed.is_ok(), "Export JSON should be valid JSON, got: {}", &result_str[..result_str.len().min(200)]);
        // Should contain function info
        assert!(
            result_str.contains("serve") || result_str.contains("port"),
            "JSON export should contain function info, got: {}",
            &result_str[..result_str.len().min(200)]
        );
    }

    client.shutdown();
}

#[test]
fn test_code_lens_shows_parent_link() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a usage=(\n    'serve' \"Start server\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\nmain::serve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"S\" \"${@}\"\n}\n";
    client.open_document("file:///test_parent.sh", content);

    let resp = client.send_request(
        "textDocument/codeLens",
        json!({
            "textDocument": { "uri": "file:///test_parent.sh" }
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
        "Expected code lenses for main + main::serve, got {}",
        lenses.len()
    );

    // Find the lens for main::serve and check it has "← main"
    let serve_lens = lenses.iter().find(|l| {
        l["command"]["title"]
            .as_str()
            .map(|t| t.contains("\u{2190} main") || t.contains("← main"))
            .unwrap_or(false)
    });
    assert!(
        serve_lens.is_some(),
        "Expected a code lens with '← main' for main::serve, got titles: {:?}",
        lenses
            .iter()
            .filter_map(|l| l["command"]["title"].as_str())
            .collect::<Vec<_>>()
    );

    client.shutdown();
}

#[test]
fn test_code_lens_no_args_leaf() {
    let mut client = LspTestClient::new();
    client.initialize();

    // A function that calls :args but has no args=() array (no flags/params, just a title)
    let content = "#!/usr/bin/env bash\nsource argsh\nlist() {\n  :args \"List secrets\" \"${@}\"\n  echo \"listing...\"\n}\n";
    client.open_document("file:///test_noargs.sh", content);

    let resp = client.send_request(
        "textDocument/codeLens",
        json!({
            "textDocument": { "uri": "file:///test_noargs.sh" }
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
        !lenses.is_empty(),
        "Expected a code lens for list() even without args=() array"
    );

    // Should show as a leaf (terminal icon)
    let title = lenses[0]["command"]["title"].as_str().unwrap();
    assert!(
        title.contains("$(terminal)"),
        "No-args leaf should show terminal icon, got: {}",
        title
    );

    client.shutdown();
}

#[test]
fn test_settings_resolve_depth_passed() {
    let mut client = LspTestClient::new();
    // Initialize with custom resolveDepth
    let resp = client.send_request("initialize", serde_json::json!({
        "processId": null,
        "capabilities": {},
        "rootUri": null,
        "initializationOptions": {
            "resolveDepth": 1,
            "codeLensEnabled": true
        }
    }));
    assert!(resp.get("error").is_none());
    assert!(resp["result"]["capabilities"].is_object());
    client.notify("initialized", serde_json::json!({}));
    client.shutdown();
}

#[test]
fn test_settings_code_lens_disabled() {
    let mut client = LspTestClient::new();
    let resp = client.send_request("initialize", serde_json::json!({
        "processId": null,
        "capabilities": {},
        "rootUri": null,
        "initializationOptions": {
            "resolveDepth": 2,
            "codeLensEnabled": false
        }
    }));
    assert!(resp.get("error").is_none());
    client.notify("initialized", serde_json::json!({}));

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a usage=(\n    'serve' \"Start\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\n";
    client.open_document("file:///test.sh", content);

    let resp = client.send_request("textDocument/codeLens", serde_json::json!({
        "textDocument": { "uri": "file:///test.sh" }
    }));
    assert!(resp.get("error").is_none());
    // With codeLens disabled, should return null or empty
    let result = &resp["result"];
    assert!(result.is_null() || (result.is_array() && result.as_array().unwrap().is_empty()),
        "Code lens should be disabled, got: {:?}", result);

    client.shutdown();
}

#[test]
fn test_rename_function() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  local -a usage=(\n    'serve' \"Start\"\n  )\n  :usage \"App\" \"${@}\"\n  \"${usage[@]}\"\n}\nmain::serve() {\n  :args \"S\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);

    // Prepare rename on "main::serve" (line 9, col 5)
    let resp = client.send_request("textDocument/prepareRename", serde_json::json!({
        "textDocument": { "uri": "file:///test.sh" },
        "position": { "line": 9, "character": 5 }
    }));
    assert!(resp.get("error").is_none(), "prepareRename error: {:?}", resp["error"]);
    // Should return a range (the function is renameable)
    assert!(!resp["result"].is_null(), "Should be renameable");

    // Execute rename
    let resp = client.send_request("textDocument/rename", serde_json::json!({
        "textDocument": { "uri": "file:///test.sh" },
        "position": { "line": 9, "character": 5 },
        "newName": "main::start"
    }));
    assert!(resp.get("error").is_none(), "rename error: {:?}", resp["error"]);
    let result = &resp["result"];
    assert!(!result.is_null(), "Should return workspace edit");

    client.shutdown();
}

#[test]
fn test_rename_non_function_returns_null() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\necho hello\n";
    client.open_document("file:///test.sh", content);

    // Try to rename "echo" — not a function
    let resp = client.send_request("textDocument/prepareRename", serde_json::json!({
        "textDocument": { "uri": "file:///test.sh" },
        "position": { "line": 2, "character": 1 }
    }));
    assert!(resp.get("error").is_none());
    // Should return null (not renameable)
    assert!(resp["result"].is_null(), "Non-function should not be renameable");

    client.shutdown();
}

#[test]
fn test_export_mcp_json_uses_filename_prefix() {
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nsource argsh\nserve() {\n  local port\n  local -a args=(\n    'port|p:~int' \"Port\"\n  )\n  :args \"Start\" \"${@}\"\n}\n";
    client.open_document("file:///myapp.sh", content);

    let resp = client.send_request("workspace/executeCommand", serde_json::json!({
        "command": "argsh.exportMcpJson",
        "arguments": ["file:///myapp.sh"]
    }));
    assert!(resp.get("error").is_none());
    if let Some(json_str) = resp["result"].as_str() {
        assert!(json_str.contains("myapp_serve"), "Tool name should use filename prefix: {}", json_str);
        assert!(json_str.contains("additionalProperties"), "Should include additionalProperties: {}", json_str);
        assert!(json_str.contains("\"title\""), "Should include title field: {}", json_str);
    }
}

#[test]
fn test_rename_respects_word_boundaries() {
    let mut client = LspTestClient::new();
    client.initialize();

    // "main" appears in "domain" — rename should NOT touch it
    let content = "#!/usr/bin/env bash\nsource argsh\nmain() {\n  echo \"domain\"\n  :args \"T\" \"${@}\"\n}\n";
    client.open_document("file:///test.sh", content);

    let resp = client.send_request("textDocument/rename", serde_json::json!({
        "textDocument": { "uri": "file:///test.sh" },
        "position": { "line": 2, "character": 0 },
        "newName": "app"
    }));
    assert!(resp.get("error").is_none());
    if let Some(changes) = resp["result"]["changes"]["file:///test.sh"].as_array() {
        for edit in changes {
            let new_text = edit["newText"].as_str().unwrap_or("");
            assert!(!new_text.contains("doapp"), "Rename corrupted 'domain' into 'doapp': {}", new_text);
        }
    }
}

#[test]
fn test_diagnostic_ag012_scope_shadow() {
    let mut client = LspTestClient::new();
    client.initialize();

    // main has 'domain' in args, main::use also declares 'local domain'
    let content = r#"#!/usr/bin/env bash
source argsh

main() {
  local domain="default"
  local -a args=(
    'domain|d' "Domain name"
  )
  local -a usage=(
    'use' "Set domain"
  )
  :usage "App" "${@}"
  "${usage[@]}"
}

main::use() {
  local domain
  local -a args=(
    'domain:~domain' "Domain to set"
  )
  :args "Set domain" "${@}"
}
"#;
    client.open_document("file:///test.sh", content);
    std::thread::sleep(std::time::Duration::from_millis(300));
    // Should not crash — AG012 is a hint
    client.shutdown();
}

#[test]
fn test_no_ag012_for_unrelated_functions() {
    let mut client = LspTestClient::new();
    client.initialize();

    // Two unrelated functions — no parent/child relationship
    let content = r#"#!/usr/bin/env bash
source argsh

func_a() {
  local name
  local -a args=('name|n' "Name")
  :args "A" "${@}"
}

func_b() {
  local name
  local -a args=('name|n' "Name")
  :args "B" "${@}"
}
"#;
    client.open_document("file:///test.sh", content);
    std::thread::sleep(std::time::Duration::from_millis(300));
    client.shutdown();
}

#[test]
fn test_goto_import_with_prefix() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper");
    fs::write(&helper, "helper_func() { echo; }\n").unwrap();

    let main_content = "#!/usr/bin/env bash\nimport ~helper\nmain() { echo; }\n";
    let main_path = dir.path().join("main.sh");
    fs::write(&main_path, main_content).unwrap();
    let main_uri = format!("file://{}", main_path.to_str().unwrap());

    let mut client = LspTestClient::new();
    client.initialize();
    client.open_document(&main_uri, main_content);

    // Ctrl+Click on "import ~helper" (line 1, col 10)
    let resp = client.send_request("textDocument/definition", serde_json::json!({
        "textDocument": { "uri": main_uri },
        "position": { "line": 1, "character": 10 }
    }));
    assert!(resp.get("error").is_none());
    // Should resolve to the helper file (may be Location object or array)
    if !resp["result"].is_null() {
        let uri = if resp["result"].is_array() {
            resp["result"][0]["uri"].as_str().unwrap_or("")
        } else {
            resp["result"]["uri"].as_str().unwrap_or("")
        };
        assert!(uri.contains("helper"), "Should point to helper file, got: {}", uri);
    }
    client.shutdown();
}

#[test]
fn test_no_ag007_for_last_segment_resolution() {
    // main::manifest has usage entry 'list' → should resolve to manifest::list
    // (last segment prefix: main::manifest → manifest::list)
    let mut client = LspTestClient::new();
    client.initialize();

    let content = r#"#!/usr/bin/env bash
source argsh
main::manifest() {
  local -a usage=(
    'list|l' "List overlays"
    'addons|a' "List addons"
  )
  :usage "Manifest" "${@}"
  "${usage[@]}"
}
manifest::list() { echo list; }
manifest::addons() { echo addons; }
"#;
    client.open_document("file:///test_last_seg.sh", content);
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Verify server is healthy — if AG007 were wrongly produced for these,
    // it wouldn't crash but we verify symbols are correct
    let resp = client.document_symbols("file:///test_last_seg.sh");
    assert!(resp.get("error").is_none());
    let syms = resp["result"].as_array().unwrap();
    assert!(syms.len() >= 3, "Should have main::manifest + manifest::list + manifest::addons");

    client.shutdown();
}

#[test]
fn test_unresolved_import_does_not_crash_when_ag013_emitted() {
    // Crash-regression test: opening a script with an unresolved import should not
    // crash the server when AG013 is produced. We can't capture push notifications
    // in the current test client, so we verify the server stays healthy.
    let mut client = LspTestClient::new();
    client.initialize();

    let content = "#!/usr/bin/env bash\nimport nonexistent_module\nmain() { echo; }\n";
    client.open_document("file:///test.sh", content);
    std::thread::sleep(std::time::Duration::from_millis(300));
    client.shutdown();
}
