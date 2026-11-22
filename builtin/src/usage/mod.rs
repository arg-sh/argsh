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
        if hidden_from_help(entry) {
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

/// A flag that `--help` does not show: a `#` prefix or the `:#` modifier.
/// The `:@` modifier does not hide a flag from `--help`.
pub fn hidden_from_help(entry: &str) -> bool {
    entry.starts_with('#')
        || field::parse_field_spec(entry)
            .map(|f| f.is_hidden)
            .unwrap_or(false)
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
    /// `:@` modifier: an agent tool schema (MCP `tools/list`, `docgen llm`)
    /// does not list the flag, and the MCP server refuses a call that sends it.
    pub mcp_hidden: bool,
    /// `:^` modifier: the entry yields to an entry of the same name without it.
    pub inherited: bool,
    /// `#` prefix: `--help` does not show the flag. No agent schema lists it.
    pub help_hidden: bool,
    /// Every name that the parser accepts: long, short and any `|` alias.
    pub names: Vec<String>,
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
                mcp_hidden: def.is_mcp_hidden,
                inherited: def.is_inherited,
                help_hidden: false,
                names: field::field_names(entry),
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
            mcp_hidden: false,
            inherited: false,
            help_hidden: false,
            names: vec!["help".to_string(), "h".to_string()],
        });
    }

    flags
}

/// The flags of an args array for the agent paths (MCP, `docgen llm`), in
/// the order of the array and merged by `merge_flags`: every flag entry but
/// help, also a `#` one (with `help_hidden` set). A `:@` flag stays in the
/// list with `mcp_hidden` set: the MCP server refuses it by this list.
pub fn agent_flags(args_arr: &[String]) -> Vec<FlagInfo> {
    merge_flags(flag_entries(args_arr))
}

/// Extract visible flags from args array, excluding help (for LLM tool schemas).
/// A `#` flag is not in the list. A `:@` flag stays in the list with
/// `mcp_hidden` set: the command tree and the docs read this list too. The
/// agent schema writers (`mcp::format_tool`, `write_tool_properties`) leave
/// it out.
pub fn extract_flags_for_llm(args_arr: &[String]) -> Vec<FlagInfo> {
    without_help_hidden(&agent_flags(args_arr))
}

/// The flags of a list that `--help` shows (no `#` flag).
fn without_help_hidden(flags: &[FlagInfo]) -> Vec<FlagInfo> {
    flags.iter().filter(|f| !f.help_hidden).cloned().collect()
}

/// One `FlagInfo` for each flag entry of an args array, in their order, not
/// merged. Not help, a positional or a group separator.
fn flag_entries(args_arr: &[String]) -> Vec<FlagInfo> {
    let mut flags = Vec::new();

    for i in (0..args_arr.len()).step_by(2) {
        let entry = &args_arr[i];
        let desc = args_arr.get(i + 1).map(|s| s.as_str()).unwrap_or("");

        if !entry.contains('|') || entry == "-" {
            continue;
        }

        if let Ok(def) = field::parse_field_spec(entry) {
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
                mcp_hidden: def.is_mcp_hidden,
                inherited: def.is_inherited,
                help_hidden: entry.starts_with('#'),
                names: field::field_names(entry),
            });
        }
    }

    flags
}

/// The variable name of a flag: the parser merges the entries of one name.
fn var_key(flag: &FlagInfo) -> String {
    flag.name.replace('-', "_")
}

/// Give `kept` the `:@` bit and the names of `other`, an entry of the same
/// variable name. The flag is `:@` if any of its entries is (fail closed):
/// `:usage` of a parent parses the flags of the parent also after the command
/// name, so the parent's entry can apply although the child's entry wins.
fn absorb(kept: &mut FlagInfo, other: &FlagInfo) {
    kept.mcp_hidden |= other.mcp_hidden;
    for name in &other.names {
        if !kept.names.contains(name) {
            kept.names.push(name.clone());
        }
    }
}

/// Merge the entries of each variable name as `field::dedup_inherited` does:
/// the last entry without `:^` wins, else the last entry. The flag stays at
/// the place of the entry that wins, and gets the `:@` bit and the names of
/// all the entries (`absorb`).
pub fn merge_flags(entries: Vec<FlagInfo>) -> Vec<FlagInfo> {
    let keys: Vec<String> = entries.iter().map(var_key).collect();
    let mut merged = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let same: Vec<usize> = (0..entries.len()).filter(|&j| keys[j] == keys[i]).collect();
        let winner = same
            .iter()
            .rev()
            .find(|&&j| !entries[j].inherited)
            .or(same.last());
        if winner != Some(&i) {
            continue;
        }
        let mut kept = entry.clone();
        for (j, other) in entries.iter().enumerate() {
            if j != i && keys[j] == keys[i] {
                absorb(&mut kept, other);
            }
        }
        merged.push(kept);
    }
    merged
}

/// A `"${args[@]}"` word (also `"${args[@]:-}"`) in a child's args array as
/// `declare -f` shows it: the entries of the parent go at that place.
fn is_parent_expansion(word: &str) -> bool {
    word.starts_with("${args[@]") && word.ends_with('}')
}

