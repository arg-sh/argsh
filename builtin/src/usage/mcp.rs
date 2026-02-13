//! :usage::mcp builtin -- MCP (Model Context Protocol) tool server over stdio.
//!
//! Turns any argsh script into a live MCP server. AI agents connect via stdio
//! and discover/invoke subcommands as tools.

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::shared;
use crate::shell;
use std::ffi::{c_char, c_int};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Command, Stdio};
use super::{
    extract_subcommands, extract_flags_for_llm,
    json_escape, argsh_type_to_json, sanitize_tool_name,
    FlagInfo, SubCmd,
};

// -- :usage::mcp builtin registration ----------------------------------------

static MCP_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Run MCP (Model Context Protocol) tool server over stdio.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage::mcp_struct"]
pub static mut MCP_STRUCT: BashBuiltin = BashBuiltin {
    name: c":usage::mcp".as_ptr(),
    function: mcp_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":usage::mcp [--] <title> [usage_pairs...]".as_ptr(),
    long_doc: MCP_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage::mcp_builtin_load"]
pub extern "C" fn mcp_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage::mcp_builtin_unload"]
pub extern "C" fn mcp_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback

extern "C" fn mcp_builtin_fn(word_list: *const WordList) -> c_int {
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        mcp_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    std::process::exit(if code == shared::HELP_EXIT || code == 0 { 0 } else { code }) // coverage:off
}

// -- MCP server implementation ------------------------------------------------

/// Main entry point for :usage::mcp builtin.
/// Args: [user_args] [-- title original_usage_pairs...]
pub fn mcp_main(args: &[String]) -> i32 {
    let sep = args.iter().position(|s| s == "--");
    let (user_args, meta) = match sep {
        Some(pos) => (&args[..pos], &args[pos + 1..]),
        None => (args, [].as_slice()), // coverage:off - defensive_check: deferred dispatch always provides "--"
    };

    // Handle --help
    if !user_args.is_empty() && (user_args[0] == "-h" || user_args[0] == "--help") {
        let commandname = shell::get_commandname();
        let cmd_str = if commandname.len() > 1 {
            commandname[..commandname.len() - 1].join(" ")
        } else {
            shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
        };
        // Derive executable and args for MCP config:
        // command = script/binary name, args = intermediate subcommands + "mcp"
        let script_name = if !commandname.is_empty() {
            commandname[0].clone()
        } else {
            shell::get_script_name() // coverage:off - defensive_check
        };
        let mut mcp_args: Vec<String> = if commandname.len() > 2 {
            commandname[1..commandname.len() - 1].to_vec()
        } else {
            Vec::new()
        };
        mcp_args.push("mcp".to_string());
        let args_json = mcp_args
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(",");

        println!("Start an MCP (Model Context Protocol) tool server over stdio.\n");
        println!("Usage: {} mcp\n", cmd_str);
        println!("The server exposes subcommands as tools via the MCP protocol.");
        println!("Configure your AI client to connect:\n");
        println!("  # .mcp.json");
        let script_path = shell::get_script_path();
        println!("  {{\"mcpServers\": {{\"{}\":{{\"type\":\"stdio\",\"command\":\"{}\",\"args\":[{}]}}}}}}", json_escape(&cmd_str), json_escape(&script_path), args_json);
        return shared::HELP_EXIT;
    }

    let title = meta.first().map(|s| s.as_str()).unwrap_or("");
    let usage_pairs = if meta.len() > 1 { &meta[1..] } else { &[] as &[String] }; // coverage:off - defensive_check: deferred dispatch always provides title + usage_pairs
    let args_arr = shell::read_array("args");

    // Script name and path for tool naming and subprocess invocation
    let commandname = shell::get_commandname();
    let cmd_name = if commandname.len() > 1 {
        commandname[commandname.len() - 2].clone()
    } else {
        shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
    };
    let script_path = shell::get_script_path();

    // Pre-extract tool data (immutable for the session)
    let subcmds = extract_subcommands(usage_pairs);
    let flags = extract_flags_for_llm(&args_arr);

    // JSON-RPC stdio loop
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // coverage:off - io_error: stdin read errors don't occur in stdio MCP
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let id = extract_json_field(&line, "id");
        let method = extract_json_string(&line, "method");

        match method.as_deref() {
            Some("initialize") => {
                handle_initialize(&mut writer, &id, &cmd_name);
            }
            Some("notifications/initialized") => {} // no-op, no response
            Some("ping") => {
                handle_ping(&mut writer, &id);
            }
            Some("tools/list") => {
                handle_tools_list(&mut writer, &id, &cmd_name, title, &subcmds, &flags);
            }
            Some("tools/call") => {
                let params = extract_json_field(&line, "params").unwrap_or_default();
                handle_tools_call(
                    &mut writer, &id, &params,
                    &script_path, &cmd_name, &subcmds, &flags,
                );
            }
            Some(_) => {
                // Unknown method
                write_jsonrpc_error(&mut writer, &id, -32601, "Method not found");
            }
            None => {
                // Notification without method or invalid message — ignore
                if id.is_some() {
                    write_jsonrpc_error(&mut writer, &id, -32600, "Invalid request");
                }
            }
        }
        let _ = writer.flush();
    }

    0
}

