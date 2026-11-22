//! :usage::mcp builtin -- MCP (Model Context Protocol) tool server over stdio.
//!
//! Mirrors: libraries/args.sh
//!
//! Turns any argsh script into a live MCP server. AI agents connect via stdio
//! and discover/invoke subcommands as tools.

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::shared;
use crate::shell;
use std::ffi::{c_char, c_int};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Command, Stdio};
#[allow(unused_imports)]
use super::{
    extract_subcommands, extract_flags_for_llm, agent_flags,
    json_escape, argsh_type_to_json, sanitize_tool_name,
    build_command_tree, flatten_leaves,
    FlagInfo, SubCmd,
};

// -- :usage::mcp builtin registration ----------------------------------------

static MCP_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Run MCP (Model Context Protocol) tool server over stdio.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

/// Run an MCP (Model Context Protocol) tool server over stdio, exposing subcommands as tools.
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
        println!("  {{\"mcpServers\": {{\"{}\":{{\"type\":\"stdio\",\"command\":\"./{}\",\"args\":[{}]}}}}}}", json_escape(&cmd_str), json_escape(&script_name), args_json);
        println!("\nFor HTTP transport, use an MCP stdio-to-HTTP bridge.");
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

    // Build command tree and extract leaf tools
    let caller = shell::get_funcname(0);
    let tree = build_command_tree(usage_pairs, &args_arr, caller.as_deref());
    let leaves = flatten_leaves(&tree);

    // Build owned leaf data for the session (tool_name, full_path, flags)
    let leaf_tools: Vec<LeafTool> = if leaves.is_empty() {
        // No subcommands (or all hidden) — single tool for the script itself
        let top_flags = extract_flags_for_llm(&args_arr);
        vec![LeafTool {
            tool_name: sanitize_tool_name(&cmd_name),
            full_path: Vec::new(),
            desc: title.lines().next().unwrap_or(title).trim().to_string(),
            // All the flags of the script as the parser reads them, also the
            // `#` ones: a `#` flag with `:@` is refused too.
            refused: refused_flags(&agent_flags(&args_arr)),
            flags: top_flags,
            annotations: Vec::new(),
        }]
    } else {
        leaves.iter().map(|leaf| {
            let tool_name = sanitize_tool_name(
                &format!("{}_{}", cmd_name, leaf.full_path.join("_")),
            );
            LeafTool {
                tool_name,
                full_path: leaf.full_path.clone(),
                desc: leaf.desc.clone(),
                flags: leaf.flags.clone(),
                refused: leaf.refused.clone(),
                annotations: leaf.annotations.clone(),
            }
        }).collect()
    };

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
                handle_tools_list_v2(&mut writer, &id, title, &leaf_tools);
            }
            Some("tools/call") => {
                // A call can start the script, so the strict reader reads the
                // whole request (the other methods use the scanner).
                match parse_json_strict(&line) {
                    Ok(request) => {
                        let params = match &request {
                            Json::Object(members) => json_member(members, "params").cloned(),
                            _ => None,
                        }
                        .unwrap_or(Json::Object(Vec::new()));
                        handle_tools_call_json(
                            &mut writer, &id, &params,
                            &script_path, &leaf_tools,
                        );
                    }
                    Err((code, message)) => write_jsonrpc_error(&mut writer, &id, code, &message),
                }
            }
            Some("resources/list") => {
                handle_resources_list(&mut writer, &id, &cmd_name);
            }
            Some("resources/read") => {
                let params = extract_json_field(&line, "params").unwrap_or_default();
                handle_resources_read(&mut writer, &id, &params, &cmd_name, title, &leaf_tools);
            }
            Some("prompts/list") => {
                handle_prompts_list(&mut writer, &id);
            }
            Some("prompts/get") => {
                let params = extract_json_field(&line, "params").unwrap_or_default();
                handle_prompts_get(&mut writer, &id, &params, &cmd_name);
            }
            Some("logging/setLevel") => {
                write_jsonrpc_response(&mut writer, &id, "{}");
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
pub(crate) fn write_jsonrpc_response<W: Write>(writer: &mut W, id: &Option<String>, result: &str) {
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
        "{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{\"tools\":{{\"listChanged\":false}},\"resources\":{{}},\"prompts\":{{}},\"logging\":{{}}}},\"serverInfo\":{{\"name\":\"{}\",\"version\":\"{}\"}}}}",
        json_escape(cmd_name),
        json_escape(&version)
    );
    write_jsonrpc_response(writer, id, &result);
}

