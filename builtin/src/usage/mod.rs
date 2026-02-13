//! :usage builtin -- subcommand dispatch with prefix resolution.
//!
//! Mirrors: libraries/args.sh (:usage function)

pub mod completion;
pub mod docgen;
pub mod mcp;

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::field;
use crate::shared;
use crate::shell;
use std::ffi::{c_char, c_int};
use std::io::Write;

// -- Builtin registration ---------------------------------------------------

static USAGE_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Parse subcommands from the usage array and dispatch.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage_struct"]
pub static mut USAGE_STRUCT: BashBuiltin = BashBuiltin {
    name: c":usage".as_ptr(),
    function: usage_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":usage <title> [args...]".as_ptr(),
    long_doc: USAGE_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage_builtin_load"]
pub extern "C" fn usage_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage_builtin_unload"]
pub extern "C" fn usage_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn usage_builtin_fn(word_list: *const WordList) -> c_int {
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        usage_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    // Errors terminate the script. Success (0) returns to the caller.
    // HELP_EXIT is only used by --argsh now (help is deferred via usage array).
    match code {
        0 => 0,
        shared::HELP_EXIT => std::process::exit(0), // coverage:off - exit() kills process before coverage flush
        n => std::process::exit(n), // coverage:off - exit() kills process before coverage flush
    }
}

// -- :usage::help builtin registration --------------------------------------

static USAGE_HELP_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Display deferred usage help text (called via ${usage[@]}).".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage::help_struct"]
pub static mut USAGE_HELP_STRUCT: BashBuiltin = BashBuiltin {
    name: c":usage::help".as_ptr(),
    function: usage_help_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":usage::help <title> [usage_pairs...]".as_ptr(),
    long_doc: USAGE_HELP_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage::help_builtin_load"]
pub extern "C" fn usage_help_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage::help_builtin_unload"]
pub extern "C" fn usage_help_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn usage_help_builtin_fn(word_list: *const WordList) -> c_int {
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        usage_help_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    // Help always exits the script after display
    std::process::exit(code) // coverage:off - exit() kills process before coverage flush
}

// -- :usage::help implementation ----------------------------------------------

/// Main entry point for :usage::help builtin.
/// Called via "${usage[@]}" after caller's setup code has run.
/// Args: title [original_usage_pairs...]
pub fn usage_help_main(args: &[String]) -> i32 {
    if args.is_empty() { // coverage:off - defensive_check: always called via deferred dispatch with title
        return shared::error_usage("", ":usage::help requires a title argument"); // coverage:off
    } // coverage:off

    let title = &args[0];
    let usage_pairs: Vec<String> = args[1..].to_vec();
    let args_arr = shell::read_array("args");

    usage_help_text(title, &usage_pairs, &args_arr);
    0
}

// -- :usage main implementation -----------------------------------------------

/// Main entry point for :usage builtin.
/// Returns exit code (0 = success, 2 = usage error).
pub fn usage_main(args: &[String]) -> i32 {
    if args.is_empty() {
        return shared::error_usage("", ":usage requires a title argument"); // coverage:off - set_e_kills: :usage always called with title from bash wrapper
    }

    let title = &args[0];
    let cli_args = &args[1..];

    // Read usage and args arrays from shell scope
    let usage_arr = shell::read_array("usage");
    let args_arr = shell::read_array("args");

    // Validate usage array is pairs (REVIEW finding 4: use % for Rust <1.87 compat)
    #[allow(clippy::manual_is_multiple_of)]
    if usage_arr.len() % 2 != 0 {
        return shared::error_usage("", "usage array must have an even number of elements");
    }

    // Handle empty args, -h, --help — defer to "${usage[@]}" dispatch
    if cli_args.is_empty()
        || cli_args[0] == "-h"
        || cli_args[0] == "--help"
    {
        // Set usage = (":usage::help" title original_usage_pairs...)
        // so help is generated at "${usage[@]}" time (after setup code runs).
        let mut new_usage = vec![":usage::help".to_string(), title.clone()];
        new_usage.extend_from_slice(&usage_arr);
        shell::write_array("usage", &new_usage);
        return 0;
    }

    // Handle --argsh
    let commandname = shell::get_commandname();
    if commandname.is_empty() && cli_args.first().map(|s| s.as_str()) == Some("--argsh") {
        let sha = shell::get_scalar("ARGSH_COMMIT_SHA").unwrap_or_default();
        let ver = shell::get_scalar("ARGSH_VERSION").unwrap_or_default();
        println!("https://arg.sh {} {}", sha, ver);
        return shared::HELP_EXIT;
    }

    // Parse flags and find command
    let mut cli: Vec<String> = cli_args.to_vec();
    let mut cmd: Option<String> = None;
    let mut matched: Vec<String> = Vec::new();

    // idx stays 0: we always process the front element; cli.remove(0) shifts the rest down
    let idx = 0;
    while idx < cli.len() {
        // Non-flag argument = command
        if !cli[idx].starts_with('-') {
            if cmd.is_some() {
                break; // Already have a command, rest goes to subcommand
            }
            cmd = Some(cli[idx].clone());
            cli.remove(idx);
            continue;
        }

        // Try parsing as flag -- set_bool for :usage uses set_or_increment
        match shared::parse_flag_at(&mut cli, idx, &args_arr, &mut matched, set_or_increment) {
            Ok(true) => {
                // idx stays the same since parse_flag_at modifies cli
            }
            Ok(false) => {
                break; // Unknown flag, leave for subcommand
            }
            Err(code) => {
                return code;
            }
        }
    }

    // Check required flags
    let ret = shared::check_required_flags(&args_arr, &matched);
    if ret != 0 {
        return ret;
    }

    let cmd = match cmd {
        Some(c) => c,
        None => {
            // No command given (e.g. flags only, or flags + --help) → defer to help
            let mut new_usage = vec![":usage::help".to_string(), title.clone()];
            new_usage.extend_from_slice(&usage_arr);
            shell::write_array("usage", &new_usage);
            return 0;
        }
    };

    // Lookup command in usage array
    let mut found_field = String::new();
    let mut func = String::new();

    for i in (0..usage_arr.len()).step_by(2) {
        let entry = &usage_arr[i];
        let entry_cmd_part = entry.split(':').next().unwrap_or(entry);
        let entry_clean = entry_cmd_part.strip_prefix('#').unwrap_or(entry_cmd_part);

        for alias in entry_clean.split('|') {
            if alias == cmd {
                found_field = entry.strip_prefix('#').unwrap_or(entry).to_string();

                // Check for explicit :- mapping
                if entry.contains(":-") {
                    func = entry.split(":-").nth(1).unwrap_or("").to_string();
                    func = func.strip_prefix('#').unwrap_or(&func).to_string();
                } else {
                    // Use first part (before |) as function name
                    func = entry_clean.split('|').next().unwrap_or("").to_string();
                }
                break;
            }
        }
        if !func.is_empty() {
            break;
        }
    }

    if func.is_empty() {
        // Command not found in usage array — check built-in special commands.
        // These are always available without being listed in the usage array.
        return match cmd.as_str() {
            "completion" | "docgen" | "mcp" => {
                defer_builtin_command(&cmd, title, &usage_arr, cli);
                0
            }
            _ => {
                let msg = match shared::suggest_command(&cmd, &usage_arr) {
                    Some(suggestion) => format!("Invalid command: {}. Did you mean '{}'?", cmd, suggestion),
                    None => format!("Invalid command: {}", cmd),
                };
                shared::error_usage(&cmd, &msg)
            }
        };
    }

    // Resolve function with prefix fallback
    let explicit = found_field.contains(":-");

    if explicit {
        if !shell::function_exists(&func) && !is_deferred_builtin(&func) {
            return shared::error_usage(&cmd, &format!("Invalid command: {}", cmd));
        }
    } else {
        // Resolution order:
        // 1) caller::func (FUNCNAME[0] since builtins don't push to FUNCNAME)
        // 2) argsh::func
        // 3) func (bare)
        let caller = shell::get_funcname(0);

        let mut resolved = false;

        if let Some(ref caller_name) = caller {
            let prefixed = format!("{}::{}", caller_name, func);
            if shell::function_exists(&prefixed) {
                func = prefixed;
                resolved = true;
            }
        } // coverage:off - LLVM artifact: closing brace gets 0 count despite block executing

        if !resolved {
            let argsh_prefixed = format!("argsh::{}", func);
            if shell::function_exists(&argsh_prefixed) {
                func = argsh_prefixed;
                resolved = true;
            }
        }

        if !resolved && !shell::function_exists(&func) {
            return shared::error_usage(&cmd, &format!("Invalid command: {}", cmd));
        }
    }

    // Check if the resolved function is a deferred :usage:: builtin
    if is_deferred_builtin(&func) {
        let cmd_name = found_field.split(['|', ':']).next().unwrap_or(&found_field);
        shell::append_commandname(cmd_name);
        let mut new_usage = vec![func];
        new_usage.extend(cli);
        new_usage.push("--".to_string());
        new_usage.push(title.clone());
        new_usage.extend_from_slice(&usage_arr);
        shell::write_array("usage", &new_usage);
        return 0;
    }

    // Append to COMMANDNAME
    let cmd_name = found_field.split(['|', ':']).next().unwrap_or(&found_field);
    shell::append_commandname(cmd_name);

    // Set usage = (func remaining_args...)
    let mut new_usage = vec![func];
    new_usage.extend(cli);
    shell::write_array("usage", &new_usage);

    0 // EXECUTION_SUCCESS
}

/// Check if a function name is a deferred :usage:: builtin (completion, docgen, mcp).
fn is_deferred_builtin(name: &str) -> bool {
    matches!(name, ":usage::completion" | ":usage::docgen" | ":usage::mcp")
}

/// Defer a built-in special command (completion, docgen, mcp) via the usage array.
fn defer_builtin_command(cmd: &str, title: &str, usage_arr: &[String], cli: Vec<String>) {
    let builtin_name = format!(":usage::{}", cmd);
    let mut new_usage = vec![builtin_name];
    new_usage.extend(cli);
    new_usage.push("--".to_string());
    new_usage.push(title.to_string());
    new_usage.extend_from_slice(usage_arr);
    shell::write_array("usage", &new_usage);
    shell::append_commandname(cmd);
}

fn set_or_increment(name: &str) {
    if shell::is_array(name) {
        shell::array_append(name, "1"); // coverage:off - dead_code: parse_flag_at handles array booleans directly, never calls set_bool for arrays
    } else {
        shell::set_scalar(name, "1");
    }
}

// -- Help text generation -----------------------------------------------------

/// Print usage help text.
fn usage_help_text(title: &str, usage_arr: &[String], args_arr: &[String]) {
    let out = std::io::stdout();
    let mut out = out.lock();
    let fw = shell::get_field_width();
    let commandname = shell::get_commandname();
    let cmdname_str = commandname.join(" ");

    // Title (trim leading whitespace per line)
    for line in title.lines() {
        let _ = writeln!(out, "{}", line.trim_start());
    }

    // Usage line
    let _ = writeln!(out);
    let _ = writeln!(out, "Usage: {} <command> [args]", cmdname_str);

    // Commands
    let first_is_group = usage_arr.first().map(|s| s.as_str()) == Some("-");
    if !first_is_group {
        let _ = writeln!(out, "\nAvailable Commands:");
    }

    for i in (0..usage_arr.len()).step_by(2) {
        let entry = &usage_arr[i];
        let desc = usage_arr.get(i + 1).map(|s| s.as_str()).unwrap_or("");

        // Hidden commands (# prefix)
        if entry.starts_with('#') {
            continue;
        }

        // Group separator
        if entry == "-" {
            let _ = writeln!(out);
            let _ = writeln!(out, "{}", desc);
            continue;
        }

        // Command name (before | or :)
        let name = entry.split(['|', ':']).next().unwrap_or(entry);
        let _ = writeln!(out, "  {:width$} {}", name, desc, width = fw);
    }

    // Flags section
    print_flags_section(&mut out, args_arr, fw);

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Use \"{} <command> --help\" for more information about a command.",
        cmdname_str
    );
}