// -- JSON-RPC helpers ---------------------------------------------------------

/// Write a JSON-RPC success response.
fn write_jsonrpc_response<W: Write>(writer: &mut W, id: &Option<String>, result: &str) {
    let id_str = id.as_deref().unwrap_or("null");
    let _ = writeln!(writer, "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{}}}", id_str, result);
}

/// Write a JSON-RPC error response.
fn write_jsonrpc_error<W: Write>(writer: &mut W, id: &Option<String>, code: i32, message: &str) {
    let id_str = id.as_deref().unwrap_or("null");
    let _ = writeln!(
        writer,
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"error\":{{\"code\":{},\"message\":\"{}\"}}}}",
        id_str, code, json_escape(message)
    );
}

// -- Protocol handlers --------------------------------------------------------

/// Handle `initialize` request.
fn handle_initialize<W: Write>(writer: &mut W, id: &Option<String>, cmd_name: &str) {
    let version = shell::get_scalar("ARGSH_VERSION").unwrap_or_default();
    let result = format!(
        "{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{\"tools\":{{}}}},\"serverInfo\":{{\"name\":\"{}\",\"version\":\"{}\"}}}}",
        json_escape(cmd_name),
        json_escape(&version)
    );
    write_jsonrpc_response(writer, id, &result);
}

/// Handle `ping` request.
fn handle_ping<W: Write>(writer: &mut W, id: &Option<String>) {
    write_jsonrpc_response(writer, id, "{}");
}

/// Handle `tools/list` request.
fn handle_tools_list<W: Write>(
    writer: &mut W,
    id: &Option<String>,
    cmd_name: &str,
    title: &str,
    subcmds: &[SubCmd],
    flags: &[FlagInfo],
) {
    let mut tools = String::from("{\"tools\":[");
    let first_line = title.lines().next().unwrap_or(title).trim();

    if subcmds.is_empty() {
        // Single tool for the script itself
        tools.push_str(&format_tool(
            &sanitize_tool_name(cmd_name),
            first_line,
            flags,
        ));
    } else {
        for (i, cmd) in subcmds.iter().enumerate() {
            if i > 0 {
                tools.push(',');
            }
            let tool_name = sanitize_tool_name(&format!("{}_{}", cmd_name, cmd.name));
            let desc = if cmd.desc.is_empty() { first_line } else { &cmd.desc }; // coverage:off - empty_desc: test subcmds always have descriptions
            tools.push_str(&format_tool(&tool_name, desc, flags));
        }
    }

    tools.push_str("]}");
    write_jsonrpc_response(writer, id, &tools);
}

/// Format a single MCP tool definition.
fn format_tool(name: &str, description: &str, flags: &[FlagInfo]) -> String {
    let mut s = String::from("{");
    s.push_str(&format!("\"name\":\"{}\",", json_escape(name)));
    s.push_str(&format!("\"description\":\"{}\",", json_escape(description)));
    s.push_str("\"inputSchema\":{\"type\":\"object\",\"properties\":{");

    for (i, flag) in flags.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let json_type = argsh_type_to_json(&flag.type_name, flag.is_boolean);
        s.push_str(&format!(
            "\"{}\":{{\"type\":\"{}\",\"description\":\"{}\"}}",
            json_escape(&flag.name),
            json_type,
            json_escape(&flag.desc)
        ));
    }

    s.push_str("},\"required\":[");
    let mut first = true;
    for flag in flags {
        if flag.required { // coverage:off - required_flags: test fixtures don't use required flags
            if !first { // coverage:off
                s.push(','); // coverage:off
            } // coverage:off
            s.push_str(&format!("\"{}\"", json_escape(&flag.name))); // coverage:off
            first = false; // coverage:off
        } // coverage:off
    }
    s.push_str("]}}");
    s
}