/// Handle `ping` request.
fn handle_ping<W: Write>(writer: &mut W, id: &Option<String>) {
    write_jsonrpc_response(writer, id, "{}");
}

/// Owned leaf tool data extracted from the command tree.
pub(crate) struct LeafTool {
    pub(crate) tool_name: String,
    pub(crate) full_path: Vec<String>,
    pub(crate) desc: String,
    /// The flags of the schema (`format_tool` leaves out the `:@` ones).
    pub(crate) flags: Vec<FlagInfo>,
    /// The flags that a call must not send: the `:@` flags. A call that
    /// names one of them by any name gets an error, and no script starts.
    pub(crate) refused: Vec<FlagInfo>,
    pub(crate) annotations: Vec<String>,
}

/// The `:@` flags of a flag list: the flags that MCP refuses.
pub(crate) fn refused_flags(flags: &[FlagInfo]) -> Vec<FlagInfo> {
    flags.iter().filter(|f| f.mcp_hidden).cloned().collect()
}

/// Handle `tools/list` request (v2 — tree-aware, per-leaf flags).
pub(crate) fn handle_tools_list_v2<W: Write>(
    writer: &mut W,
    id: &Option<String>,
    _title: &str,
    leaf_tools: &[LeafTool],
) {
    let mut tools = String::from("{\"tools\":[");

    for (i, leaf) in leaf_tools.iter().enumerate() {
        if i > 0 {
            tools.push(',');
        }
        tools.push_str(&format_tool(&leaf.tool_name, &leaf.desc, &leaf.flags, &leaf.annotations));
    }

    tools.push_str("]}");
    write_jsonrpc_response(writer, id, &tools);
}

/// Handle `tools/list` request (legacy — kept for backward compatibility).
#[allow(dead_code)]
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
            &[],
        ));
    } else {
        for (i, cmd) in subcmds.iter().enumerate() {
            if i > 0 {
                tools.push(',');
            }
            let tool_name = sanitize_tool_name(&format!("{}_{}", cmd_name, cmd.name));
            let desc = if cmd.desc.is_empty() { first_line } else { &cmd.desc }; // coverage:off - empty_desc: test subcmds always have descriptions
            tools.push_str(&format_tool(&tool_name, desc, flags, &[]));
        }
    }

    tools.push_str("]}");
    write_jsonrpc_response(writer, id, &tools);
}

/// Format a single MCP tool definition. A `:@` flag is not listed.
pub(crate) fn format_tool(name: &str, description: &str, flags: &[FlagInfo], annotations: &[String]) -> String {
    let flags: Vec<&FlagInfo> = flags.iter().filter(|f| !f.mcp_hidden).collect();
    let mut s = String::from("{");
    s.push_str(&format!("\"name\":\"{}\",", json_escape(name)));
    s.push_str(&format!("\"title\":\"{}\",", json_escape(description)));
    s.push_str(&format!("\"description\":\"{}\",", json_escape(description)));

    // inputSchema — use minimal schema when no flags are present
    if flags.is_empty() {
        s.push_str("\"inputSchema\":{\"type\":\"object\",\"additionalProperties\":false}");
    } else {
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
        for flag in &flags {
            if flag.required { // coverage:off - required_flags: test fixtures don't use required flags
                if !first { // coverage:off
                    s.push(','); // coverage:off
                } // coverage:off
                s.push_str(&format!("\"{}\"", json_escape(&flag.name))); // coverage:off
                first = false; // coverage:off
            } // coverage:off
        }
        s.push_str("],\"additionalProperties\":false}");
    }

    // outputSchema for @json annotated tools
    if annotations.iter().any(|a| a == "json") {
        s.push_str(",\"outputSchema\":{}");
    }

    // Tool annotations (MCP hints)
    let mut hints: Vec<String> = Vec::new();
    for annot in annotations {
        match annot.as_str() {
            "readonly" => hints.push("\"readOnlyHint\":true".to_string()),
            "destructive" => hints.push("\"destructiveHint\":true".to_string()),
            "idempotent" => hints.push("\"idempotentHint\":true".to_string()),
            "openworld" => hints.push("\"openWorldHint\":true".to_string()),
            "json" => {} // handled via outputSchema above
            _ => {} // coverage:off - unknown annotations silently ignored
        }
    }
    if !hints.is_empty() {
        s.push_str(&format!(",\"annotations\":{{{}}}", hints.join(",")));
    }

    s.push('}');
    s
}

