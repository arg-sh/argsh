//! :args builtin — argument parsing with type checking.
//!
//! Mirrors: libraries/args.sh (:args function)

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::field;
use crate::shell;
use crate::usage;
use std::ffi::{c_char, c_int};
use std::io::Write;

// ── Builtin registration ─────────────────────────────────────────

static ARGS_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Parse arguments and flags from the args array.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":args_struct"]
pub static mut ARGS_STRUCT: BashBuiltin = BashBuiltin {
    name: c":args".as_ptr(),
    function: args_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c":args <title> [args...]".as_ptr(),
    long_doc: ARGS_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":args_builtin_load"]
pub extern "C" fn args_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":args_builtin_unload"]
pub extern "C" fn args_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn args_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        args_main(&args)
    })
    .unwrap_or(1)
}

// ── Implementation ───────────────────────────────────────────────

/// Main entry point for :args builtin.
/// Returns exit code (0 = success, 2 = usage error).
pub fn args_main(args: &[String]) -> i32 {
    if args.is_empty() {
        shell::write_stderr(":args error [???] ➜ :args requires a title argument"); // coverage:off - exit(2) prevents coverage flush in forked subshell
        return 2; // coverage:off - exit(2) prevents coverage flush in forked subshell
    }

    let title = &args[0];
    let cli_args = &args[1..];

    // Read args array from shell scope
    let args_arr = shell::read_array("args");

    // Validate args array is pairs
    if !args_arr.len().is_multiple_of(2) {
        shell::write_stderr(":args error [???] ➜ args must be an associative array"); // coverage:off - exit(2) prevents coverage flush in forked subshell
        std::process::exit(2); // coverage:off - exit(2) prevents coverage flush in forked subshell
    }

    // Handle -h, --help
    if cli_args.first().map(|s| s.as_str()) == Some("-h")
        || cli_args.first().map(|s| s.as_str()) == Some("--help")
    {
        args_help_text(title, &args_arr);
        std::process::exit(0);
    }

    // Parse CLI arguments
    let mut cli: Vec<String> = cli_args.to_vec();
    let mut positional_index: usize = 1;
    let mut matched: Vec<String> = Vec::new();
    let mut first_array = false;

    // idx stays 0: we always process the front element; cli.remove(0) shifts the rest down
    let idx = 0;
    while idx < cli.len() {
        // Positional argument
        if !cli[idx].starts_with('-') {
            let pos_idx = match field::field_positional(positional_index, &args_arr) {
                Some(i) => i,
                None => {
                    error_usage("???", &format!("too many arguments: {}", cli[idx])); // coverage:off - exit(2) prevents coverage flush in forked subshell
                    unreachable!() // coverage:off - exit(2) prevents coverage flush in forked subshell
                }
            };

            let field_str = &args_arr[pos_idx];
            let name = field::field_name(field_str, true);
            let def = field::parse_field(field_str);

            // Type convert
            let value = match field::convert_type(&def.type_name, &cli[idx], &name) {
                Ok(v) => v,
                Err(msg) => {
                    error_usage(field_str, &msg);
                    unreachable!()
                }
            };

            if shell::is_array(&name) {
                if !first_array {
                    // Clear array on first assignment
                    shell::write_array(&name, &[]);
                    first_array = true;
                }
                shell::array_append(&name, &value);
            } else {
                shell::set_scalar(&name, &value);
            }

            cli.remove(idx);
            positional_index += 1;
            continue;
        }

        // Flag argument
        if let Some(result) = parse_flag_at(&mut cli, idx, &args_arr, &mut matched) {
            if !result {
                error_usage("???", &format!("unknown flag: {}", cli[idx]));
                unreachable!()
            }
            // idx stays same since parse_flag_at modifies cli
        } else {
            error_usage("???", &format!("unknown flag: {}", cli[idx]));
            unreachable!()
        }
    }

    // Check if next expected positional is required
    if let Some(pos_idx) = field::field_positional(positional_index, &args_arr) {
        let field_str = &args_arr[pos_idx];
        let name = field::field_name(field_str, true);
        if shell::is_uninitialized(&name) && !shell::is_array(&name) {
            error_usage(&name, &format!("missing required argument: {}", name));
        }
    }

    // Check required flags and set boolean defaults
    check_required_flags(&args_arr, &matched);

    // Error on remaining args
    if !cli.is_empty() {
        error_usage("???", &format!("too many arguments: {}", cli.join(" ")));
    }

    0 // EXECUTION_SUCCESS
}