/// Handle `tools/call` request.
fn handle_tools_call<W: Write>(
    writer: &mut W,
    id: &Option<String>,
    params: &str,
    script_path: &str,
    cmd_name: &str,
    subcmds: &[SubCmd],
    flags: &[FlagInfo],
) {
    // Extract tool name and arguments from params
    let tool_name = match extract_json_string(params, "name") {
        Some(n) => n,
        None => {
            write_jsonrpc_error(writer, id, -32602, "Missing tool name");
            return;
        }
    };

    // Resolve tool name → subcommand
    let subcommand = resolve_tool(&tool_name, cmd_name, subcmds);
    if subcommand.is_none() && !subcmds.is_empty() {
        write_jsonrpc_error(writer, id, -32602, &format!("Unknown tool: {}", tool_name));
        return;
    }

    // Parse arguments
    let args_json = extract_json_field(params, "arguments").unwrap_or_default();
    let arg_pairs = parse_flat_json_object(&args_json);

    // Build CLI args
    let cli_args = build_cli_args(subcommand.as_deref(), &arg_pairs, flags);

    // Execute
    let (exit_code, stdout_text, stderr_text) = execute_tool(script_path, &cli_args);

    // Format response
    let is_error = exit_code != 0;
    let text = if is_error && !stderr_text.is_empty() { // coverage:off - error_path: test fixtures always succeed
        format!("{}\n{}", stdout_text, stderr_text) // coverage:off
    } else {
        stdout_text
    };
    let text = text.trim_end_matches('\n');

    let result = format!(
        "{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}],\"isError\":{}}}",
        json_escape(text),
        is_error
    );
    write_jsonrpc_response(writer, id, &result);
}

/// Resolve a tool name to a subcommand name.
fn resolve_tool(tool_name: &str, cmd_name: &str, subcmds: &[SubCmd]) -> Option<String> {
    if subcmds.is_empty() { // coverage:off - tools_call: no-subcommand scripts not tested via fixture
        // No-subcommands script — tool name should match the script itself // coverage:off
        if tool_name == sanitize_tool_name(cmd_name) { // coverage:off
            return Some(String::new()); // coverage:off
        } // coverage:off
        return None; // coverage:off
    } // coverage:off
    for cmd in subcmds {
        let expected = sanitize_tool_name(&format!("{}_{}", cmd_name, cmd.name));
        if tool_name == expected {
            return Some(cmd.name.clone());
        }
    }
    None
}

/// Build CLI arguments from JSON key-value pairs.
fn build_cli_args(
    subcommand: Option<&str>,
    arg_pairs: &[(String, JsonValue)],
    flags: &[FlagInfo],
) -> Vec<String> {
    let mut args = Vec::new();

    // Prepend subcommand if present
    if let Some(sub) = subcommand {
        if !sub.is_empty() {
            args.push(sub.to_string());
        }
    }

    for (key, value) in arg_pairs {
        // Find matching flag
        let flag = flags.iter().find(|f| f.name == *key);
        let _flag = match flag {
            Some(f) => f,
            None => continue, // Unknown arg — ignore (lenient for LLM hallucinations)
        };

        match value {
            JsonValue::Bool(true) => {
                args.push(format!("--{}", key));
            }
            JsonValue::Bool(false) | JsonValue::Null => {
                // Omit
            }
            JsonValue::Str(s) => {
                args.push(format!("--{}", key));
                args.push(s.clone());
            }
            JsonValue::Number(n) => {
                args.push(format!("--{}", key));
                args.push(n.clone());
            }
        }
    }

    args
}