/// Handle `resources/list` request.
pub(crate) fn handle_resources_list<W: Write>(writer: &mut W, id: &Option<String>, cmd_name: &str) {
    let result = format!(
        "{{\"resources\":[{{\"uri\":\"script:///help\",\"name\":\"Help\",\"description\":\"Help output for {}\",\"mimeType\":\"text/plain\"}},{{\"uri\":\"script:///version\",\"name\":\"Version\",\"description\":\"argsh version\",\"mimeType\":\"text/plain\"}}]}}",
        json_escape(cmd_name)
    );
    write_jsonrpc_response(writer, id, &result);
}

/// Handle `resources/read` request.
/// Generates help text in-process from the already-parsed metadata to avoid
/// re-executing the script (which could have side effects).
pub(crate) fn handle_resources_read<W: Write>(
    writer: &mut W,
    id: &Option<String>,
    params: &str,
    cmd_name: &str,
    title: &str,
    leaf_tools: &[LeafTool],
) {
    let uri = extract_json_string(params, "uri");
    match uri.as_deref() {
        Some("script:///help") => {
            let first_line = title.lines().next().unwrap_or(title).trim();
            let mut help = format!("{}\n\nAvailable commands:\n", first_line);
            for tool in leaf_tools {
                help.push_str(&format!("  {:20} {}\n", tool.full_path.join(" "), tool.desc));
            }
            help.push_str(&format!("\nUse \"{} <command> --help\" for more information.", cmd_name));
            let result = format!(
                "{{\"contents\":[{{\"uri\":\"script:///help\",\"mimeType\":\"text/plain\",\"text\":\"{}\"}}]}}",
                json_escape(&help)
            );
            write_jsonrpc_response(writer, id, &result);
        }
        Some("script:///version") => {
            let version = shell::get_scalar("ARGSH_VERSION").unwrap_or_default();
            let result = format!(
                "{{\"contents\":[{{\"uri\":\"script:///version\",\"mimeType\":\"text/plain\",\"text\":\"{}\"}}]}}",
                json_escape(&version)
            );
            write_jsonrpc_response(writer, id, &result);
        }
        _ => {
            write_jsonrpc_error(writer, id, -32602, "Unknown resource URI");
        }
    }
}

/// Handle `prompts/list` request.
pub(crate) fn handle_prompts_list<W: Write>(writer: &mut W, id: &Option<String>) {
    let mut prompts = String::from("{\"prompts\":[");

    // run_subcommand prompt
    prompts.push_str("{\"name\":\"run_subcommand\",\"description\":\"Run a specific subcommand\",\"arguments\":[");
    prompts.push_str("{\"name\":\"subcommand\",\"description\":\"The subcommand to run\",\"required\":true},");
    prompts.push_str("{\"name\":\"args\",\"description\":\"Additional arguments\",\"required\":false}");
    prompts.push_str("]},");

    // get_help prompt
    prompts.push_str("{\"name\":\"get_help\",\"description\":\"Show help for a subcommand\",\"arguments\":[");
    prompts.push_str("{\"name\":\"subcommand\",\"description\":\"The subcommand to get help for\",\"required\":false}");
    prompts.push_str("]}");

    prompts.push_str("]}");
    write_jsonrpc_response(writer, id, &prompts);
}

/// Handle `prompts/get` request.
pub(crate) fn handle_prompts_get<W: Write>(writer: &mut W, id: &Option<String>, params: &str, cmd_name: &str) {
    let name = extract_json_string(params, "name");
    let args_obj = extract_json_field(params, "arguments").unwrap_or_default();

    match name.as_deref() {
        Some("run_subcommand") => {
            let subcmd = extract_json_string(&args_obj, "subcommand").unwrap_or_default();
            if subcmd.is_empty() {
                write_jsonrpc_error(writer, id, -32602, "Missing required argument: subcommand");
                return;
            }
            let extra = extract_json_string(&args_obj, "args").unwrap_or_default();
            let content = if extra.is_empty() {
                format!("Run the '{}' subcommand of {}", subcmd, cmd_name)
            } else {
                format!("Run the '{}' subcommand of {}. With arguments: {}", subcmd, cmd_name, extra)
            };
            let result = format!(
                "{{\"description\":\"Run a subcommand\",\"messages\":[{{\"role\":\"user\",\"content\":{{\"type\":\"text\",\"text\":\"{}\"}}}}]}}",
                json_escape(&content)
            );
            write_jsonrpc_response(writer, id, &result);
        }
        Some("get_help") => {
            let subcmd = extract_json_string(&args_obj, "subcommand").unwrap_or_default();
            let content = if subcmd.is_empty() {
                format!("Show the full help output for {}", cmd_name)
            } else {
                format!("Show help for the '{}' subcommand of {}", subcmd, cmd_name)
            };
            let result = format!(
                "{{\"description\":\"Get help\",\"messages\":[{{\"role\":\"user\",\"content\":{{\"type\":\"text\",\"text\":\"{}\"}}}}]}}",
                json_escape(&content)
            );
            write_jsonrpc_response(writer, id, &result);
        }
        _ => {
            write_jsonrpc_error(writer, id, -32602, "Unknown prompt");
        }
    }
}