/// Parse a flag at position `idx` in the cli args.
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

    let (lookup_name, is_long) = if let Some(stripped) = flag_part.strip_prefix("--") {
        (stripped.to_string(), true)
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
        if def.is_multiple || shell::is_array(&def.name) {
            shell::array_append(&def.name, "1");
        } else {
            shell::set_scalar(&def.name, "1");
        }

        if is_long {
            cli.remove(idx);
        } else {
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
        if arg.contains('=') {
            let val = arg.split_once('=').map(|x| x.1).unwrap_or("").to_string();
            cli.remove(idx);
            val
        } else {
            cli.remove(idx);
            if idx >= cli.len() {
                error_args(&def.name, &format!("missing value for flag: {}", def.name)); // coverage:off - exit(2) prevents coverage flush in forked subshell
                unreachable!() // coverage:off - exit(2) prevents coverage flush in forked subshell
            }
            let val = cli[idx].clone();
            cli.remove(idx);
            val
        }
    } else {
        let inline_val = &cli[idx][2..];
        if inline_val.is_empty() {
            cli.remove(idx);
            if idx >= cli.len() {
                error_args(&def.name, &format!("missing value for flag: {}", def.name)); // coverage:off - exit(2) prevents coverage flush in forked subshell
                unreachable!() // coverage:off - exit(2) prevents coverage flush in forked subshell
            }
            let val = cli[idx].clone();
            cli.remove(idx);
            val
        } else {
            let val = if let Some(stripped) = inline_val.strip_prefix('=') {
                stripped.to_string()
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
            error_usage(field_str, &msg);
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

/// Check required flags and set boolean defaults.
fn check_required_flags(args_arr: &[String], matched: &[String]) {
    for i in (0..args_arr.len()).step_by(2) {
        let field_str = &args_arr[i];
        if field_str == "-" {
            continue;
        }
        let def = field::parse_field(field_str);
        if def.is_positional {
            continue; // Skip positionals, they're checked separately
        }

        // Set boolean to false if not matched and no default
        if def.is_boolean && !def.has_default && !matched.contains(field_str) {
            // For arrays: sets arr[0]=0. For scalars: sets var=0.
            shell::set_scalar(&def.name, "0");
        }

        // Check required
        if def.required && !matched.contains(field_str) {
            let display = field_str.split('|').next().unwrap_or(field_str);
            error_usage(field_str, &format!("missing required flag: {}", display));
        }
    }
}

/// Print :args help text.
fn args_help_text(title: &str, args_arr: &[String]) {
    let out = std::io::stdout();
    let mut out = out.lock();
    let fw = shell::get_field_width();
    let commandname = shell::get_commandname();
    let cmdname_str = commandname.join(" ");

    // Title (trim leading whitespace per line)
    for line in title.lines() {
        let _ = writeln!(out, "{}", line.trim_start());
    }

    // Build positionals and params
    let mut positional_indices: Vec<usize> = Vec::new();
    let mut params: Vec<String> = Vec::new();

    for i in (0..args_arr.len()).step_by(2) {
        let entry = &args_arr[i];
        if entry.contains('|') || entry == "-" {
            continue;
        }
        let name = field::field_name(entry, true);
        positional_indices.push(i);

        if shell::is_array(&name) {
            params.push(format!("...{}", name));
        } else if !shell::is_uninitialized(&name) {
            params.push(format!("[{}]", name));
        } else {
            params.push(format!("<{}>", name));
        }
    }

    // Usage line
    let _ = writeln!(out);
    let _ = writeln!(out, "Usage:");
    let _ = writeln!(out, "  {} {}", cmdname_str, params.join(" "));

    // Arguments section
    if !positional_indices.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Arguments:");

        for &i in &positional_indices {
            let entry = &args_arr[i];
            if entry == "-" {
                continue;
            }
            let desc = args_arr.get(i + 1).map(|s| s.as_str()).unwrap_or("");
            let def = field::parse_field(entry);
            let field_fmt = field::format_field(&def);

            let _ = writeln!(out, "   {:width$}{}", field_fmt, desc, width = fw);
        }
    }

    // Flags section
    usage::print_flags_section(&mut out, args_arr, fw);

    let _ = writeln!(out);
}

/// Print error and exit with code 2.
fn error_usage(field: &str, msg: &str) { // coverage:off - exit(2) prevents coverage flush in forked subshell
    let field_display = field.split(['|', ':']).next().unwrap_or(field); // coverage:off - exit(2) prevents coverage flush in forked subshell
    let script = shell::get_script_name(); // coverage:off - exit(2) prevents coverage flush in forked subshell
    eprint!("[ {} ] invalid usage\n\u{279c} {}\n\n", field_display, msg); // coverage:off - exit(2) prevents coverage flush in forked subshell
    eprintln!("Use \"{} -h\" for more information", script); // coverage:off - exit(2) prevents coverage flush in forked subshell
    std::process::exit(2); // coverage:off - exit(2) prevents coverage flush in forked subshell
}

fn error_args(field: &str, msg: &str) { // coverage:off - exit(2) prevents coverage flush in forked subshell
    let field_display = field.split(['|', ':']).next().unwrap_or(field); // coverage:off - exit(2) prevents coverage flush in forked subshell
    eprint!("[ {} ] invalid argument\n\u{279c} {}\n\n", field_display, msg); // coverage:off - exit(2) prevents coverage flush in forked subshell
    std::process::exit(2); // coverage:off - exit(2) prevents coverage flush in forked subshell
}