/// The flags of a child command as the parser sees them, from its args array
/// (as `declare -f` shows it) and the flags of its parent.
///
/// - With a parent expansion in the array, or with `args+=(...)` (the entries
///   of the parent come first), the parser reads the child's entries and the
///   parent's entries in that order: `merge_flags`.
/// - Else the parser reads the child's entries only. The list still gets the
///   flags of the parent that the child does not have (as before), and a flag
///   of the child gets the `:@` bit and the names of a parent flag of the
///   same name (`absorb`).
pub fn child_flags(sub_args: &[String], appended: bool, parent: &[FlagInfo]) -> Vec<FlagInfo> {
    let mut own = Vec::new();
    let mut at = if appended { Some(0) } else { None };
    for word in sub_args {
        if is_parent_expansion(word) {
            // The first expansion only, at the start of its pair
            if at.is_none() {
                at = Some(own.len() / 2 * 2);
            }
        } else {
            own.push(word.clone());
        }
    }
    match at {
        Some(at) => {
            let mut entries = flag_entries(&own[..at]);
            entries.extend(parent.iter().cloned());
            entries.extend(flag_entries(&own[at..]));
            merge_flags(entries)
        }
        None => {
            let mut merged = merge_flags(flag_entries(&own));
            for p in parent {
                match merged.iter_mut().find(|f| var_key(f) == var_key(p)) {
                    Some(f) => absorb(f, p),
                    None => merged.push(p.clone()),
                }
            }
            merged
        }
    }
}

// -- Command tree walker ------------------------------------------------------

/// A node in the command tree. Each node represents a subcommand (or the root).
#[allow(dead_code)]
pub struct CommandNode {
    pub name: String,           // subcommand name (e.g. "up")
    pub desc: String,           // description
    pub full_path: Vec<String>, // full command path (e.g. ["cluster", "up"])
    pub flags: Vec<FlagInfo>,   // per-command flags from args array (excludes help)
    pub refused: Vec<FlagInfo>, // `:@` flags, also of a parent: MCP refuses them
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

/// Where `build_tree` finds the function of a command and its body.
pub(crate) trait Functions {
    fn exists(&self, name: &str) -> bool;
    fn body(&self, name: &str) -> Option<String>;
}

/// The functions of the running shell.
struct ShellFunctions;

impl Functions for ShellFunctions {
    fn exists(&self, name: &str) -> bool {
        shell::function_exists(name)
    }

    fn body(&self, name: &str) -> Option<String> {
        get_function_body(name)
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
    build_tree(&ShellFunctions, usage_pairs, &agent_flags(parent_args), caller)
}

/// The tree below one command. `parent` holds the flags of that command as
/// the parser sees them, `#` flags too (`agent_flags`). Each child gets them
/// merged with its own (`child_flags`), and its children get that merged
/// list: a `:@` flag of any level above stays refused.
pub(crate) fn build_tree<F: Functions>(
    funcs: &F,
    usage_pairs: &[String],
    parent: &[FlagInfo],
    caller: Option<&str>,
) -> Vec<CommandNode> {
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

        // Resolve the function name with the logic of :usage dispatch. A
        // command without a function or a body is a leaf with the parent's
        // flags.
        let func_name = resolve_function_name(funcs, &entry_no_annot, name, caller);
        let body = func_name.as_deref().and_then(|f| funcs.body(f));
        let (Some(func_name), Some(body)) = (func_name, body) else {
            nodes.push(CommandNode {
                name: name.to_string(),
                desc: desc.to_string(),
                full_path: vec![name.to_string()],
                flags: without_help_hidden(parent),
                refused: mcp::refused_flags(parent),
                children: Vec::new(),
                hidden,
                annotations: annotations.clone(),
            });
            continue;
        };

        // Parse usage and args arrays from the function body
        let sub_usage = parse_shell_array_from_body(&body, "usage");
        let (sub_args, appended) = parse_shell_array_and_form(&body, "args");

        // The flags of this node: the parent's, merged with its own
        let all = if sub_args.is_empty() {
            parent.to_vec()
        } else {
            child_flags(&sub_args, appended, parent)
        };

        let children = if sub_usage.is_empty() {
            Vec::new()
        } else {
            // Recurse with the merged flags, and put this node's name in
            // front of each child path
            build_tree(funcs, &sub_usage, &all, Some(&func_name))
                .into_iter()
                .map(|mut child| {
                    let mut path = vec![name.to_string()];
                    path.extend(child.full_path);
                    child.full_path = path;
                    child
                })
                .collect()
        };

        nodes.push(CommandNode {
            name: name.to_string(),
            desc: desc.to_string(),
            full_path: vec![name.to_string()],
            flags: without_help_hidden(&all),
            refused: mcp::refused_flags(&all),
            children,
            hidden,
            annotations: annotations.clone(),
        });
    }