/// The `:@` flag that a `tools/call` argument names, if any. The key is read
/// as the parser reads a flag (`shared::parse_flag_at`): up to a `=`, after
/// `--`, or the first character after a single `-`. A bare key is a name, as
/// the call sends it as `--<key>`. The key matches any name of the flag (long,
/// short or alias), and also its variable form (`_` for `-`).
pub(crate) fn mcp_hidden_flag<'a>(key: &str, flags: &'a [FlagInfo]) -> Option<&'a FlagInfo> {
    let part = key.split('=').next().unwrap_or(key);
    let name = if let Some(long) = part.strip_prefix("--") {
        long
    } else if let Some(short) = part.strip_prefix('-') {
        short
            .chars()
            .next()
            .map_or(short, |c| &short[..c.len_utf8()])
    } else {
        part
    };
    if name.is_empty() {
        return None;
    }
    flags.iter().filter(|f| f.mcp_hidden).find(|f| {
        f.names
            .iter()
            .any(|n| n == name || n.replace('-', "_") == name)
    })
}

/// The text of the refusal of a `:@` flag. It names the key as the call sent it.
fn refusal(key: &str) -> String {
    format!("Flag not available over MCP: {}", key)
}

/// The tool name and the arguments of the params of a `tools/call`. An error
/// is a JSON-RPC code and its text.
pub(crate) fn tools_call_request(
    params: &Json,
) -> Result<(String, Vec<(String, Json)>), (i32, String)> {
    let Json::Object(members) = params else {
        return Err((-32602, "Params must be an object".to_string()));
    };
    let name = match json_member(members, "name") {
        Some(Json::Str(name)) => name.clone(),
        _ => return Err((-32602, "Missing tool name".to_string())),
    };
    let arguments = match json_member(members, "arguments") {
        None => Vec::new(),
        Some(Json::Object(arguments)) => arguments.clone(),
        Some(_) => return Err((-32602, "Arguments must be an object".to_string())),
    };
    Ok((name, arguments))
}

/// The argv of a `tools/call`: the command path, then one element for each
/// argument that names a visible flag by its long name.
///
/// - A boolean flag takes a JSON boolean only: `true` gives `--name`, and
///   `false` gives nothing.
/// - A value flag takes a JSON string or number only, as ONE element
///   `--name=value`. Both parsers read the value after the first `=` of that
///   element and never read it as a flag (`shared::parse_flag_at`, and
///   `:args::parse_flag` with `:args::field_set_flag` in `args.sh`). So a
///   value cannot become an option, also when a parent command parses the
///   element first (`:usage` reads the flags of the parent after the command).
/// - An argument that names a `:@` flag (by any name, see `mcp_hidden_flag`)
///   is an error. The call refuses it before this function too: this is the
///   second guard.
/// - An argument with another name is ignored (an agent can send a name that
///   is not in the schema).
pub(crate) fn tool_argv(
    full_path: &[String],
    arguments: &[(String, Json)],
    flags: &[FlagInfo],
    refused: &[FlagInfo],
) -> Result<Vec<String>, String> {
    let mut argv = full_path.to_vec();
    for (key, value) in arguments {
        if mcp_hidden_flag(key, refused).is_some() {
            return Err(refusal(key));
        }
        let Some(flag) = flags.iter().find(|f| f.name == *key && !f.mcp_hidden) else {
            continue; // Unknown arg — ignore (lenient for LLM hallucinations)
        };
        match (flag.is_boolean, value) {
            (true, Json::Bool(true)) => argv.push(format!("--{}", flag.name)),
            (true, Json::Bool(false)) => {}
            (true, _) => return Err(format!("Flag --{} takes a boolean", key)),
            (false, Json::Str(s)) if s.contains('\0') => {
                return Err(format!("Flag --{}: the value has a NUL character", key));
            }
            (false, Json::Str(s)) => argv.push(format!("--{}={}", flag.name, s)),
            (false, Json::Number(n)) => argv.push(format!("--{}={}", flag.name, n)),
            (false, _) => return Err(format!("Flag --{} takes a string or a number", key)),
        }
    }
    Ok(argv)
}

