//! :usage builtin — subcommand dispatch with prefix resolution.

use crate::field;
use crate::shell;
use std::io::Write;

/// Main entry point for :usage builtin.
/// Returns exit code (0 = success, 2 = usage error).
pub fn usage_main(args: &[String]) -> i32 {
    if args.is_empty() {
        shell::write_stderr(":args error [???] ➜ :usage requires a title argument");
        return 2;
    }

    let title = &args[0];
    let cli_args = &args[1..];

    // Read usage and args arrays from shell scope
    let usage_arr = shell::read_array("usage");
    let args_arr = shell::read_array("args");

    // Validate usage array is pairs
    if usage_arr.len() % 2 != 0 {
        shell::write_stderr(":args error [???] ➜ usage must be an associative array");
        std::process::exit(2);
    }

    // Handle empty args, -h, --help
    if cli_args.is_empty()
        || cli_args[0] == "-h"
        || cli_args[0] == "--help"
    {
        usage_help_text(title, &usage_arr, &args_arr);
        std::process::exit(0);
    }

    // Handle --argsh
    let commandname = shell::get_commandname();
    if commandname.is_empty() && cli_args.first().map(|s| s.as_str()) == Some("--argsh") {
        let sha = shell::get_scalar("ARGSH_COMMIT_SHA").unwrap_or_default();
        let ver = shell::get_scalar("ARGSH_VERSION").unwrap_or_default();
        println!("https://arg.sh {} {}", sha, ver);
        std::process::exit(0);
    }

    // Parse flags and find command
    let mut cli: Vec<String> = cli_args.to_vec();
    let mut cmd: Option<String> = None;
    let mut matched: Vec<String> = Vec::new();

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

        // Try parsing as flag
        if let Some(result) = parse_flag_at(&mut cli, idx, &args_arr, &mut matched) {
            if !result {
                break; // Unknown flag, leave for subcommand
            }
            // idx stays the same since parse_flag_at modifies cli
        } else {
            break;
        }
    }

    // Check required flags
    check_required_flags(&args_arr, &matched);

    let cmd = match cmd {
        Some(c) => c,
        None => {
            error_usage("???", &format!("Invalid command: "));
            unreachable!()
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
        error_usage("???", &format!("Invalid command: {}", cmd));
        unreachable!()
    }

    // Resolve function with prefix fallback
    let explicit = found_field.contains(":-");

    if explicit {
        if !shell::function_exists(&func) {
            error_usage(&cmd, &format!("Invalid command: {}", cmd));
            unreachable!()
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
            error_usage(&cmd, &format!("Invalid command: {}", cmd));
            unreachable!()
        }
    }

    // Append to COMMANDNAME
    let cmd_name = found_field.split(|c: char| c == '|' || c == ':').next().unwrap_or(&found_field);
    shell::append_commandname(cmd_name);

    // Set usage = (func remaining_args...)
    let mut new_usage = vec![func];
    new_usage.extend(cli.into_iter());
    shell::write_array("usage", &new_usage);

    0 // EXECUTION_SUCCESS
}