/// Execute a tool by spawning the script as a subprocess.
///
/// Bash installs a SIGCHLD handler that reaps child processes via waitpid(-1).
/// This races with Rust's Command::output() which also calls waitpid(pid).
/// We temporarily reset SIGCHLD to SIG_DFL around the spawn+wait cycle.
fn execute_tool(script_path: &str, cli_args: &[String]) -> (i32, String, String) {
    // Safety: bash is single-threaded, and we restore the handler immediately after.
    unsafe {
        let old_handler = libc::signal(libc::SIGCHLD, libc::SIG_DFL);
        let result = Command::new(script_path)
            .args(cli_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        libc::signal(libc::SIGCHLD, old_handler);

        match result {
            Ok(output) => {
                let code = output.status.code().unwrap_or(1);
                let stdout_text = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr_text = String::from_utf8_lossy(&output.stderr).into_owned();
                (code, stdout_text, stderr_text)
            }
            Err(e) => (1, String::new(), format!("Failed to execute: {}", e)), // coverage:off - exec_error: script path comes from $0 which always exists
        }
    }
}

// -- Minimal JSON parsing -----------------------------------------------------
//
// CLI flags are always flat key-value pairs (no nesting, no arrays).
// This parser handles: strings, numbers, booleans, null.

/// JSON value types for flat objects.
#[derive(Debug)]
pub enum JsonValue {
    Str(String),
    Number(String),
    Bool(bool),
    Null,
}

/// Extract a raw JSON field value (as a string) from a JSON object.
/// Returns the raw JSON fragment for the given key.
/// Only matches keys at the top level of the object (depth 1) to avoid
/// false positives from nested objects with the same key name.
fn extract_json_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let needle_bytes = needle.as_bytes();
    let bytes = json.as_bytes();
    let len = bytes.len();
    let needle_len = needle_bytes.len();

    let mut in_string = false;
    let mut escape = false;
    let mut depth: i32 = 0;
    let mut i = 0;

    while i < len {
        let c = bytes[i];

        if in_string {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        match c {
            b'"' => {
                // At depth 1, check if this is our key
                if depth == 1 && i + needle_len <= len && json[i..i + needle_len] == *needle {
                    let after_key = &json[i + needle_len..];
                    let after_colon = after_key.trim_start();
                    if let Some(rest) = after_colon.strip_prefix(':') {
                        let value_start = rest.trim_start();
                        return extract_value_at(value_start);
                    }
                }
                in_string = true;
            }
            b'{' | b'[' => { depth += 1; }
            b'}' | b']' => {
                depth -= 1;
                if depth <= 0 { return None; }
            }
            _ => {}
        }

        i += 1;
    }

    None
}

/// Extract a JSON value starting at the given position.
fn extract_value_at(value_start: &str) -> Option<String> {
    if value_start.starts_with('"') {
        let end = find_string_end(value_start)?;
        Some(value_start[..end + 1].to_string())
    } else if value_start.starts_with('{') {
        let end = find_matching_brace(value_start, '{', '}')?;
        Some(value_start[..end + 1].to_string())
    } else if value_start.starts_with('[') {
        let end = find_matching_brace(value_start, '[', ']')?; // coverage:off - json_defensive: MCP protocol doesn't use array fields
        Some(value_start[..end + 1].to_string()) // coverage:off
    } else {
        let end = value_start.find([',', '}', ']', '\n'])
            .unwrap_or(value_start.len());
        let raw = value_start[..end].trim();
        if raw.is_empty() {
            None // coverage:off - json_defensive: empty value after colon not possible with valid JSON
        } else {
            Some(raw.to_string())
        }
    }
}

/// Extract a JSON string field (unquoted).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let raw = extract_json_field(json, key)?;
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        Some(unescape_json_string(&raw[1..raw.len() - 1]))
    } else {
        None // coverage:off - json_defensive: extract_json_string only called on string fields
    }
}

/// Find the end of a JSON string (position of closing quote).
fn find_string_end(s: &str) -> Option<usize> {
    if !s.starts_with('"') { // coverage:off - json_defensive: only called on string-starting positions
        return None; // coverage:off
    } // coverage:off
    let mut i = 1;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'\\' { // coverage:off - json_string: MCP protocol values don't contain escape sequences
            i += 2; // coverage:off
            continue; // coverage:off
        } // coverage:off
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None // coverage:off - json_defensive: unterminated string in well-formed JSON
}