/// Print the flags/options section (shared between :usage and :args help).
pub fn print_flags_section<W: Write>(out: &mut W, args_arr: &[String], _fw: usize) {
    // Build args with help added if not present
    let mut args_with_help: Vec<String> = args_arr.to_vec();
    let has_help = args_arr.iter().any(|s| s == "help|h:+");
    if !has_help {
        args_with_help.push("help|h:+".to_string());
        args_with_help.push("Show this help message".to_string());
    }

    // Find all flag indices
    let mut flag_indices: Vec<usize> = Vec::new();
    for i in (0..args_with_help.len()).step_by(2) {
        let entry = &args_with_help[i];
        if entry.contains('|') || entry == "-" {
            flag_indices.push(i);
        }
    }

    if flag_indices.is_empty() { // coverage:off - dead_code: help|h:+ always auto-added so flag_indices never empty
        return; // coverage:off
    } // coverage:off

    // Check if first flag is a group separator
    let first_is_group = args_with_help.get(flag_indices[0]).map(|s| s.as_str()) == Some("-");
    if !first_is_group {
        let _ = writeln!(out, "\nOptions:");
    }

    for &i in &flag_indices {
        let entry = &args_with_help[i];
        let desc = args_with_help.get(i + 1).map(|s| s.as_str()).unwrap_or("");

        // Hidden
        if entry.starts_with('#') {
            continue;
        }

        // Group separator
        if entry == "-" {
            let _ = writeln!(out);
            let _ = writeln!(out, "{}", desc);
            continue;
        }

        // Field format
        let def = match field::parse_field(entry) {
            Ok(d) => d,
            Err(e) => { // coverage:off - defensive_check: invalid fields caught during :args/:usage parsing before help display
                eprintln!("warning: invalid flag definition '{}': {}", entry, e); // coverage:off
                continue; // coverage:off
            } // coverage:off
        };
        let field_fmt = field::format_field(&def);
        let _ = writeln!(out, "{}", field_fmt);

        // Description (indented 11 spaces)
        let _ = writeln!(out, "           {}", desc);
    }
}

