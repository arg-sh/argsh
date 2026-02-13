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

// -- :usage::completion builtin registration ----------------------------------

static USAGE_COMPLETION_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Generate shell completion scripts (bash, zsh, fish).".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage::completion_struct"]
pub static mut USAGE_COMPLETION_STRUCT: BashBuiltin = BashBuiltin {
    name: c":usage::completion".as_ptr(),
    function: usage_completion_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":usage::completion <shell> [-- title usage_pairs...]".as_ptr(),
    long_doc: USAGE_COMPLETION_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage::completion_builtin_load"]
pub extern "C" fn usage_completion_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage::completion_builtin_unload"]
pub extern "C" fn usage_completion_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback

extern "C" fn usage_completion_builtin_fn(word_list: *const WordList) -> c_int {
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        usage_completion_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    std::process::exit(if code == shared::HELP_EXIT || code == 0 { 0 } else { code }) // coverage:off
}

// -- :usage::docgen builtin registration --------------------------------------

static USAGE_DOCGEN_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Generate documentation (man, md, rst, yaml, llm).".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage::docgen_struct"]
pub static mut USAGE_DOCGEN_STRUCT: BashBuiltin = BashBuiltin {
    name: c":usage::docgen".as_ptr(),
    function: usage_docgen_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":usage::docgen <format> [-- title usage_pairs...]".as_ptr(),
    long_doc: USAGE_DOCGEN_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage::docgen_builtin_load"]
pub extern "C" fn usage_docgen_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage::docgen_builtin_unload"]
pub extern "C" fn usage_docgen_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback

extern "C" fn usage_docgen_builtin_fn(word_list: *const WordList) -> c_int {
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        usage_docgen_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    std::process::exit(if code == shared::HELP_EXIT || code == 0 { 0 } else { code }) // coverage:off
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

// -- :usage::completion implementation ----------------------------------------

/// Main entry point for :usage::completion builtin.
/// Called via "${usage[@]}" — generates shell completion scripts.
/// Args: [shell_type] [-- title original_usage_pairs...]
pub fn usage_completion_main(args: &[String]) -> i32 {
    let sep = args.iter().position(|s| s == "--");
    let (user_args, meta) = match sep {
        Some(pos) => (&args[..pos], &args[pos + 1..]),
        None => (args, [].as_slice()),
    };

    if user_args.is_empty() || user_args[0] == "-h" || user_args[0] == "--help" {
        let commandname = shell::get_commandname();
        let cmd_str = if commandname.len() > 1 {
            commandname[..commandname.len() - 1].join(" ")
        } else {
            shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
        };
        println!("Generate shell completion scripts.\n");
        println!("Usage: {} completion <shell>\n", cmd_str);
        println!("Available shells:");
        println!("  bash    Bash completion script");
        println!("  zsh     Zsh completion script");
        println!("  fish    Fish completion script");
        return shared::HELP_EXIT;
    }

    let shell_type = &user_args[0];
    let title = meta.first().map(|s| s.as_str()).unwrap_or("");
    let usage_pairs = if meta.len() > 1 { &meta[1..] } else { &[] as &[String] };
    let args_arr = shell::read_array("args");

    // Base command name (COMMANDNAME minus "completion" at the end)
    let commandname = shell::get_commandname();
    let cmd_name = if commandname.len() > 1 {
        commandname[commandname.len() - 2].clone()
    } else {
        shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
    };

    let out = std::io::stdout();
    let mut out = out.lock();

    match shell_type.as_str() {
        "bash" => generate_bash_completion(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "zsh" => generate_zsh_completion(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "fish" => generate_fish_completion(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        _ => {
            return shared::error_usage("", &format!(
                "unknown shell: {}. Use bash, zsh, or fish", shell_type
            ));
        }
    }
    shared::HELP_EXIT
}

// -- :usage::docgen implementation --------------------------------------------

/// Main entry point for :usage::docgen builtin.
/// Called via "${usage[@]}" — generates documentation in various formats.
/// Args: [format] [-- title original_usage_pairs...]
pub fn usage_docgen_main(args: &[String]) -> i32 {
    let sep = args.iter().position(|s| s == "--");
    let (user_args, meta) = match sep {
        Some(pos) => (&args[..pos], &args[pos + 1..]),
        None => (args, [].as_slice()),
    };

    if user_args.is_empty() || user_args[0] == "-h" || user_args[0] == "--help" {
        let commandname = shell::get_commandname();
        let cmd_str = if commandname.len() > 1 {
            commandname[..commandname.len() - 1].join(" ")
        } else {
            shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
        };
        println!("Generate documentation in various formats.\n");
        println!("Usage: {} docgen <format>\n", cmd_str);
        println!("Available formats:");
        println!("  man     Man page (troff format)");
        println!("  md      Markdown");
        println!("  rst     reStructuredText");
        println!("  yaml    YAML");
        println!("  llm     LLM tool schema (claude, openai, gemini, kimi)");
        return shared::HELP_EXIT;
    }

    let format = &user_args[0];
    let title = meta.first().map(|s| s.as_str()).unwrap_or("");
    let usage_pairs = if meta.len() > 1 { &meta[1..] } else { &[] as &[String] };
    let args_arr = shell::read_array("args");

    let commandname = shell::get_commandname();
    let cmd_name = if commandname.len() > 1 {
        commandname[commandname.len() - 2].clone()
    } else {
        shell::get_script_name() // coverage:off - defensive_check: always called via :usage dispatch which sets COMMANDNAME
    };

    let out = std::io::stdout();
    let mut out = out.lock();

    match format.as_str() {
        "man" => generate_man_page(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "md" => generate_markdown(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "rst" => generate_rst(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "yaml" => generate_yaml(&mut out, &cmd_name, title, usage_pairs, &args_arr),
        "llm" => {
            let provider = user_args.get(1).map(|s| s.as_str());
            match provider {
                Some("claude") | Some("anthropic") => {
                    generate_llm_claude(&mut out, &cmd_name, title, usage_pairs, &args_arr);
                }
                Some("openai") | Some("gemini") | Some("kimi") => {
                    generate_llm_openai(&mut out, &cmd_name, title, usage_pairs, &args_arr);
                }
                Some(unknown) => {
                    return shared::error_usage("", &format!(
                        "unknown LLM provider: {}. Use claude, openai, gemini, or kimi", unknown
                    ));
                }
                None => {
                    return shared::error_usage("", "llm format requires a provider: claude, openai, gemini, or kimi");
                }
            }
        }
        _ => {
            return shared::error_usage("", &format!(
                "unknown format: {}. Use man, md, rst, yaml, or llm", format
            ));
        }
    }
    shared::HELP_EXIT
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
            "completion" | "docgen" => {
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

/// Check if a function name is a deferred :usage:: builtin (completion, docgen).
fn is_deferred_builtin(name: &str) -> bool {
    matches!(name, ":usage::completion" | ":usage::docgen")
}

/// Defer a built-in special command (completion, docgen) via the usage array.
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

// -- Completion generation ----------------------------------------------------

/// Extracted subcommand info from usage pairs.
struct SubCmd {
    name: String,
    desc: String,
}

/// Extracted flag info from args array.
struct FlagInfo {
    name: String,
    short: Option<String>,
    desc: String,
    is_boolean: bool,
    type_name: String,
    required: bool,
}

/// Extract visible subcommands from usage pairs.
fn extract_subcommands(usage_pairs: &[String]) -> Vec<SubCmd> {
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
fn extract_flags(args_arr: &[String]) -> Vec<FlagInfo> {
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
fn extract_flags_for_llm(args_arr: &[String]) -> Vec<FlagInfo> {
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

/// Generate bash completion script.
fn generate_bash_completion<W: Write>(
    out: &mut W,
    cmd_name: &str,
    _title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let func_name = format!("_{}", cmd_name.replace('-', "_"));

    let _ = writeln!(out, "# bash completion for {}", cmd_name);
    let _ = writeln!(out, "{}() {{", func_name);
    let _ = writeln!(out, "    local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"");
    let _ = writeln!(out);

    // Flags
    let flag_words: Vec<String> = flags.iter().flat_map(|f| {
        let mut words = vec![format!("--{}", f.name)];
        if let Some(ref s) = f.short {
            words.push(format!("-{}", s));
        }
        words
    }).collect();

    let _ = writeln!(out, "    if [[ \"${{cur}}\" == -* ]]; then");
    let _ = writeln!(out, "        COMPREPLY=($(compgen -W \"{}\" -- \"${{cur}}\"))", flag_words.join(" "));
    let _ = writeln!(out, "    else");

    // Subcommands
    let cmd_words: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
    let _ = writeln!(out, "        COMPREPLY=($(compgen -W \"{}\" -- \"${{cur}}\"))", cmd_words.join(" "));
    let _ = writeln!(out, "    fi");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "complete -o default -F {} {}", func_name, cmd_name);
}

/// Generate zsh completion script.
fn generate_zsh_completion<W: Write>(
    out: &mut W,
    cmd_name: &str,
    _title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let func_name = format!("_{}", cmd_name.replace('-', "_"));

    let _ = writeln!(out, "#compdef {}", cmd_name);
    let _ = writeln!(out);
    let _ = writeln!(out, "{}() {{", func_name);

    if !cmds.is_empty() {
        let _ = writeln!(out, "    local -a commands=(");
        for cmd in &cmds {
            let esc_desc = cmd.desc.replace('\'', "'\\''");
            let _ = writeln!(out, "        '{}:{}'", cmd.name, esc_desc);
        }
        let _ = writeln!(out, "    )");
        let _ = writeln!(out);
    }

    let _ = write!(out, "    _arguments -s");

    for flag in &flags {
        let long = &flag.name;
        let esc_desc = flag.desc.replace('\'', "'\\''").replace('[', "\\[").replace(']', "\\]");
        if let Some(ref short) = flag.short {
            if flag.is_boolean {
                let _ = write!(out, " \\\n        '(-{} --{})'{{\"-{}\",\"--{}\"}}'[{}]'",
                    short, long, short, long, esc_desc);
            } else {
                let _ = write!(out, " \\\n        '(-{} --{})'{{\"-{}\",\"--{}\"}}'[{}]:{}:'",
                    short, long, short, long, esc_desc, flag.type_name);
            }
        } else if flag.is_boolean {
            let _ = write!(out, " \\\n        '--{}[{}]'", long, esc_desc);
        } else {
            let _ = write!(out, " \\\n        '--{}[{}]:{}:'", long, esc_desc, flag.type_name);
        }
    }

    if !cmds.is_empty() {
        let _ = writeln!(out, " \\\n        '*::command:->commands'");
        let _ = writeln!(out);
        let _ = writeln!(out, "    case \"$state\" in");
        let _ = writeln!(out, "        commands)");
        let _ = writeln!(out, "            _describe 'command' commands");
        let _ = writeln!(out, "            ;;");
        let _ = writeln!(out, "    esac");
    } else {
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "}}");
    let _ = writeln!(out);
    let _ = writeln!(out, "{} \"$@\"", func_name);
}

/// Generate fish completion script.
fn generate_fish_completion<W: Write>(
    out: &mut W,
    cmd_name: &str,
    _title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);

    let _ = writeln!(out, "# fish completion for {}", cmd_name);

    // Subcommands
    for cmd in &cmds {
        let esc_desc = cmd.desc.replace('\'', "\\'");
        let _ = writeln!(out, "complete -c {} -n '__fish_use_subcommand' -a '{}' -d '{}'",
            cmd_name, cmd.name, esc_desc);
    }

    // Flags
    for flag in &flags {
        let esc_desc = flag.desc.replace('\'', "\\'");
        let mut parts = format!("complete -c {} -l '{}'", cmd_name, flag.name);
        if let Some(ref short) = flag.short {
            parts.push_str(&format!(" -s '{}'", short));
        }
        if !flag.is_boolean {
            parts.push_str(" -r");
        }
        parts.push_str(&format!(" -d '{}'", esc_desc));
        let _ = writeln!(out, "{}", parts);
    }
}

// -- Man page generation ------------------------------------------------------

/// Generate man page in troff format.
fn generate_man_page<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let upper_name = cmd_name.to_uppercase();
    let first_line = title.lines().next().unwrap_or(title).trim();

    // Header
    let _ = writeln!(out, ".TH \"{}\" 1", upper_name);

    // NAME
    let _ = writeln!(out, ".SH NAME");
    let _ = writeln!(out, "{} \\- {}", cmd_name, man_escape(first_line));

    // SYNOPSIS
    let _ = writeln!(out, ".SH SYNOPSIS");
    let _ = writeln!(out, ".B {}", cmd_name);
    if !cmds.is_empty() {
        let _ = writeln!(out, ".RI [ command ]");
    }
    let _ = writeln!(out, ".RI [ options ]");

    // DESCRIPTION
    let _ = writeln!(out, ".SH DESCRIPTION");
    for line in title.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            let _ = writeln!(out, ".PP");
        } else {
            let _ = writeln!(out, "{}", man_escape(trimmed));
        }
    }

    // COMMANDS
    if !cmds.is_empty() {
        let _ = writeln!(out, ".SH COMMANDS");
        for cmd in &cmds {
            let _ = writeln!(out, ".TP");
            let _ = writeln!(out, ".B {}", cmd.name);
            let _ = writeln!(out, "{}", man_escape(&cmd.desc));
        }
    }

    // OPTIONS
    if !flags.is_empty() {
        let _ = writeln!(out, ".SH OPTIONS");
        for flag in &flags {
            let _ = writeln!(out, ".TP");
            if let Some(ref short) = flag.short {
                if flag.is_boolean {
                    let _ = writeln!(out, ".BR \\-{} \", \" \\-\\-{}", short, flag.name);
                } else {
                    let _ = writeln!(out, ".BR \\-{} \", \" \\-\\-{} \" \" \\fI{}\\fR",
                        short, flag.name, flag.type_name);
                }
            } else if flag.is_boolean {
                let _ = writeln!(out, ".BR \\-\\-{}", flag.name);
            } else {
                let _ = writeln!(out, ".BR \\-\\-{} \" \" \\fI{}\\fR", flag.name, flag.type_name);
            }
            let _ = writeln!(out, "{}", man_escape(&flag.desc));
        }
    }
}

/// Escape special troff characters.
fn man_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('-', "\\-")
}

// -- Markdown generation ------------------------------------------------------

/// Generate documentation as Markdown.
fn generate_markdown<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "# {}\n", cmd_name);
    let _ = writeln!(out, "{}\n", first_line);

    // Synopsis
    let _ = writeln!(out, "## Synopsis\n");
    let _ = write!(out, "```\n{}", cmd_name);
    if !cmds.is_empty() {
        let _ = write!(out, " [command]");
    }
    let _ = writeln!(out, " [options]\n```\n");

    // Description (skip first line since it's already shown as summary above)
    let remaining: Vec<&str> = title.lines().skip(1).collect();
    if !remaining.is_empty() {
        let _ = writeln!(out, "## Description\n");
        for line in &remaining {
            let _ = writeln!(out, "{}", line.trim());
        }
        let _ = writeln!(out);
    }

    // Commands
    if !cmds.is_empty() {
        let _ = writeln!(out, "## Commands\n");
        let _ = writeln!(out, "| Command | Description |");
        let _ = writeln!(out, "|---------|-------------|");
        for cmd in &cmds {
            let _ = writeln!(out, "| `{}` | {} |", cmd.name, cmd.desc);
        }
        let _ = writeln!(out);
    }

    // Options
    if !flags.is_empty() {
        let _ = writeln!(out, "## Options\n");
        let _ = writeln!(out, "| Flag | Description |");
        let _ = writeln!(out, "|------|-------------|");
        for flag in &flags {
            let mut flag_str = format!("`--{}`", flag.name);
            if let Some(ref short) = flag.short {
                flag_str = format!("`-{}`, {}", short, flag_str);
            }
            if !flag.is_boolean {
                flag_str.push_str(&format!(" *{}*", flag.type_name));
            }
            let _ = writeln!(out, "| {} | {} |", flag_str, flag.desc);
        }
        let _ = writeln!(out);
    }
}

// -- reStructuredText generation ----------------------------------------------

/// Generate documentation as reStructuredText.
fn generate_rst<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    // Title
    let underline: String = "=".repeat(cmd_name.len());
    let _ = writeln!(out, "{}", cmd_name);
    let _ = writeln!(out, "{}\n", underline);
    let _ = writeln!(out, "{}\n", first_line);

    // Synopsis
    let _ = writeln!(out, "Synopsis");
    let _ = writeln!(out, "--------\n");
    let _ = writeln!(out, ".. code-block:: bash\n");
    let _ = write!(out, "   {}", cmd_name);
    if !cmds.is_empty() {
        let _ = write!(out, " [command]");
    }
    let _ = writeln!(out, " [options]\n");

    // Description (skip first line since it's already shown as summary above)
    let remaining: Vec<&str> = title.lines().skip(1).collect();
    if !remaining.is_empty() {
        let _ = writeln!(out, "Description");
        let _ = writeln!(out, "-----------\n");
        for line in &remaining {
            let _ = writeln!(out, "{}", line.trim());
        }
        let _ = writeln!(out);
    }

    // Commands
    if !cmds.is_empty() {
        let _ = writeln!(out, "Commands");
        let _ = writeln!(out, "--------\n");
        for cmd in &cmds {
            let _ = writeln!(out, "**{}**", cmd.name);
            let _ = writeln!(out, "   {}\n", cmd.desc);
        }
    }

    // Options
    if !flags.is_empty() {
        let _ = writeln!(out, "Options");
        let _ = writeln!(out, "-------\n");
        for flag in &flags {
            let mut flag_str = format!("--{}", flag.name);
            if let Some(ref short) = flag.short {
                flag_str = format!("-{}, {}", short, flag_str);
            }
            if !flag.is_boolean {
                flag_str.push_str(&format!(" *{}*", flag.type_name));
            }
            let _ = writeln!(out, "**{}**", flag_str);
            let _ = writeln!(out, "   {}\n", flag.desc);
        }
    }
}

// -- YAML generation ----------------------------------------------------------

/// Escape a string for YAML double-quoted output.
fn yaml_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Generate documentation as YAML.
fn generate_yaml<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "name: \"{}\"", yaml_escape(cmd_name));
    let _ = writeln!(out, "description: \"{}\"", yaml_escape(first_line));

    let synopsis = if !cmds.is_empty() {
        format!("{} [command] [options]", cmd_name)
    } else {
        format!("{} [options]", cmd_name)
    };
    let _ = writeln!(out, "synopsis: \"{}\"", yaml_escape(&synopsis));

    // Commands
    if !cmds.is_empty() {
        let _ = writeln!(out, "commands:");
        for cmd in &cmds {
            let _ = writeln!(out, "  - name: \"{}\"", yaml_escape(&cmd.name));
            let _ = writeln!(out, "    description: \"{}\"", yaml_escape(&cmd.desc));
        }
    }

    // Options
    if !flags.is_empty() {
        let _ = writeln!(out, "options:");
        for flag in &flags {
            let _ = writeln!(out, "  - name: \"{}\"", yaml_escape(&flag.name));
            if let Some(ref short) = flag.short {
                let _ = writeln!(out, "    short: \"{}\"", yaml_escape(short));
            }
            let _ = writeln!(out, "    description: \"{}\"", yaml_escape(&flag.desc));
            if flag.is_boolean {
                let _ = writeln!(out, "    type: boolean");
            } else {
                let _ = writeln!(out, "    type: \"{}\"", yaml_escape(&flag.type_name));
            }
        }
    }
}

// -- LLM tool schema generation -----------------------------------------------

/// Escape a string for JSON string output.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Map argsh type names to JSON Schema types.
fn argsh_type_to_json(type_name: &str, is_boolean: bool) -> &'static str {
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
fn sanitize_tool_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

/// Write JSON Schema properties and required array (shared by all LLM providers).
fn write_tool_properties<W: Write>(out: &mut W, flags: &[FlagInfo], indent: &str) {
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

/// Generate LLM tool schema in Anthropic Claude format.
fn generate_llm_claude<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags_for_llm(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "[");

    if cmds.is_empty() {
        write_claude_tool(out, &sanitize_tool_name(cmd_name), first_line, &flags, true);
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            let tool_name = sanitize_tool_name(&format!("{}_{}", cmd_name, cmd.name));
            let desc = if cmd.desc.is_empty() { first_line } else { &cmd.desc };
            write_claude_tool(out, &tool_name, desc, &flags, i == cmds.len() - 1);
        }
    }

    let _ = writeln!(out, "]");
}

fn write_claude_tool<W: Write>(out: &mut W, name: &str, description: &str, flags: &[FlagInfo], is_last: bool) {
    let _ = writeln!(out, "  {{");
    let _ = writeln!(out, "    \"name\": \"{}\",", json_escape(name));
    let _ = writeln!(out, "    \"description\": \"{}\",", json_escape(description));
    let _ = writeln!(out, "    \"input_schema\": {{");
    let _ = writeln!(out, "      \"type\": \"object\",");
    write_tool_properties(out, flags, "      ");
    let _ = writeln!(out, "    }}");
    let trailing = if is_last { "" } else { "," };
    let _ = writeln!(out, "  }}{}", trailing);
}

/// Generate LLM tool schema in OpenAI function calling format.
/// Also used for Gemini and Kimi (OpenAI-compatible).
fn generate_llm_openai<W: Write>(
    out: &mut W,
    cmd_name: &str,
    title: &str,
    usage_pairs: &[String],
    args_arr: &[String],
) {
    let cmds = extract_subcommands(usage_pairs);
    let flags = extract_flags_for_llm(args_arr);
    let first_line = title.lines().next().unwrap_or(title).trim();

    let _ = writeln!(out, "[");

    if cmds.is_empty() {
        write_openai_tool(out, &sanitize_tool_name(cmd_name), first_line, &flags, true);
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            let tool_name = sanitize_tool_name(&format!("{}_{}", cmd_name, cmd.name));
            let desc = if cmd.desc.is_empty() { first_line } else { &cmd.desc };
            write_openai_tool(out, &tool_name, desc, &flags, i == cmds.len() - 1);
        }
    }

    let _ = writeln!(out, "]");
}

fn write_openai_tool<W: Write>(out: &mut W, name: &str, description: &str, flags: &[FlagInfo], is_last: bool) {
    let _ = writeln!(out, "  {{");
    let _ = writeln!(out, "    \"type\": \"function\",");
    let _ = writeln!(out, "    \"function\": {{");
    let _ = writeln!(out, "      \"name\": \"{}\",", json_escape(name));
    let _ = writeln!(out, "      \"description\": \"{}\",", json_escape(description));
    let _ = writeln!(out, "      \"parameters\": {{");
    let _ = writeln!(out, "        \"type\": \"object\",");
    write_tool_properties(out, flags, "        ");
    let _ = writeln!(out, "      }}");
    let _ = writeln!(out, "    }}");
    let trailing = if is_last { "" } else { "," };
    let _ = writeln!(out, "  }}{}", trailing);
}