/// Parse a flag at position `idx` in the cli args.
/// Returns Some(true) if parsed, Some(false) if not a known flag, None on error.
fn parse_flag_at(
    cli: &mut Vec<String>,
    idx: usize,
    args_arr: &[String],
    matched: &mut Vec<String>,
) -> Option<bool> {
    if idx >= cli.len() {
        return None;
    }

    let arg = cli[idx].clone();
    let flag_part = arg.split('=').next().unwrap_or(&arg);

    let (lookup_name, is_long) = if flag_part.starts_with("--") {
        (flag_part[2..].to_string(), true)
    } else if flag_part.starts_with('-') && flag_part.len() >= 2 {
        (flag_part[1..2].to_string(), false)
    } else {
        return Some(false);
    };

    // Find field in args array
    let field_idx = match field::field_lookup(&lookup_name, args_arr) {
        Some(i) => i,
        None => return Some(false),
    };

    let field_str = &args_arr[field_idx];
    matched.push(field_str.clone());
    let def = field::parse_field(field_str);

    // Boolean flag (no value)
    if def.is_boolean {
        // Set variable
        if def.is_multiple {
            shell::array_append(&def.name, "1");
        } else {
            set_or_increment(&def.name);
        }

        if is_long {
            cli.remove(idx);
        } else {
            // Short flag: strip this char, keep rest
            let remaining = format!("-{}", &cli[idx][2..]);
            if remaining == "-" {
                cli.remove(idx);
            } else {
                cli[idx] = remaining;
            }
        }
        return Some(true);
    }

    // Value flag
    let value = if is_long {
        // Check for --flag=value
        if arg.contains('=') {
            let val = arg.splitn(2, '=').nth(1).unwrap_or("").to_string();
            cli.remove(idx);
            val
        } else {
            // Value is next arg
            cli.remove(idx);
            if idx >= cli.len() {
                error_args(&def.name, &format!("missing value for flag: {}", def.name));
                unreachable!()
            }
            let val = cli[idx].clone();
            cli.remove(idx);
            val
        }
    } else {
        // Short flag: -fvalue or -f value
        let inline_val = &cli[idx][2..];
        if inline_val.is_empty() {
            // Value is next arg
            cli.remove(idx);
            if idx >= cli.len() {
                error_args(&def.name, &format!("missing value for flag: {}", def.name));
                unreachable!()
            }
            let val = cli[idx].clone();
            cli.remove(idx);
            val
        } else {
            // Check for =value
            let val = if inline_val.starts_with('=') {
                inline_val[1..].to_string()
            } else {
                inline_val.to_string()
            };
            cli.remove(idx);
            val
        }
    };

    // Type convert
    let converted = match field::convert_type(&def.type_name, &value, &def.name) {
        Ok(v) => v,
        Err(msg) => {
            error_usage(&field_str, &msg);
            unreachable!()
        }
    };

    // Set variable
    if def.is_multiple {
        shell::array_append(&def.name, &converted);
    } else {
        shell::set_scalar(&def.name, &converted);
    }

    Some(true)
}

fn set_or_increment(name: &str) {
    if shell::is_array(name) {
        shell::array_append(name, "1");
    } else {
        shell::set_scalar(name, "1");
    }
}

/// Check required flags and set boolean defaults.
fn check_required_flags(args_arr: &[String], matched: &[String]) {
    for i in (0..args_arr.len()).step_by(2) {
        let field_str = &args_arr[i];
        if field_str == "-" {
            continue;
        }
        let def = field::parse_field(field_str);

        // Set boolean to false if not matched and no default
        if def.is_boolean && !def.has_default {
            if !matched.contains(field_str) {
                // For arrays: sets arr[0]=0. For scalars: sets var=0.
                shell::set_scalar(&def.name, "0");
            }
        }

        // Check required
        if def.required && !matched.contains(field_str) {
            let display = field_str.split('|').next().unwrap_or(field_str);
            error_usage(field_str, &format!("missing required flag: {}", display));
        }
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
        let name = entry.split(|c: char| c == '|' || c == ':').next().unwrap_or(entry);
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

/// Print error and exit with code 2.
fn error_usage(field: &str, msg: &str) {
    let field_display = field.split(|c: char| c == '|' || c == ':').next().unwrap_or(field);
    let script = shell::get_script_name();
    eprint!("[ {} ] invalid usage\n\u{279c} {}\n\n", field_display, msg);
    eprintln!("Use \"{} -h\" for more information", script);
    std::process::exit(2);
}

fn error_args(field: &str, msg: &str) {
    let field_display = field.split(|c: char| c == '|' || c == ':').next().unwrap_or(field);
    eprint!("[ {} ] invalid argument\n\u{279c} {}\n\n", field_display, msg);
    std::process::exit(2);
}