// -- Shared types -------------------------------------------------------------

/// Extracted subcommand info from usage pairs.
pub struct SubCmd {
    pub name: String,
    pub desc: String,
}

/// Extracted flag info from args array.
pub struct FlagInfo {
    pub name: String,
    pub short: Option<String>,
    pub desc: String,
    pub is_boolean: bool,
    pub type_name: String,
    pub required: bool,
}

// -- Shared extraction helpers ------------------------------------------------

/// Extract visible subcommands from usage pairs.
pub fn extract_subcommands(usage_pairs: &[String]) -> Vec<SubCmd> {
    let mut cmds = Vec::new();
    for i in (0..usage_pairs.len()).step_by(2) {
        let entry = &usage_pairs[i];
        let desc = usage_pairs.get(i + 1).map(|s| s.as_str()).unwrap_or("");

        // Skip hidden (#prefix) and group separators (-)
        if entry.starts_with('#') || entry == "-" {
            continue;
        }

        let name = entry.split(['|', ':']).next().unwrap_or(entry);
        cmds.push(SubCmd {
            name: name.to_string(),
            desc: desc.to_string(),
        });
    }
    cmds
}

/// Extract visible flags from args array.
pub fn extract_flags(args_arr: &[String]) -> Vec<FlagInfo> {
    let mut flags = Vec::new();
    let mut has_help = false;

    for i in (0..args_arr.len()).step_by(2) {
        let entry = &args_arr[i];
        let desc = args_arr.get(i + 1).map(|s| s.as_str()).unwrap_or("");

        // Only process flags (have | separator), skip positionals and group separators
        if !entry.contains('|') || entry == "-" || entry.starts_with('#') {
            continue;
        }

        if let Ok(def) = field::parse_field(entry) {
            if def.name == "help" {
                has_help = true;
            }
            flags.push(FlagInfo {
                name: def.display_name,
                short: def.short,
                desc: desc.to_string(),
                is_boolean: def.is_boolean,
                type_name: def.type_name,
                required: def.required,
            });
        }
    }

    if !has_help {
        flags.push(FlagInfo {
            name: "help".to_string(),
            short: Some("h".to_string()),
            desc: "Show this help message".to_string(),
            is_boolean: true,
            type_name: String::new(),
            required: false,
        });
    }

    flags
}