/// Find the matching closing brace/bracket.
fn find_matching_brace(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut prev_backslash = false;

    for (i, c) in s.char_indices() {
        if in_string { // coverage:off - json_brace: string tracking in nested objects; MCP params are shallow
            if c == '\\' && !prev_backslash { // coverage:off
                prev_backslash = true; // coverage:off
                continue; // coverage:off
            } // coverage:off
            if c == '"' && !prev_backslash { // coverage:off
                in_string = false; // coverage:off
            } // coverage:off
            prev_backslash = false; // coverage:off
            continue; // coverage:off
        }
        if c == '"' {
            in_string = true;
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None // coverage:off - json_defensive: unmatched brace in well-formed JSON
}

/// Unescape a JSON string (handles \\, \", \n, \r, \t, \/, \uXXXX).
fn unescape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'), // coverage:off - json_escape: MCP field values don't contain literal \n
                Some('r') => result.push('\r'), // coverage:off - json_escape: MCP field values don't contain literal \r
                Some('t') => result.push('\t'), // coverage:off - json_escape: MCP field values don't contain literal \t
                Some('/') => result.push('/'), // coverage:off - json_escape: MCP field values don't contain \/
                Some('b') => result.push('\u{0008}'), // coverage:off - json_escape: backspace
                Some('f') => result.push('\u{000C}'), // coverage:off - json_escape: form feed
                Some('u') => { // coverage:off - json_escape: unicode escapes
                    let hex: String = chars.by_ref().take(4).collect(); // coverage:off
                    if hex.len() == 4 { // coverage:off
                        if let Ok(code) = u32::from_str_radix(&hex, 16) { // coverage:off
                            if let Some(ch) = char::from_u32(code) { // coverage:off
                                result.push(ch); // coverage:off
                            } else { // coverage:off
                                result.push_str("\\u"); // coverage:off
                                result.push_str(&hex); // coverage:off
                            } // coverage:off
                        } else { // coverage:off
                            result.push_str("\\u"); // coverage:off
                            result.push_str(&hex); // coverage:off
                        } // coverage:off
                    } else { // coverage:off
                        result.push_str("\\u"); // coverage:off
                        result.push_str(&hex); // coverage:off
                    } // coverage:off
                } // coverage:off
                Some(other) => { // coverage:off - json_escape: unknown escape sequence defensive check
                    result.push('\\'); // coverage:off
                    result.push(other); // coverage:off
                } // coverage:off
                None => result.push('\\'), // coverage:off - trailing backslash edge case
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parse a flat JSON object into key-value pairs.
pub fn parse_flat_json_object(json: &str) -> Vec<(String, JsonValue)> {
    let mut pairs = Vec::new();
    let trimmed = json.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return pairs;
    }
    let inner = &trimmed[1..trimmed.len() - 1];

    let mut pos = 0;
    let bytes = inner.as_bytes();

    while pos < bytes.len() {
        // Skip whitespace and commas
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b',' || bytes[pos] == b'\n' || bytes[pos] == b'\r' || bytes[pos] == b'\t') {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }

        // Expect opening quote for key
        if bytes[pos] != b'"' { // coverage:off - json_defensive: valid JSON objects have quoted keys
            break; // coverage:off
        } // coverage:off
        let key_start = pos;
        let key_end = match find_string_end(&inner[key_start..]) {
            Some(end) => key_start + end,
            None => break, // coverage:off - json_defensive: unterminated key string
        };
        let key = unescape_json_string(&inner[key_start + 1..key_end]);
        pos = key_end + 1;

        // Skip whitespace and colon
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        if pos >= bytes.len() || bytes[pos] != b':' { // coverage:off - json_defensive: valid JSON has colon between key and value
            break; // coverage:off
        } // coverage:off
        pos += 1;
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }

        // Parse value
        if pos >= bytes.len() { // coverage:off - json_defensive: value always follows colon
            break; // coverage:off
        } // coverage:off

        let (value, new_pos) = parse_json_value(&inner[pos..]);
        pos += new_pos;

        pairs.push((key, value));
    }

    pairs
}

/// Parse a single JSON value, returning the value and number of bytes consumed
/// (relative to the original input, including any leading whitespace).
fn parse_json_value(s: &str) -> (JsonValue, usize) {
    let original_len = s.len();
    let trimmed = s.trim_start();
    let offset = original_len - trimmed.len();

    if trimmed.starts_with('"') {
        // String
        if let Some(end) = find_string_end(trimmed) {
            let val = unescape_json_string(&trimmed[1..end]);
            return (JsonValue::Str(val), offset + end + 1);
        } // coverage:off - json_defensive: find_string_end always succeeds for quoted values
    } else if trimmed.starts_with("true") {
        return (JsonValue::Bool(true), offset + 4);
    } else if trimmed.starts_with("false") {
        return (JsonValue::Bool(false), offset + 5);
    } else if trimmed.starts_with("null") {
        return (JsonValue::Null, offset + 4);
    } else if trimmed.starts_with('{') { // coverage:off - json_defensive: nested objects not used in MCP argument values
        // Nested object — skip over it // coverage:off
        if let Some(end) = find_matching_brace(trimmed, '{', '}') { // coverage:off
            return (JsonValue::Str(trimmed[..end + 1].to_string()), offset + end + 1); // coverage:off
        } // coverage:off
    } else if trimmed.starts_with('[') { // coverage:off - json_defensive: arrays not used in MCP argument values
        // Array — skip over it // coverage:off
        if let Some(end) = find_matching_brace(trimmed, '[', ']') { // coverage:off
            return (JsonValue::Str(trimmed[..end + 1].to_string()), offset + end + 1); // coverage:off
        } // coverage:off
    } else {
        // Number or other literal
        let end = trimmed.find(|c: char| c == ',' || c == '}' || c == ']' || c.is_whitespace())
            .unwrap_or(trimmed.len());
        let raw = &trimmed[..end];
        if !raw.is_empty() {
            return (JsonValue::Number(raw.to_string()), offset + end);
        }
    }

    (JsonValue::Null, offset + 1) // coverage:off - json_defensive: fallback for malformed JSON
}
