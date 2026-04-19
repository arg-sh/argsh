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

/// Parse subcommands from the usage array and dispatch to the matching handler.
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

/// Display formatted help text for subcommands defined in the usage array.
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
    let args_arr = field::dedup_inherited(&shell::read_array("args"));
    shell::write_array("args", &args_arr);

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

    // Read usage and args arrays from shell scope, dedup :^ inherited entries
    let usage_arr = shell::read_array("usage");
    let args_arr = field::dedup_inherited(&shell::read_array("args"));
    shell::write_array("args", &args_arr);

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
        let entry_cmd_part = entry_cmd_part.split('@').next().unwrap_or(entry_cmd_part);
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
        // 2) last_segment::func (last :: segment of caller)
        // 3) argsh::func
        // 4) func (bare)
        let caller = shell::get_funcname(0);

        let mut resolved = false;

        if let Some(ref caller_name) = caller {
            let prefixed = format!("{}::{}", caller_name, func);
            if shell::function_exists(&prefixed) {
                func = prefixed;
                resolved = true;
            }
        } // coverage:off - LLVM artifact: closing brace gets 0 count despite block executing

        // Second lookup: try last segment of caller as prefix
        if !resolved {
            if let Some(ref caller_name) = caller {
                if let Some(pos) = caller_name.rfind("::") {
                    let segment = &caller_name[pos + 2..];
                    let seg_prefixed = format!("{}::{}", segment, func);
                    if shell::function_exists(&seg_prefixed) {
                        func = seg_prefixed;
                        resolved = true;
                    }
                }
            }
        }

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
        let cmd_name = cmd_name.split('@').next().unwrap_or(cmd_name);
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
    let cmd_name = cmd_name.split('@').next().unwrap_or(cmd_name);
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

        // Command name (before | or : or @)
        let raw_name = entry.split(['|', ':']).next().unwrap_or(entry);
        let name = raw_name.split('@').next().unwrap_or(raw_name);
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

        // Hidden (# prefix or :# modifier)
        if entry.starts_with('#') || field::parse_field(entry).map(|f| f.is_hidden).unwrap_or(false) {
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

        let raw_name = entry.split(['|', ':']).next().unwrap_or(entry);
        let name = raw_name.split('@').next().unwrap_or(raw_name);
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

// -- Command tree walker ------------------------------------------------------

/// A node in the command tree. Each node represents a subcommand (or the root).
#[allow(dead_code)]
pub struct CommandNode {
    pub name: String,           // subcommand name (e.g. "up")
    pub desc: String,           // description
    pub full_path: Vec<String>, // full command path (e.g. ["cluster", "up"])
    pub flags: Vec<FlagInfo>,   // per-command flags from args array (excludes help)
    pub children: Vec<CommandNode>, // nested subcommands
    pub hidden: bool,           // #-prefixed entries
    pub annotations: Vec<String>, // e.g. ["readonly", "json"] from @readonly, @json suffixes
}

/// Parse `@` annotations from a usage entry name.
///
/// Format: `name@readonly`, `name@destructive@json` (multiple annotations).
/// Returns (entry_without_annotations, annotations_vec).
pub fn parse_entry_annotations(entry: &str) -> (String, Vec<String>) {
    if let Some(at_pos) = entry.find('@') {
        let name_part = entry[..at_pos].to_string();
        let annot_part = &entry[at_pos + 1..];
        let annotations: Vec<String> = annot_part
            .split('@')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        (name_part, annotations)
    } else {
        (entry.to_string(), Vec::new())
    }
}

/// Build a command tree by recursively discovering subcommands.
///
/// Starts from the top-level `usage_pairs` and for each subcommand, resolves
/// the actual function name, retrieves its body via `declare -f`, and parses
/// nested `usage` and `args` arrays from the function body text.
pub fn build_command_tree(
    usage_pairs: &[String],
    parent_args: &[String],
    caller: Option<&str>,
) -> Vec<CommandNode> {
    let parent_flags = extract_flags_for_llm(parent_args);
    let mut nodes = Vec::new();

    for i in (0..usage_pairs.len()).step_by(2) {
        let entry = &usage_pairs[i];
        let desc = usage_pairs.get(i + 1).map(|s| s.as_str()).unwrap_or("");

        // Skip group separators
        if entry == "-" {
            continue;
        }

        let hidden = entry.starts_with('#');
        let entry_clean = entry.strip_prefix('#').unwrap_or(entry);

        // Extract annotations from @ suffix (e.g. "serve@readonly" -> ["readonly"])
        let (entry_no_annot, annotations) = parse_entry_annotations(entry_clean);

        // Extract command name (before | or :)
        let entry_cmd_part = entry_no_annot.split(':').next().unwrap_or(&entry_no_annot);
        let name = entry_cmd_part.split('|').next().unwrap_or(entry_cmd_part);

        // Resolve function name using the same logic as :usage dispatch
        let func_name = resolve_function_name(&entry_no_annot, name, caller);
        let func_name = match func_name {
            Some(f) => f,
            None => {
                // Can't resolve — treat as leaf with parent flags
                nodes.push(CommandNode {
                    name: name.to_string(),
                    desc: desc.to_string(),
                    full_path: vec![name.to_string()],
                    flags: parent_flags.clone(),
                    children: Vec::new(),
                    hidden,
                    annotations: annotations.clone(),
                });
                continue;
            }
        };

        // Get function body via declare -f
        let body = get_function_body(&func_name);
        let body = match body {
            Some(b) => b,
            None => {
                // No body — treat as leaf with parent flags
                nodes.push(CommandNode {
                    name: name.to_string(),
                    desc: desc.to_string(),
                    full_path: vec![name.to_string()],
                    flags: parent_flags.clone(),
                    children: Vec::new(),
                    hidden,
                    annotations: annotations.clone(),
                });
                continue;
            }
        };

        // Parse usage and args arrays from the function body
        let sub_usage = parse_shell_array_from_body(&body, "usage");
        let sub_args = parse_shell_array_from_body(&body, "args");

        // Determine flags for this node: its own args if present, else inherit parent
        let own_flags = if sub_args.is_empty() {
            parent_flags.clone()
        } else {
            // Merge parent flags with own flags (own flags take precedence)
            let own = extract_flags_for_llm(&sub_args);
            let mut merged = own;
            // Add parent flags that aren't overridden by child
            for f in parent_flags.iter() {
                if !merged.iter().any(|existing| existing.name == f.name) {
                    merged.push(f.clone());
                }
            }
            merged
        };

        if sub_usage.is_empty() {
            // Leaf node
            nodes.push(CommandNode {
                name: name.to_string(),
                desc: desc.to_string(),
                full_path: vec![name.to_string()],
                flags: own_flags,
                children: Vec::new(),
                hidden,
                annotations: annotations.clone(),
            });
        } else {
            // Recurse into children
            let children = build_command_tree(&sub_usage, &sub_args, Some(&func_name));
            // Fix up children full_path to include this node's name
            let children: Vec<CommandNode> = children
                .into_iter()
                .map(|mut child| {
                    let mut path = vec![name.to_string()];
                    path.extend(child.full_path);
                    child.full_path = path;
                    child
                })
                .collect();

            nodes.push(CommandNode {
                name: name.to_string(),
                desc: desc.to_string(),
                full_path: vec![name.to_string()],
                flags: own_flags,
                children,
                hidden,
                annotations: annotations.clone(),
            });
        }
    }

    nodes
}

/// Resolve a function name using the same resolution logic as :usage dispatch.
fn resolve_function_name(entry: &str, name: &str, caller: Option<&str>) -> Option<String> {
    // Check for explicit :- mapping
    if entry.contains(":-") {
        let func = entry.split(":-").nth(1).unwrap_or("");
        let func = func.strip_prefix('#').unwrap_or(func).to_string();
        if shell::function_exists(&func) {
            return Some(func);
        }
        return None;
    }

    // Resolution order:
    // 1) caller::func
    // 2) last_segment::func
    // 3) argsh::func
    // 4) func (bare)
    if let Some(caller_name) = caller {
        let prefixed = format!("{}::{}", caller_name, name);
        if shell::function_exists(&prefixed) {
            return Some(prefixed);
        }

        // Last segment of caller
        if let Some(pos) = caller_name.rfind("::") {
            let segment = &caller_name[pos + 2..];
            let seg_prefixed = format!("{}::{}", segment, name);
            if shell::function_exists(&seg_prefixed) {
                return Some(seg_prefixed);
            }
        }
    }

    let argsh_prefixed = format!("argsh::{}", name);
    if shell::function_exists(&argsh_prefixed) {
        return Some(argsh_prefixed);
    }

    if shell::function_exists(name) {
        return Some(name.to_string());
    }

    None
}

/// Get a function's body text via `declare -f`.
fn get_function_body(func_name: &str) -> Option<String> {
    // Validate function name to prevent injection
    if !func_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-') {
        return None; // coverage:off - defensive_check: function names from usage arrays are always valid
    }
    shell::exec_capture(&format!("declare -f -- {}", func_name), "__argsh_r")
}