/// Extract visible flags from args array, excluding help (for LLM tool schemas).
pub fn extract_flags_for_llm(args_arr: &[String]) -> Vec<FlagInfo> {
    let mut flags = Vec::new();

    for i in (0..args_arr.len()).step_by(2) {
        let entry = &args_arr[i];
        let desc = args_arr.get(i + 1).map(|s| s.as_str()).unwrap_or("");

        if !entry.contains('|') || entry == "-" || entry.starts_with('#') {
            continue;
        }

        if let Ok(def) = field::parse_field(entry) {
            if def.name == "help" {
                continue;
            }
            flags.push(FlagInfo {
                name: def.display_name,
                short: def.short,
                desc: desc.to_string(),
                is_boolean: def.is_boolean,
                type_name: def.type_name,
                required: def.required,
            });
        }
    }

    flags
}

// -- Shared JSON/LLM helpers --------------------------------------------------

/// Escape a string for JSON string output.
pub fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Map argsh type names to JSON Schema types.
pub fn argsh_type_to_json(type_name: &str, is_boolean: bool) -> &'static str {
    if is_boolean {
        return "boolean";
    }
    match type_name {
        "int" => "integer",
        "float" => "number",
        _ => "string",
    }
}

/// Sanitize a string for use as a tool/function name (only [a-zA-Z0-9_-]).
pub fn sanitize_tool_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