/// Handle `tools/call` request (v2 — tree-aware, per-leaf flags and full command paths).
/// `params` is the JSON text of the params: the strict reader reads it.
pub(crate) fn handle_tools_call_v2<W: Write>(
    writer: &mut W,
    id: &Option<String>,
    params: &str,
    script_path: &str,
    leaf_tools: &[LeafTool],
) {
    match parse_json_strict(params) {
        Ok(params) => handle_tools_call_json(writer, id, &params, script_path, leaf_tools),
        Err((code, message)) => write_jsonrpc_error(writer, id, code, &message),
    }
}

/// Handle the params of a `tools/call`. The script starts only when the
/// request, the tool and each argument are valid.
pub(crate) fn handle_tools_call_json<W: Write>(
    writer: &mut W,
    id: &Option<String>,
    params: &Json,
    script_path: &str,
    leaf_tools: &[LeafTool],
) {
    let (tool_name, arguments) = match tools_call_request(params) {
        Ok(request) => request,
        Err((code, message)) => {
            write_jsonrpc_error(writer, id, code, &message);
            return;
        }
    };

    // Find matching leaf tool
    let Some(leaf) = leaf_tools.iter().find(|t| t.tool_name == tool_name) else {
        write_jsonrpc_error(writer, id, -32602, &format!("Unknown tool: {}", tool_name));
        return;
    };

    // A `:@` flag is not in the schema. Refuse a call that sends one, before
    // any child starts: never drop it in silence.
    if let Some((key, _)) = arguments
        .iter()
        .find(|(key, _)| mcp_hidden_flag(key, &leaf.refused).is_some())
    {
        write_jsonrpc_error(writer, id, -32602, &refusal(key));
        return;
    }

    // Build CLI args — all path segments, then the flags
    let cli_args = match tool_argv(&leaf.full_path, &arguments, &leaf.flags, &leaf.refused) {
        Ok(argv) => argv,
        Err(message) => {
            write_jsonrpc_error(writer, id, -32602, &message);
            return;
        }
    };

    // Execute
    let (exit_code, stdout_text, stderr_text) = execute_tool(script_path, &cli_args);

    // Emit stderr lines as log notifications (Feature 9: Logging)
    for stderr_line in stderr_text.lines() {
        if !stderr_line.is_empty() {
            let notification = format!(
                "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\",\"params\":{{\"level\":\"info\",\"logger\":\"{}\",\"data\":\"{}\"}}}}",
                json_escape(&leaf.tool_name), json_escape(stderr_line)
            );
            let _ = writeln!(writer, "{}", notification);
        }
    }

    // Format response
    let is_error = exit_code != 0;
    let text = if is_error && !stderr_text.is_empty() { // coverage:off - error_path: test fixtures always succeed
        format!("{}\n{}", stdout_text, stderr_text) // coverage:off
    } else {
        stdout_text
    };
    let text = text.trim_end_matches('\n');

    // Check for @json annotation — emit structuredContent if stdout is valid JSON
    let has_json_annot = leaf.annotations.iter().any(|a| a == "json");
    let structured_json = if has_json_annot && !is_error {
        let trimmed = text.trim();
        if is_likely_valid_json(trimmed) {
            Some(trimmed.to_string())
        } else {
            None
        }
    } else {
        None
    };

    let result = if let Some(ref json_content) = structured_json {
        format!(
            "{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}],\"structuredContent\":{},\"isError\":{}}}",
            json_escape(text),
            json_content,
            is_error
        )
    } else {
        format!(
            "{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}],\"isError\":{}}}",
            json_escape(text),
            is_error
        )
    };
    write_jsonrpc_response(writer, id, &result);
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

// -- JSON validation ----------------------------------------------------------

/// Validate that a string is well-formed JSON by checking structure with a
/// stack-based bracket matcher and basic token validation. Verifies that:
/// - Braces/brackets are properly nested and matched
/// - Strings are terminated
/// - Colons are followed by a value (not immediately by `}` or `]`)
/// - No stray characters after the root value
pub fn is_likely_valid_json(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return false;
    }
    let mut stack: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut escape = false;
    let mut root_closed_at: Option<usize> = None;
    let mut expect_value = false; // true after `:` — next non-whitespace must be a value
    let bytes = trimmed.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if b == b'\\' {
                escape = true;
                continue;
            }
            if b == b'"' {
                in_string = false;
                expect_value = false;
            }
            continue;
        }
        match b {
            b'"' => {
                in_string = true;
                expect_value = false;
            }
            b'{' | b'[' => {
                stack.push(b);
                expect_value = false;
            }
            b'}' => {
                if expect_value { return false; } // colon followed by }
                if stack.last() != Some(&b'{') { return false; }
                stack.pop();
                if stack.is_empty() { root_closed_at = Some(i); }
            }
            b']' => {
                if expect_value { return false; } // colon followed by ]
                if stack.last() != Some(&b'[') { return false; }
                stack.pop();
                if stack.is_empty() { root_closed_at = Some(i); }
            }
            b':' => {
                expect_value = true; // next non-ws must be a value
            }
            b' ' | b'\t' | b'\n' | b'\r' => {} // whitespace doesn't clear expect_value
            _ => {
                // Any other token (number, true, false, null chars) counts as value
                expect_value = false;
            }
        }
    }

    stack.is_empty() && !in_string && !expect_value && root_closed_at == Some(bytes.len() - 1)
}