/// Parse a shell array from a `declare -f` function body.
///
/// Extracts the content of `local -a {name}=(...)` from the indented function
/// body that bash's `declare -f` produces. Returns alternating key-value pairs.
pub fn parse_shell_array_from_body(body: &str, array_name: &str) -> Vec<String> {
    // Look for patterns like:
    //     local -a usage=('cmd1' "desc1" 'cmd2' "desc2")
    // or multi-line:
    //     local -a usage=(
    //         'cmd1' "desc1"
    //         'cmd2' "desc2"
    //     )
    // Also handle `args+=(...)`-style appends and `local -a verbose args=(...)`
    // where there are extra variable names between `-a` and the target array.

    let needle_local = format!("{}=(", array_name);
    let needle_append = format!("{}+=(", array_name);

    let mut start_pos = None;
    for needle in &[&needle_local, &needle_append] {
        for (idx, matched) in body.match_indices(needle.as_str()) {
            // Verify this looks like a local -a or direct assignment context
            let prefix = &body[..idx];
            let last_line_start = prefix.rfind('\n').map(|p| p + 1).unwrap_or(0);
            let line_prefix = &body[last_line_start..idx];
            // Accept: "    local -a usage=", "    local -a verbose args=", "    args+=", etc.
            if line_prefix.contains("local") || line_prefix.trim_start().is_empty()
                || line_prefix.trim_start().starts_with(array_name)
            {
                start_pos = Some(idx + matched.len());
                break;
            }
        }
        if start_pos.is_some() {
            break;
        }
    }

    let start = match start_pos {
        Some(p) => p,
        None => return Vec::new(),
    };

    // Find matching closing paren
    let rest = &body[start..];
    let mut depth = 1;
    let mut end = 0;
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            b'\'' => {
                // Skip single-quoted string
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
            }
            b'"' => {
                // Skip double-quoted string
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 1; // skip escaped char
                    } else if bytes[i] == b'"' {
                        break;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if depth != 0 {
        return Vec::new(); // coverage:off - defensive_check: unmatched paren in declare -f output
    }

    let content = &rest[..end];
    parse_shell_words(content)
}

/// Parse shell words from array content (handles single and double-quoted strings).
fn parse_shell_words(content: &str) -> Vec<String> {
    let mut words = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip whitespace
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        if bytes[i] == b'\'' {
            // Single-quoted string
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'\'' {
                i += 1;
            }
            words.push(String::from_utf8_lossy(&bytes[start..i]).to_string());
            if i < bytes.len() {
                i += 1; // skip closing quote
            }
        } else if bytes[i] == b'"' {
            // Double-quoted string
            i += 1;
            let mut word = String::new();
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    // Handle escape
                    i += 1;
                    word.push(bytes[i] as char);
                } else {
                    word.push(bytes[i] as char);
                }
                i += 1;
            }
            words.push(word);
            if i < bytes.len() {
                i += 1; // skip closing quote
            }
        } else if bytes[i] != b'#' {
            // Unquoted word (stop at whitespace)
            let start = i;
            while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' && bytes[i] != b'\n' && bytes[i] != b'\r' {
                i += 1;
            }
            words.push(String::from_utf8_lossy(&bytes[start..i]).to_string());
        } else {
            // Comment — skip rest of line
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        }
    }

    words
}