    nodes
}

/// Resolve a function name using the same resolution logic as :usage dispatch.
fn resolve_function_name<F: Functions>(
    funcs: &F,
    entry: &str,
    name: &str,
    caller: Option<&str>,
) -> Option<String> {
    // Check for explicit :- mapping
    if entry.contains(":-") {
        let func = entry.split(":-").nth(1).unwrap_or("");
        let func = func.strip_prefix('#').unwrap_or(func).to_string();
        if funcs.exists(&func) {
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
        if funcs.exists(&prefixed) {
            return Some(prefixed);
        }

        // Last segment of caller
        if let Some(pos) = caller_name.rfind("::") {
            let segment = &caller_name[pos + 2..];
            let seg_prefixed = format!("{}::{}", segment, name);
            if funcs.exists(&seg_prefixed) {
                return Some(seg_prefixed);
            }
        }
    }

    let argsh_prefixed = format!("argsh::{}", name);
    if funcs.exists(&argsh_prefixed) {
        return Some(argsh_prefixed);
    }

    if funcs.exists(name) {
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
    parse_shell_array_and_form(body, array_name).0
}

/// `parse_shell_array_from_body`, and true if the array is an append
/// (`{name}+=(...)`): then the words come after the entries that the array
/// has already.
pub(crate) fn parse_shell_array_and_form(body: &str, array_name: &str) -> (Vec<String>, bool) {
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
    let mut appended = false;
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
                appended = *needle == &needle_append;
                break;
            }
        }
        if start_pos.is_some() {
            break;
        }
    }

    let start = match start_pos {
        Some(p) => p,
        None => return (Vec::new(), false),
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
        return (Vec::new(), false); // coverage:off - defensive_check: unmatched paren in declare -f output
    }

    let content = &rest[..end];
    (parse_shell_words(content), appended)
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
            mcp_hidden: self.mcp_hidden,
            inherited: self.inherited,
            help_hidden: self.help_hidden,
            names: self.names.clone(),
        }
    }
}

// -- Shared JSON/LLM helpers --------------------------------------------------