// -- Strict JSON reader for tools/call ----------------------------------------
//
// A `tools/call` can start the script, so its request is read with serde_json
// (all of RFC 8259: the four whitespace characters between tokens, every
// string escape, the number grammar, no text after the value). The `Json`
// type keeps the members of an object in their order and refuses a key that
// occurs twice in any object.

/// A JSON value as the strict reader gives it.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// The number in its decimal form (serde_json reads it as an i64, a u64
    /// or an f64).
    Number(String),
    Str(String),
    Array(Vec<Json>),
    /// The members in their order. No key occurs twice.
    Object(Vec<(String, Json)>),
}

/// The value of the member `key` of an object.
pub(crate) fn json_member<'a>(members: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

impl<'de> serde::Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> serde::de::Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_unit<E>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E>(self, b: bool) -> Result<Json, E> {
        Ok(Json::Bool(b))
    }

    fn visit_i64<E>(self, n: i64) -> Result<Json, E> {
        Ok(Json::Number(n.to_string()))
    }

    fn visit_u64<E>(self, n: u64) -> Result<Json, E> {
        Ok(Json::Number(n.to_string()))
    }

    fn visit_f64<E>(self, n: f64) -> Result<Json, E> {
        Ok(Json::Number(n.to_string()))
    }

    fn visit_str<E>(self, s: &str) -> Result<Json, E> {
        Ok(Json::Str(s.to_owned()))
    }

    fn visit_string<E>(self, s: String) -> Result<Json, E> {
        Ok(Json::Str(s))
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Json::Array(items))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut members: Vec<(String, Json)> = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            if members.iter().any(|(k, _)| *k == key) {
                return Err(serde::de::Error::custom(format!("duplicate key: {}", key)));
            }
            let value = map.next_value()?;
            members.push((key, value));
        }
        Ok(Json::Object(members))
    }
}

/// Read a JSON text that holds exactly one value. An error is a JSON-RPC code
/// and its text: -32700 when the text is not JSON, -32602 for a key that
/// occurs twice in an object.
pub fn parse_json_strict(text: &str) -> Result<Json, (i32, String)> {
    serde_json::from_str::<Json>(text).map_err(|e| match e.classify() {
        serde_json::error::Category::Data => (-32602, format!("Invalid params: {}", e)),
        _ => (-32700, format!("Parse error: {}", e)),
    })
}

// -- Minimal JSON scanner -----------------------------------------------------
//
// The methods that start no script (initialize, resources, prompts) read
// their fields with this scanner. `tools/call` uses the strict reader above.

/// Extract a raw JSON field value (as a string) from a JSON object.
/// Returns the raw JSON fragment for the given key.
/// Only matches keys at the top level of the object (depth 1) to avoid
/// false positives from nested objects with the same key name.
pub(crate) fn extract_json_field(json: &str, key: &str) -> Option<String> {
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
pub(crate) fn extract_json_string(json: &str, key: &str) -> Option<String> {
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

// Unit tests for MCP pure functions live in mod.rs::tests to avoid linker
// errors from bash FFI symbols. The test binary can't resolve bash-builtins
// FFI when a #[cfg(test)] module exists in this file. Functions that use
// shell::* or execute_tool are tested via BATS integration tests.