/// Write JSON Schema properties and required array (shared by all LLM providers).
pub fn write_tool_properties<W: Write>(out: &mut W, flags: &[FlagInfo], indent: &str) {
    let _ = writeln!(out, "{}\"properties\": {{", indent);
    for (i, flag) in flags.iter().enumerate() {
        let json_type = argsh_type_to_json(&flag.type_name, flag.is_boolean);
        let trailing = if i < flags.len() - 1 { "," } else { "" };
        let _ = writeln!(out, "{}  \"{}\": {{", indent, json_escape(&flag.name));
        let _ = writeln!(out, "{}    \"type\": \"{}\",", indent, json_type);
        let _ = writeln!(out, "{}    \"description\": \"{}\"", indent, json_escape(&flag.desc));
        let _ = writeln!(out, "{}  }}{}", indent, trailing);
    }
    let _ = writeln!(out, "{}}},", indent);

    let required: Vec<&str> = flags
        .iter()
        .filter(|f| f.required)
        .map(|f| f.name.as_str())
        .collect();
    let _ = write!(out, "{}\"required\": [", indent);
    for (i, name) in required.iter().enumerate() {
        if i > 0 {
            let _ = write!(out, ", ");
        }
        let _ = write!(out, "\"{}\"", json_escape(name));
    }
    let _ = writeln!(out, "]");
}
