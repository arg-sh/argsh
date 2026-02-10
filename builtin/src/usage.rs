//! :usage builtin -- subcommand dispatch with prefix resolution.
//!
//! Mirrors: libraries/args.sh (:usage function)

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
    .unwrap_or(1);

    // Match bash's `exit` behavior: help and errors terminate the script.
    // Only success (0) returns to the caller so the script can continue.
    match code {
        0 => 0,
        shared::HELP_EXIT => std::process::exit(0),
        n => std::process::exit(n),
    }
}

// -- Implementation ---------------------------------------------------------

/// Main entry point for :usage builtin.
/// Returns exit code (0 = success, 2 = usage error).
pub fn usage_main(args: &[String]) -> i32 {
    if args.is_empty() {
        return shared::error_usage("", ":usage requires a title argument");
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

    // Handle empty args, -h, --help
    if cli_args.is_empty()
        || cli_args[0] == "-h"
        || cli_args[0] == "--help"
    {
        usage_help_text(title, &usage_arr, &args_arr);
        return shared::HELP_EXIT;
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
            Err(_) => {
                break;
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
            let display = commandname.last().cloned().unwrap_or_default();
            return shared::error_usage(&display, "Missing command");
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
        return shared::error_usage(&cmd, &format!("Invalid command: {}", cmd));
    }

    // Resolve function with prefix fallback
    let explicit = found_field.contains(":-");

    if explicit {
        if !shell::function_exists(&func) {
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

    // Append to COMMANDNAME
    let cmd_name = found_field.split(['|', ':']).next().unwrap_or(&found_field);
    shell::append_commandname(cmd_name);

    // Set usage = (func remaining_args...)
    let mut new_usage = vec![func];
    new_usage.extend(cli);
    shell::write_array("usage", &new_usage);

    0 // EXECUTION_SUCCESS
}

fn set_or_increment(name: &str) {
    if shell::is_array(name) {
        shell::array_append(name, "1");
    } else {
        shell::set_scalar(name, "1");
    }
}

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

    if flag_indices.is_empty() {
        return;
    }

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
        let def = field::parse_field(entry);
        let field_fmt = field::format_field(&def);
        let _ = writeln!(out, "{}", field_fmt);

        // Description (indented 11 spaces)
        let _ = writeln!(out, "           {}", desc);
    }
}