/// Escape a string for JSON string output. Every control character gets an
/// escape: an MCP error can echo a key that a caller sent.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
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
/// A `:@` flag is not written: the schema is for an agent.
pub fn write_tool_properties<W: Write>(out: &mut W, flags: &[FlagInfo], indent: &str) {
    let flags: Vec<&FlagInfo> = flags.iter().filter(|f| !f.mcp_hidden).collect();
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
                refused: Vec::new(),
                children: Vec::new(),
                hidden: false,
                annotations: Vec::new(),
            },
            CommandNode {
                name: "build".to_string(),
                desc: "Build".to_string(),
                full_path: vec!["build".to_string()],
                flags: Vec::new(),
                refused: Vec::new(),
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
                refused: Vec::new(),
                children: vec![
                    CommandNode {
                        name: "up".to_string(),
                        desc: "Start".to_string(),
                        full_path: vec!["cluster".to_string(), "up".to_string()],
                        flags: Vec::new(),
                        refused: Vec::new(),
                        children: Vec::new(),
                        hidden: false,
                        annotations: Vec::new(),
                    },
                    CommandNode {
                        name: "down".to_string(),
                        desc: "Stop".to_string(),
                        full_path: vec!["cluster".to_string(), "down".to_string()],
                        flags: Vec::new(),
                        refused: Vec::new(),
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
                refused: Vec::new(),
                children: Vec::new(),
                hidden: false,
                annotations: Vec::new(),
            },
            CommandNode {
                name: "hidden".to_string(),
                desc: "Hidden".to_string(),
                full_path: vec!["hidden".to_string()],
                flags: Vec::new(),
                refused: Vec::new(),
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
                refused: Vec::new(),
                children: Vec::new(),
                hidden: false,
                annotations: Vec::new(),
            },
            CommandNode {
                name: "cluster".to_string(),
                desc: "Cluster".to_string(),
                full_path: vec!["cluster".to_string()],
                flags: Vec::new(),
                refused: Vec::new(),
                children: vec![
                    CommandNode {
                        name: "up".to_string(),
                        desc: "Start".to_string(),
                        full_path: vec!["cluster".to_string(), "up".to_string()],
                        flags: Vec::new(),
                        refused: Vec::new(),
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
            mcp_hidden: false,
            inherited: false,
            help_hidden: false,
            names: vec!["port".to_string(), "p".to_string()],
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
                refused: Vec::new(),
                annotations: vec!["readonly".to_string()],
            },
            mcp::LeafTool {
                tool_name: "myapp_status".to_string(),
                full_path: vec!["status".to_string()],
                desc: "Get status".to_string(),
                flags: Vec::new(),
                refused: Vec::new(),
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
                refused: Vec::new(),
                annotations: vec![],
            },
            mcp::LeafTool {
                tool_name: "app_build".to_string(),
                full_path: vec!["build".to_string()],
                desc: "Build project".to_string(),
                flags: vec![],
                refused: Vec::new(),
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

    // -- The `:@` modifier: a flag that agents do not get --

    /// A command with a `:@` flag (with a short name and an alias), a value
    /// flag and a boolean flag.
    fn agent_args() -> Vec<String> {
        [
            "print-token|t|show-token:+@",
            "Print the access token",
            "name|n",
            "Name to log in as",
            "verbose|v:+",
            "Verbose output",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// The tool of `agent_tools_call`: its "script" is `/bin/sh -c <probe>`.
    /// The probe writes its argv to `<dir>/argv` and prints "ran", so a test
    /// can see if a child started.
    fn agent_leaf(dir: &std::path::Path, flags: Vec<FlagInfo>) -> mcp::LeafTool {
        let probe = format!(
            "printf '%s\\n' \"$@\" > '{}'; echo ran",
            dir.join("argv").display()
        );
        mcp::LeafTool {
            tool_name: "app_login".to_string(),
            full_path: vec!["-c".to_string(), probe, "probe".to_string()],
            desc: "Log in".to_string(),
            refused: mcp::refused_flags(&flags),
            flags,
            annotations: vec![],
        }
    }

    /// Send a tools/call with these params (JSON text) to the probe tool.
    /// Returns the JSON-RPC output and the argv of the child, or None when no
    /// child ran.
    fn agent_tools_call_params(label: &str, params: &str) -> (String, Option<String>) {
        let dir =
            std::env::temp_dir().join(format!("argsh-mcp-hidden-{}-{}", std::process::id(), label));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let leaf = vec![agent_leaf(&dir, extract_flags_for_llm(&agent_args()))];
        let mut buf = Vec::new();
        mcp::handle_tools_call_v2(&mut buf, &Some("7".to_string()), params, "/bin/sh", &leaf);
        let ran = std::fs::read_to_string(dir.join("argv")).ok();
        std::fs::remove_dir_all(&dir).unwrap();
        (String::from_utf8(buf).unwrap(), ran)
    }

    /// Send a tools/call with these arguments (JSON text) to the probe tool.
    fn agent_tools_call(label: &str, arguments: &str) -> (String, Option<String>) {
        let params = format!(r#"{{"name":"app_login","arguments":{}}}"#, arguments);
        agent_tools_call_params(label, &params)
    }

    /// Assert a JSON-RPC error with this code and text, and that no child ran.
    fn assert_refused(what: &str, output: &str, ran: &Option<String>, code: i32, text: &str) {
        assert!(output.contains(&format!("\"code\":{}", code)), "{}: {}", what, output);
        assert!(output.contains(text), "{}: {}", what, output);
        assert!(!output.contains("\"result\""), "{}: {}", what, output);
        assert_eq!(*ran, None, "{}: a child ran", what);
    }

    #[test]
    fn test_mcp_hidden_flag_stays_in_help() {
        let def = field::parse_field_spec("print-token|t:+@").unwrap();
        assert!(def.is_mcp_hidden);
        assert!(!def.is_hidden);
        assert!(!hidden_from_help("print-token|t:+@"));
        // `#` and `:#` still hide a flag from --help
        assert!(hidden_from_help("#debug|d:+"));
        assert!(hidden_from_help("debug|d:+#"));
        assert!(!hidden_from_help("name|n"));
    }

    #[test]
    fn test_mcp_hidden_flag_parses_on_the_command_line() {
        let def = field::parse_field_spec("print-token|t|show-token:+@").unwrap();
        assert!(def.is_boolean);
        assert_eq!(def.short.as_deref(), Some("t"));
        // `@` ends a type name, in any order with the other modifiers
        let def = field::parse_field_spec("token|k:~string@!").unwrap();
        assert!(def.is_mcp_hidden && def.required);
        assert_eq!(def.type_name, "string");
        assert!(!field::parse_field_spec("name|n").unwrap().is_mcp_hidden);
        // The parser finds the flag by each of its names
        let args = agent_args();
        for name in ["print-token", "t", "show-token"] {
            assert_eq!(
                field::field_lookup(name, &args),
                Some(0),
                "lookup of {}",
                name
            );
        }
        assert_eq!(
            field::field_names("print-token|t|show-token:+@"),
            vec!["print-token", "t", "show-token"]
        );
    }

    #[test]
    fn test_mcp_hidden_flag_absent_from_tools_list() {
        let flags = extract_flags_for_llm(&agent_args());
        // The command tree and the docs keep the flag, marked
        assert!(flags
            .iter()
            .any(|f| f.name == "print-token" && f.mcp_hidden));
        let leaf_tools = vec![mcp::LeafTool {
            tool_name: "app_login".to_string(),
            full_path: vec!["login".to_string()],
            desc: "Log in".to_string(),
            refused: mcp::refused_flags(&flags),
            flags,
            annotations: vec![],
        }];
        let mut buf = Vec::new();
        mcp::handle_tools_list_v2(&mut buf, &Some("1".to_string()), "App", &leaf_tools);
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("\"name\":{\"type\":\"string\""),
            "normal flag missing: {}",
            output
        );
        assert!(
            !output.contains("print-token"),
            "hidden flag listed: {}",
            output
        );
        assert!(
            !output.contains("show-token"),
            "hidden flag alias listed: {}",
            output
        );
    }

    #[test]
    fn test_mcp_hidden_flag_absent_from_docgen_llm_schema() {
        let mut buf = Vec::new();
        write_tool_properties(&mut buf, &extract_flags_for_llm(&agent_args()), "");
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("\"name\": {"),
            "normal flag missing: {}",
            output
        );
        assert!(
            !output.contains("print-token"),
            "hidden flag listed: {}",
            output
        );
    }

    #[test]
    fn test_mcp_tools_call_refuses_a_hidden_flag_by_long_name() {
        let (output, ran) = agent_tools_call("long", r#"{"print-token":true}"#);
        assert_refused(
            "long name",
            &output,
            &ran,
            -32602,
            "\"message\":\"Flag not available over MCP: print-token\"",
        );
    }

    #[test]
    fn test_mcp_tools_call_refuses_a_hidden_flag_by_short_name() {
        for (label, arguments) in [
            ("short", r#"{"t":true}"#),
            ("short-mixed", r#"{"name":"x","t":true}"#),
        ] {
            let (output, ran) = agent_tools_call(label, arguments);
            assert_refused(
                arguments,
                &output,
                &ran,
                -32602,
                "\"message\":\"Flag not available over MCP: t\"",
            );
        }
    }

    #[test]
    fn test_mcp_tools_call_refuses_a_hidden_flag_by_alias_or_dashes() {
        // The refusal names the key as the call sent it.
        for (label, arguments, key) in [
            ("alias", r#"{"show-token":true}"#, "show-token"),
            ("long-dashes", r#"{"--print-token":true}"#, "--print-token"),
            ("short-dash", r#"{"-t":true}"#, "-t"),
            ("short-inline", r#"{"-tx":true}"#, "-tx"),
            ("value", r#"{"print-token=1":true}"#, "print-token=1"),
            ("variable", r#"{"print_token":true}"#, "print_token"),
            ("dashed-alias-value", r#"{"--show-token=yes":"1"}"#, "--show-token=yes"),
        ] {
            let (output, ran) = agent_tools_call(label, arguments);
            assert_refused(
                arguments,
                &output,
                &ran,
                -32602,
                &format!("\"message\":\"Flag not available over MCP: {}\"", key),
            );
        }
    }

    #[test]
    fn test_mcp_tools_call_runs_without_the_hidden_flag() {
        let (output, ran) = agent_tools_call("normal", r#"{"name":"x","verbose":true}"#);
        assert!(output.contains("\"text\":\"ran\""), "{}", output);
        assert!(output.contains("\"isError\":false"), "{}", output);
        // A value is one element `--name=value`.
        assert_eq!(ran.as_deref(), Some("--name=x\n--verbose\n"));
    }

    #[test]
    fn test_mcp_tools_call_refuses_a_string_for_a_boolean_flag() {
        // A string for a boolean would be one more argv element: the parser
        // reads it as the next flag, so `--print-token` would run.
        for (label, arguments) in [
            ("bool-string", r#"{"verbose":"--print-token"}"#),
            ("bool-short-string", r#"{"verbose":"-t"}"#),
            ("bool-number", r#"{"verbose":1}"#),
            ("bool-null", r#"{"verbose":null}"#),
            ("bool-object", r#"{"verbose":{"a":true}}"#),
            ("bool-array", r#"{"verbose":[true]}"#),
        ] {
            let (output, ran) = agent_tools_call(label, arguments);
            assert_refused(
                arguments,
                &output,
                &ran,
                -32602,
                "\"message\":\"Flag --verbose takes a boolean\"",
            );
        }
    }

    #[test]
    fn test_mcp_tools_call_refuses_a_boolean_for_a_value_flag() {
        for (label, arguments) in [
            ("value-true", r#"{"name":true}"#),
            ("value-false", r#"{"name":false}"#),
            ("value-null", r#"{"name":null}"#),
            ("value-object", r#"{"name":{"a":"b"}}"#),
            ("value-array", r#"{"name":["--print-token"]}"#),
        ] {
            let (output, ran) = agent_tools_call(label, arguments);
            assert_refused(
                arguments,
                &output,
                &ran,
                -32602,
                "\"message\":\"Flag --name takes a string or a number\"",
            );
        }
    }

    #[test]
    fn test_mcp_tools_call_refuses_a_duplicate_key() {
        let (output, ran) = agent_tools_call("dup-args", r#"{"name":"x","name":"--print-token"}"#);
        assert_refused("arguments", &output, &ran, -32602, "duplicate key: name");
        let (output, ran) = agent_tools_call_params(
            "dup-params",
            r#"{"name":"app_login","arguments":{},"arguments":{"t":true}}"#,
        );
        assert_refused("params", &output, &ran, -32602, "duplicate key: arguments");
        let (output, ran) = agent_tools_call("dup-nested", r#"{"name":{"a":1,"a":2}}"#);
        assert_refused("nested", &output, &ran, -32602, "duplicate key: a");
    }

    #[test]
    fn test_mcp_tools_call_reads_all_json_whitespace() {
        // A newline or a CR before the colon: the call is refused, it does
        // not run without the check.
        for (label, arguments, key) in [
            ("lf-colon", "{\"print-token\"\n:true}", "print-token"),
            ("cr-colon", "{\"print-token\"\r:true}", "print-token"),
            ("ws-comma", "{\"name\":\"x\"\r\n,\t\"t\"\n:\ttrue}", "t"),
        ] {
            let (output, ran) = agent_tools_call(label, arguments);
            assert_refused(
                label,
                &output,
                &ran,
                -32602,
                &format!("\"message\":\"Flag not available over MCP: {}\"", key),
            );
        }
        let (output, ran) =
            agent_tools_call("ws-ok", "{\n\"name\"\r:\t\"x\"\n,\"verbose\" : true\r\n}");
        assert!(output.contains("\"text\":\"ran\""), "{}", output);
        assert_eq!(ran.as_deref(), Some("--name=x\n--verbose\n"));
    }

    #[test]
    fn test_mcp_tools_call_refuses_a_request_it_cannot_read() {
        for (label, params, code, text) in [
            ("unterminated", r#"{"name":"app_login","arguments":{"name":"x"}"#, -32700, "Parse error"),
            ("trailing", r#"{"name":"app_login","arguments":{}} {"#, -32700, "Parse error"),
            ("bad-escape", r#"{"name":"app_login","arguments":{"name":"\q"}}"#, -32700, "Parse error"),
            ("lone-surrogate", r#"{"name":"app_login","arguments":{"name":"\ud800"}}"#, -32700, "Parse error"),
            ("not-object", r#"["app_login"]"#, -32602, "Params must be an object"),
            ("args-array", r#"{"name":"app_login","arguments":[{"t":true}]}"#, -32602, "Arguments must be an object"),
            ("name-number", r#"{"name":7,"arguments":{}}"#, -32602, "Missing tool name"),
        ] {
            let (output, ran) = agent_tools_call_params(label, params);
            assert_refused(label, &output, &ran, code, text);
        }
    }

    #[test]
    fn test_mcp_tool_argv_alone() {
        let flags = extract_flags_for_llm(&agent_args());
        let refused = mcp::refused_flags(&flags);
        let path = vec!["login".to_string()];
        let s = |v: &str| mcp::Json::Str(v.to_string());
        let n = |v: &str| mcp::Json::Number(v.to_string());
        let arg = |k: &str, v: mcp::Json| (k.to_string(), v);

        // A value that looks like a flag stays the value of its flag.
        let argv = mcp::tool_argv(
            &path,
            &[
                arg("verbose", mcp::Json::Bool(true)),
                arg("name", s("--print-token")),
            ],
            &flags,
            &refused,
        );
        assert_eq!(argv, Ok(vec!["login".into(), "--verbose".into(), "--name=--print-token".into()]));
        // A number, `false`, and a key that no flag has
        let argv = mcp::tool_argv(
            &path,
            &[
                arg("name", n("-5")),
                arg("verbose", mcp::Json::Bool(false)),
                arg("other", s("x")),
            ],
            &flags,
            &refused,
        );
        assert_eq!(argv, Ok(vec!["login".into(), "--name=-5".into()]));
        // The second guard: a `:@` flag is an error here too, by any name.
        for key in ["print-token", "t", "show-token", "--print-token", "print_token"] {
            let argv = mcp::tool_argv(
                &path,
                &[arg("name", s("x")), arg(key, mcp::Json::Bool(true))],
                &flags,
                &refused,
            );
            assert_eq!(argv, Err(format!("Flag not available over MCP: {}", key)), "{}", key);
        }
        // A NUL cannot go into an argv element.
        let argv = mcp::tool_argv(&path, &[arg("name", s("a\0b"))], &flags, &refused);
        assert_eq!(argv, Err("Flag --name: the value has a NUL character".to_string()));
    }

    #[test]
    fn test_mcp_parse_json_strict() {
        use mcp::Json;
        assert_eq!(
            mcp::parse_json_strict(r#" {"a":1.5,"b":-3,"c":10,"d":1.0,"e":"\ud83d\ude00\n"} "#),
            Ok(Json::Object(vec![
                ("a".into(), Json::Number("1.5".into())),
                ("b".into(), Json::Number("-3".into())),
                ("c".into(), Json::Number("10".into())),
                ("d".into(), Json::Number("1".into())),
                ("e".into(), Json::Str("\u{1f600}\n".into())),
            ]))
        );
        assert_eq!(mcp::parse_json_strict("[]"), Ok(Json::Array(vec![])));
        assert_eq!(mcp::parse_json_strict("01").map_err(|e| e.0), Err(-32700));
        assert_eq!(mcp::parse_json_strict(r#"{"a":1,}"#).map_err(|e| e.0), Err(-32700));
        assert_eq!(mcp::parse_json_strict(r#"{"a":1,"a":1}"#).map_err(|e| e.0), Err(-32602));
    }

    #[test]
    fn test_mcp_json_escape_control_characters() {
        assert_eq!(json_escape("a\u{1}b\u{1f}c"), "a\\u0001b\\u001fc");
        assert_eq!(json_escape("q\"\\\n\r\t"), "q\\\"\\\\\\n\\r\\t");
    }

    // -- `:@` through inheritance and `#` --

    /// Functions for `build_tree` without a shell: name and body.
    struct FakeFunctions(Vec<(String, String)>);

    impl Functions for FakeFunctions {
        fn exists(&self, name: &str) -> bool {
            self.0.iter().any(|(n, _)| n == name)
        }

        fn body(&self, name: &str) -> Option<String> {
            self.0.iter().find(|(n, _)| n == name).map(|(_, b)| b.clone())
        }
    }

    /// A function as `declare -f` prints it, with these statements.
    fn declared(name: &str, statements: &[&str]) -> (String, String) {
        let body: Vec<String> = statements.iter().map(|s| format!("    {};", s)).collect();
        (name.to_string(), format!("{} () \n{{ \n{}\n}}", name, body.join("\n")))
    }

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// The tree of a script whose root has `root_args` and the command `login`.
    fn login_tree(root_args: &[&str], login: &[&str]) -> Vec<CommandNode> {
        let funcs = FakeFunctions(vec![declared("login", login)]);
        build_tree(&funcs, &words(&["login", "Log in"]), &agent_flags(&words(root_args)), Some("main"))
    }

    /// Send a tools/call to a node of the tree, as the probe tool.
    fn node_tools_call(label: &str, node: &CommandNode, arguments: &str) -> (String, Option<String>) {
        let dir =
            std::env::temp_dir().join(format!("argsh-mcp-tree-{}-{}", std::process::id(), label));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut leaf = agent_leaf(&dir, node.flags.clone());
        leaf.refused = node.refused.clone();
        let params = format!(r#"{{"name":"app_login","arguments":{}}}"#, arguments);
        let mut buf = Vec::new();
        mcp::handle_tools_call_v2(&mut buf, &Some("8".to_string()), &params, "/bin/sh", &[leaf]);
        let ran = std::fs::read_to_string(dir.join("argv")).ok();
        std::fs::remove_dir_all(&dir).unwrap();
        (String::from_utf8(buf).unwrap(), ran)
    }

    fn refused_names(node: &CommandNode) -> Vec<&str> {
        node.refused.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn test_mcp_hidden_flag_under_a_yielding_child_entry() {
        // The child's `:^` entry yields to the parent's `:@` entry.
        let tree = login_tree(
            &["print-token|t:+@", "Print the access token"],
            &[r#"local -a args=('print-token|t:+^' 'Inherited setting' "${args[@]:-}")"#],
        );
        let login = &tree[0];
        assert_eq!(refused_names(login), vec!["print-token"]);
        assert!(login.flags.iter().all(|f| f.name != "print-token" || f.mcp_hidden));
        let tool = mcp::format_tool("app_login", "Log in", &login.flags, &[]);
        assert!(!tool.contains("print-token"), "{}", tool);
        let (output, ran) = node_tools_call("yield", login, r#"{"print-token":true}"#);
        assert_refused("yield", &output, &ran, -32602, "Flag not available over MCP: print-token");
    }

    #[test]
    fn test_mcp_hidden_flag_stays_when_a_visible_child_entry_wins() {
        // The child's entry wins at its own parser, but `:usage` of the parent
        // parses `--print-token` first. The flag stays refused.
        for (label, login, keys) in [
            (
                "after-expansion",
                r#"local -a args=("${args[@]:-}" 'print-token|t:+' 'Print it')"#,
                vec!["print-token", "t"],
            ),
            (
                "no-expansion",
                r#"local -a args=('print-token|p:+' 'Print it')"#,
                vec!["print-token", "t", "p"],
            ),
        ] {
            let tree = login_tree(&["print-token|t:+@", "Print the access token"], &[login]);
            let login = &tree[0];
            assert_eq!(refused_names(login), vec!["print-token"], "{}", label);
            for key in keys {
                let (output, ran) =
                    node_tools_call(&format!("{}-{}", label, key), login, &format!(r#"{{"{}":true}}"#, key));
                assert_refused(
                    label,
                    &output,
                    &ran,
                    -32602,
                    &format!("Flag not available over MCP: {}", key),
                );
            }
        }
    }

    #[test]
    fn test_mcp_hidden_flag_of_a_grandparent() {
        // root (the `:@` flag) -> cluster -> up: three levels.
        let funcs = FakeFunctions(vec![
            declared(
                "cluster",
                &[
                    r#"local -a args=('verbose|v:+' 'Verbose output' "${args[@]:-}")"#,
                    "local -a usage=('up' 'Start the cluster' 'down' 'Stop the cluster')",
                ],
            ),
            declared(
                "cluster::up",
                &[r#"local -a args=('nodes|n:~int' 'Nodes' "${args[@]:-}")"#],
            ),
            declared(
                "cluster::down",
                &[r#"local -a args=('print-token|k:+^' 'Inherited setting' "${args[@]:-}")"#],
            ),
        ]);
        let root = agent_flags(&words(&["print-token|t:+@", "Print the access token"]));
        let tree = build_tree(&funcs, &words(&["cluster", "Cluster"]), &root, Some("main"));
        let leaves = flatten_leaves(&tree);
        assert_eq!(leaves.len(), 2);
        for leaf in &leaves {
            assert_eq!(refused_names(leaf), vec!["print-token"], "{:?}", leaf.full_path);
        }
        let up = leaves[0];
        let names: Vec<&str> = up.flags.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["nodes", "verbose", "print-token"]);
        let down = leaves[1];
        assert!(down.refused[0].names.contains(&"k".to_string()), "{:?}", down.refused[0].names);
        for (label, node, key) in [("up-t", up, "t"), ("down-k", down, "k")] {
            let (output, ran) = node_tools_call(label, node, &format!(r#"{{"{}":true}}"#, key));
            assert_refused(label, &output, &ran, -32602, "Flag not available over MCP");
        }
        let (output, ran) = node_tools_call("up-ok", up, r#"{"nodes":3,"verbose":true}"#);
        assert!(output.contains("\"text\":\"ran\""), "{}", output);
        assert_eq!(ran.as_deref(), Some("--nodes=3\n--verbose\n"));
    }

    #[test]
    fn test_mcp_hash_prefixed_hidden_flag_is_refused() {
        // A `#` flag is not in the schema and not in --help. With `:@` it is
        // refused, also through a child command.
        let root = ["#print-token|t:+@", "Print the access token", "name|n", "Name"];
        let all = agent_flags(&words(&root));
        assert!(all.iter().any(|f| f.name == "print-token" && f.help_hidden && f.mcp_hidden));
        let flags = extract_flags_for_llm(&words(&root));
        assert!(flags.iter().all(|f| f.name != "print-token"));
        let refused = mcp::refused_flags(&all);
        assert_eq!(refused.len(), 1);
        let dir = std::env::temp_dir().join(format!("argsh-mcp-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut leaf = agent_leaf(&dir, flags);
        leaf.refused = refused;
        for key in ["print-token", "t"] {
            let mut buf = Vec::new();
            let params = format!(r#"{{"name":"app_login","arguments":{{"{}":true}}}}"#, key);
            mcp::handle_tools_call_v2(
                &mut buf,
                &Some("9".to_string()),
                &params,
                "/bin/sh",
                std::slice::from_ref(&leaf),
            );
            let output = String::from_utf8(buf).unwrap();
            let ran = std::fs::read_to_string(dir.join("argv")).ok();
            assert_refused(key, &output, &ran, -32602, &format!("Flag not available over MCP: {}", key));
        }
        std::fs::remove_dir_all(&dir).unwrap();
        let tree = login_tree(&root, &[r#"local -a args=('verbose|v:+' 'Verbose' "${args[@]:-}")"#]);
        assert_eq!(refused_names(&tree[0]), vec!["print-token"]);
        assert!(tree[0].flags.iter().all(|f| f.name != "print-token"));
    }

    #[test]
    fn test_child_flags_follow_the_place_of_the_parent_expansion() {
        let parent = agent_flags(&words(&["domain|d", "Parent domain", "verbose|v:+", "Verbose"]));
        // The expansion first: the child's non-`:^` entry comes later and wins,
        // and the child's own flags stay (they are not read as pairs of the
        // wrong words).
        let flags = child_flags(
            &words(&["${args[@]:-}", "domain|d", "Child domain", "port|p:~int", "Port"]),
            false,
            &parent,
        );
        let got: Vec<(&str, &str)> = flags.iter().map(|f| (f.name.as_str(), f.desc.as_str())).collect();
        assert_eq!(got, vec![("verbose", "Verbose"), ("domain", "Child domain"), ("port", "Port")]);
        // The expansion last: the parent's entry comes later and wins.
        let flags = child_flags(&words(&["domain|d", "Child domain", "${args[@]:-}"]), false, &parent);
        assert_eq!(flags[0].desc, "Parent domain");
        // A `:^` child entry yields to the parent's entry, wherever it is.
        let flags = child_flags(&words(&["${args[@]}", "domain|d:^", "Child domain"]), false, &parent);
        assert_eq!(flags.iter().find(|f| f.name == "domain").unwrap().desc, "Parent domain");
        // `args+=(...)`: the parent's entries come first.
        let flags = child_flags(&words(&["domain|d", "Child domain"]), true, &parent);
        assert_eq!(flags.iter().find(|f| f.name == "domain").unwrap().desc, "Child domain");
    }

    #[test]
    fn test_parse_shell_array_and_form() {
        let local = "f () \n{ \n    local -a args=('a|a' 'A' \"${args[@]:-}\");\n}";
        assert_eq!(
            parse_shell_array_and_form(local, "args"),
            (words(&["a|a", "A", "${args[@]:-}"]), false)
        );
        let append = "f () \n{ \n    args+=('a|a' 'A');\n}";
        assert_eq!(parse_shell_array_and_form(append, "args"), (words(&["a|a", "A"]), true));
    }
}