/// Flatten a command tree to leaf nodes only (nodes with no children).
/// Skips hidden nodes.
pub fn flatten_leaves(nodes: &[CommandNode]) -> Vec<&CommandNode> {
    let mut leaves = Vec::new();
    for node in nodes {
        if node.hidden {
            continue;
        }
        if node.children.is_empty() {
            leaves.push(node);
        } else {
            leaves.extend(flatten_leaves(&node.children));
        }
    }
    leaves
}

/// Flatten a command tree to all visible nodes (both intermediate and leaf).
pub fn flatten_all(nodes: &[CommandNode]) -> Vec<&CommandNode> {
    let mut all = Vec::new();
    for node in nodes {
        if node.hidden {
            continue;
        }
        all.push(node);
        if !node.children.is_empty() {
            all.extend(flatten_all(&node.children));
        }
    }
    all
}

impl Clone for FlagInfo {
    fn clone(&self) -> Self {
        FlagInfo {
            name: self.name.clone(),
            short: self.short.clone(),
            desc: self.desc.clone(),
            is_boolean: self.is_boolean,
            type_name: self.type_name.clone(),
            required: self.required,
        }
    }
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

// -- Unit tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shell_words_single_quoted() {
        let words = parse_shell_words("'hello' 'world'");
        assert_eq!(words, vec!["hello", "world"]);
    }

    #[test]
    fn test_parse_shell_words_double_quoted() {
        let words = parse_shell_words("\"hello\" \"world\"");
        assert_eq!(words, vec!["hello", "world"]);
    }

    #[test]
    fn test_parse_shell_words_mixed() {
        let words = parse_shell_words("'cmd1' \"Description of cmd1\" 'cmd2' \"Description of cmd2\"");
        assert_eq!(words, vec!["cmd1", "Description of cmd1", "cmd2", "Description of cmd2"]);
    }

    #[test]
    fn test_parse_shell_words_unquoted() {
        let words = parse_shell_words("hello world");
        assert_eq!(words, vec!["hello", "world"]);
    }

    #[test]
    fn test_parse_shell_words_multiline() {
        let words = parse_shell_words("'cmd1' \"desc1\"\n    'cmd2' \"desc2\"");
        assert_eq!(words, vec!["cmd1", "desc1", "cmd2", "desc2"]);
    }

    #[test]
    fn test_parse_shell_array_from_body_usage() {
        let body = r#"cluster ()
{
    local -a usage=('up' "Start cluster" 'down' "Stop cluster");
    :usage "Cluster management" "${@}";
    "${usage[@]}"
}"#;
        let result = parse_shell_array_from_body(body, "usage");
        assert_eq!(result, vec!["up", "Start cluster", "down", "Stop cluster"]);
    }

    #[test]
    fn test_parse_shell_array_from_body_args() {
        let body = r#"serve ()
{
    local port;
    local -a args=('port|p:int' "Port number");
    :args "Start the server" "${@}";
    echo "serving on port ${port:-8080}"
}"#;
        let result = parse_shell_array_from_body(body, "args");
        assert_eq!(result, vec!["port|p:int", "Port number"]);
    }

    #[test]
    fn test_parse_shell_array_from_body_multiline() {
        let body = r#"main ()
{
    local -a usage=(
        'serve' "Start the server"
        'build' "Build the project"
    );
    :usage "My app" "${@}"
}"#;
        let result = parse_shell_array_from_body(body, "usage");
        assert_eq!(result, vec!["serve", "Start the server", "build", "Build the project"]);
    }

    #[test]
    fn test_parse_shell_array_from_body_not_found() {
        let body = r#"foo ()
{
    echo hello
}"#;
        let result = parse_shell_array_from_body(body, "usage");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_shell_array_with_extra_locals() {
        // bash declare -f output when there are extra vars before the array
        let body = r#"main ()
{
    local config;
    local -a verbose args=('verbose|v:+' "Enable verbose" 'config|c' "Config file");
    :args "test" "${@}"
}"#;
        let result = parse_shell_array_from_body(body, "args");
        assert_eq!(result, vec!["verbose|v:+", "Enable verbose", "config|c", "Config file"]);
    }

    #[test]
    fn test_parse_shell_array_from_body_append_syntax() {
        let body = r#"deploy ()
{
    local target;
    args+=('target|t' "Deploy target");
    :args "Deploy the app" "${@}"
}"#;
        let result = parse_shell_array_from_body(body, "args");
        assert_eq!(result, vec!["target|t", "Deploy target"]);
    }

    #[test]
    fn test_flatten_leaves_simple() {
        let nodes = vec![
            CommandNode {
                name: "serve".to_string(),
                desc: "Start".to_string(),
                full_path: vec!["serve".to_string()],
                flags: Vec::new(),
                children: Vec::new(),
                hidden: false,
                annotations: Vec::new(),
            },
            CommandNode {
                name: "build".to_string(),
                desc: "Build".to_string(),
                full_path: vec!["build".to_string()],
                flags: Vec::new(),
                children: Vec::new(),
                hidden: false,
                annotations: Vec::new(),
            },
        ];
        let leaves = flatten_leaves(&nodes);
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].name, "serve");
        assert_eq!(leaves[1].name, "build");
    }

    #[test]
    fn test_flatten_leaves_nested() {
        let nodes = vec![
            CommandNode {
                name: "cluster".to_string(),
                desc: "Cluster".to_string(),
                full_path: vec!["cluster".to_string()],
                flags: Vec::new(),
                children: vec![
                    CommandNode {
                        name: "up".to_string(),
                        desc: "Start".to_string(),
                        full_path: vec!["cluster".to_string(), "up".to_string()],
                        flags: Vec::new(),
                        children: Vec::new(),
                        hidden: false,
                        annotations: Vec::new(),
                    },
                    CommandNode {
                        name: "down".to_string(),
                        desc: "Stop".to_string(),
                        full_path: vec!["cluster".to_string(), "down".to_string()],
                        flags: Vec::new(),
                        children: Vec::new(),
                        hidden: false,
                        annotations: Vec::new(),
                    },
                ],
                hidden: false,
                annotations: Vec::new(),
            },
        ];
        let leaves = flatten_leaves(&nodes);
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].full_path, vec!["cluster", "up"]);
        assert_eq!(leaves[1].full_path, vec!["cluster", "down"]);
    }

    #[test]
    fn test_flatten_leaves_skips_hidden() {
        let nodes = vec![
            CommandNode {
                name: "visible".to_string(),
                desc: "Visible".to_string(),
                full_path: vec!["visible".to_string()],
                flags: Vec::new(),
                children: Vec::new(),
                hidden: false,
                annotations: Vec::new(),
            },
            CommandNode {
                name: "hidden".to_string(),
                desc: "Hidden".to_string(),
                full_path: vec!["hidden".to_string()],
                flags: Vec::new(),
                children: Vec::new(),
                hidden: true,
                annotations: Vec::new(),
            },
        ];
        let leaves = flatten_leaves(&nodes);
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].name, "visible");
    }

    #[test]
    fn test_flatten_all_nested() {
        let nodes = vec![
            CommandNode {
                name: "serve".to_string(),
                desc: "Start".to_string(),
                full_path: vec!["serve".to_string()],
                flags: Vec::new(),
                children: Vec::new(),
                hidden: false,
                annotations: Vec::new(),
            },
            CommandNode {
                name: "cluster".to_string(),
                desc: "Cluster".to_string(),
                full_path: vec!["cluster".to_string()],
                flags: Vec::new(),
                children: vec![
                    CommandNode {
                        name: "up".to_string(),
                        desc: "Start".to_string(),
                        full_path: vec!["cluster".to_string(), "up".to_string()],
                        flags: Vec::new(),
                        children: Vec::new(),
                        hidden: false,
                        annotations: Vec::new(),
                    },
                ],
                hidden: false,
                annotations: Vec::new(),
            },
        ];
        let all = flatten_all(&nodes);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "serve");
        assert_eq!(all[1].name, "cluster");
        assert_eq!(all[2].name, "up");
    }

    #[test]
    fn test_parse_entry_annotations_none() {
        let (name, annots) = parse_entry_annotations("serve");
        assert_eq!(name, "serve");
        assert!(annots.is_empty());
    }

    #[test]
    fn test_parse_entry_annotations_single() {
        let (name, annots) = parse_entry_annotations("serve@readonly");
        assert_eq!(name, "serve");
        assert_eq!(annots, vec!["readonly"]);
    }

    #[test]
    fn test_parse_entry_annotations_multiple() {
        let (name, annots) = parse_entry_annotations("build@destructive@json");
        assert_eq!(name, "build");
        assert_eq!(annots, vec!["destructive", "json"]);
    }

    #[test]
    fn test_parse_entry_annotations_with_alias() {
        let (name, annots) = parse_entry_annotations("serve|s@readonly");
        assert_eq!(name, "serve|s");
        assert_eq!(annots, vec!["readonly"]);
    }

    #[test]
    fn test_parse_entry_annotations_with_explicit_mapping() {
        let (name, annots) = parse_entry_annotations("serve:-my_serve@readonly");
        assert_eq!(name, "serve:-my_serve");
        assert_eq!(annots, vec!["readonly"]);
    }

    // -- MCP pure function tests (live here to avoid bash FFI linker errors) --

    #[test]
    fn test_mcp_format_tool_no_flags_no_annotations() {
        let result = mcp::format_tool("my_tool", "A test tool", &[], &[]);
        assert!(result.contains("\"name\":\"my_tool\""));
        assert!(result.contains("\"title\":\"A test tool\""));
        assert!(result.contains("\"description\":\"A test tool\""));
        assert!(result.contains("\"additionalProperties\":false"));
        assert!(!result.contains("\"properties\""));
        assert!(!result.contains("\"required\""));
        assert!(!result.contains("\"annotations\""));
        assert!(!result.contains("\"outputSchema\""));
    }

    #[test]
    fn test_mcp_format_tool_with_flags() {
        let flags = vec![FlagInfo {
            name: "port".to_string(),
            short: Some("p".to_string()),
            desc: "Port number".to_string(),
            is_boolean: false,
            type_name: "int".to_string(),
            required: false,
        }];
        let result = mcp::format_tool("serve", "Start server", &flags, &[]);
        assert!(result.contains("\"port\":{\"type\":\"integer\""));
        assert!(result.contains("\"additionalProperties\":false"));
        assert!(result.contains("\"title\":\"Start server\""));
    }

    #[test]
    fn test_mcp_format_tool_readonly_annotation() {
        let result = mcp::format_tool("serve", "desc", &[], &["readonly".to_string()]);
        assert!(result.contains("\"annotations\":{\"readOnlyHint\":true}"));
    }

    #[test]
    fn test_mcp_format_tool_destructive_annotation() {
        let result = mcp::format_tool("build", "desc", &[], &["destructive".to_string()]);
        assert!(result.contains("\"annotations\":{\"destructiveHint\":true}"));
    }

    #[test]
    fn test_mcp_format_tool_idempotent_annotation() {
        let result = mcp::format_tool("up", "desc", &[], &["idempotent".to_string()]);
        assert!(result.contains("\"annotations\":{\"idempotentHint\":true}"));
    }

    #[test]
    fn test_mcp_format_tool_openworld_annotation() {
        let result = mcp::format_tool("search", "desc", &[], &["openworld".to_string()]);
        assert!(result.contains("\"annotations\":{\"openWorldHint\":true}"));
    }

    #[test]
    fn test_mcp_format_tool_json_annotation() {
        let result = mcp::format_tool("status", "desc", &[], &["json".to_string()]);
        assert!(result.contains("\"outputSchema\":{}"));
        assert!(!result.contains("\"annotations\""));
    }

    #[test]
    fn test_mcp_format_tool_multiple_annotations() {
        let annots = vec!["readonly".to_string(), "json".to_string()];
        let result = mcp::format_tool("status", "desc", &[], &annots);
        assert!(result.contains("\"outputSchema\":{}"));
        assert!(result.contains("\"annotations\":{\"readOnlyHint\":true}"));
    }

    #[test]
    fn test_mcp_prompts_list() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        mcp::handle_prompts_list(&mut buf, &id);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"run_subcommand\""));
        assert!(output.contains("\"get_help\""));
    }

    #[test]
    fn test_mcp_prompts_get_run_subcommand() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let params = r#"{"name":"run_subcommand","arguments":{"subcommand":"serve","args":"--port 8080"}}"#;
        mcp::handle_prompts_get(&mut buf, &id, params, "myapp");
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Run the 'serve' subcommand of myapp"));
        assert!(output.contains("With arguments: --port 8080"));
    }

    #[test]
    fn test_mcp_prompts_get_help_no_subcmd() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let params = r#"{"name":"get_help","arguments":{}}"#;
        mcp::handle_prompts_get(&mut buf, &id, params, "myapp");
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Show the full help output for myapp"));
    }

    #[test]
    fn test_mcp_prompts_get_help_with_subcmd() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let params = r#"{"name":"get_help","arguments":{"subcommand":"serve"}}"#;
        mcp::handle_prompts_get(&mut buf, &id, params, "myapp");
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Show help for the 'serve' subcommand of myapp"));
    }

    #[test]
    fn test_mcp_prompts_get_unknown() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let params = r#"{"name":"nonexistent","arguments":{}}"#;
        mcp::handle_prompts_get(&mut buf, &id, params, "myapp");
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Unknown prompt"));
    }

    #[test]
    fn test_mcp_resources_list() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        mcp::handle_resources_list(&mut buf, &id, "myapp");
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("script:///help"));
        assert!(output.contains("script:///version"));
        assert!(output.contains("Help output for myapp"));
    }

    #[test]
    fn test_mcp_write_jsonrpc_response() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        mcp::write_jsonrpc_response(&mut buf, &id, "{}");
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"result\":{}"));
    }

    #[test]
    fn test_mcp_tools_list_v2_with_annotations() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let leaf_tools = vec![
            mcp::LeafTool {
                tool_name: "myapp_serve".to_string(),
                full_path: vec!["serve".to_string()],
                desc: "Start the server".to_string(),
                flags: Vec::new(),
                annotations: vec!["readonly".to_string()],
            },
            mcp::LeafTool {
                tool_name: "myapp_status".to_string(),
                full_path: vec!["status".to_string()],
                desc: "Get status".to_string(),
                flags: Vec::new(),
                annotations: vec!["json".to_string()],
            },
        ];
        mcp::handle_tools_list_v2(&mut buf, &id, "My app", &leaf_tools);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"readOnlyHint\":true"));
        assert!(output.contains("\"outputSchema\":{}"));
        assert!(output.contains("\"title\":\"Start the server\""));
        assert!(output.contains("\"title\":\"Get status\""));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_object() {
        assert!(mcp::is_likely_valid_json(r#"{"key":"value"}"#));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_array() {
        assert!(mcp::is_likely_valid_json(r#"[1,2,3]"#));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_nested() {
        assert!(mcp::is_likely_valid_json(r#"{"a":{"b":[1,2]}}"#));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_empty_object() {
        assert!(mcp::is_likely_valid_json("{}"));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_empty_array() {
        assert!(mcp::is_likely_valid_json("[]"));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_with_whitespace() {
        assert!(mcp::is_likely_valid_json("  { \"key\": \"value\" }  "));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_unbalanced_brace() {
        assert!(!mcp::is_likely_valid_json("{\"key\":\"value\""));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_extra_close() {
        assert!(!mcp::is_likely_valid_json("{}}"));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_braces_in_string() {
        assert!(mcp::is_likely_valid_json(r#"{"msg":"hello {world}"}"#));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_not_json() {
        assert!(!mcp::is_likely_valid_json("hello world"));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_empty() {
        assert!(!mcp::is_likely_valid_json(""));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_starts_with_brace_but_invalid() {
        // Starts with { ends with } but unbalanced inside
        assert!(!mcp::is_likely_valid_json(r#"{ "a": ] }"#));
    }

    #[test]
    fn test_mcp_is_likely_valid_json_unterminated_string() {
        assert!(!mcp::is_likely_valid_json(r#"{"key":"value}"#));
    }

    #[test]
    fn test_mcp_prompts_get_run_subcommand_missing_arg() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let params = r#"{"name":"run_subcommand","arguments":{}}"#;
        mcp::handle_prompts_get(&mut buf, &id, params, "myapp");
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("-32602"));
        assert!(output.contains("Missing required argument: subcommand"));
    }

    #[test]
    fn test_mcp_extract_json_field_nested() {
        let json = r#"{"jsonrpc":"2.0","method":"prompts/get","params":{"name":"get_help","arguments":{"subcommand":"serve"}}}"#;
        let params = mcp::extract_json_field(json, "params").unwrap();
        assert!(params.contains("\"name\":\"get_help\""));
        let name = mcp::extract_json_string(&params, "name").unwrap();
        assert_eq!(name, "get_help");
        let args = mcp::extract_json_field(&params, "arguments").unwrap();
        let subcmd = mcp::extract_json_string(&args, "subcommand").unwrap();
        assert_eq!(subcmd, "serve");
    }

    #[test]
    fn test_mcp_resources_read_help() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let tools = vec![
            mcp::LeafTool {
                tool_name: "app_serve".to_string(),
                full_path: vec!["serve".to_string()],
                desc: "Start server".to_string(),
                flags: vec![],
                annotations: vec![],
            },
            mcp::LeafTool {
                tool_name: "app_build".to_string(),
                full_path: vec!["build".to_string()],
                desc: "Build project".to_string(),
                flags: vec![],
                annotations: vec![],
            },
        ];
        mcp::handle_resources_read(
            &mut buf, &id,
            r#"{"uri":"script:///help"}"#,
            "app", "My app", &tools,
        );
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("My app"));
        assert!(output.contains("serve"));
        assert!(output.contains("build"));
    }

    #[test]
    fn test_mcp_resources_read_version() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let tools = vec![];
        mcp::handle_resources_read(
            &mut buf, &id,
            r#"{"uri":"script:///version"}"#,
            "app", "My app", &tools,
        );
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("\"result\""));
        assert!(output.contains("script:///version"));
    }

    #[test]
    fn test_mcp_resources_read_unknown_uri() {
        let mut buf = Vec::new();
        let id = Some("1".to_string());
        let tools = vec![];
        mcp::handle_resources_read(
            &mut buf, &id,
            r#"{"uri":"script:///nonexistent"}"#,
            "app", "My app", &tools,
        );
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("-32602"));
        assert!(output.contains("Unknown resource URI"));
    }
}
