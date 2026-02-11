//! :args builtin -- argument parsing with type checking.
//!
//! Mirrors: libraries/args.sh (:args function)

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::field;
use crate::shared;
use crate::shell;
use std::ffi::{c_char, c_int};
use std::io::Write;

// -- Builtin registration ---------------------------------------------------

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
    let code = std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        args_main(&args)
    })
    .unwrap_or(1); // coverage:off - catch_unwind: panics don't occur in practice

    // Match bash's `exit` behavior: help and errors terminate the script.
    // Only success (0) returns to the caller so the script can continue.
    match code {
        0 => 0,
        shared::HELP_EXIT => std::process::exit(0), // coverage:off - exit() kills process before coverage flush
        n => std::process::exit(n), // coverage:off - exit() kills process before coverage flush
    }
}

// -- Implementation ---------------------------------------------------------

/// Main entry point for :args builtin.
/// Returns exit code (0 = success, 2 = usage error).
pub fn args_main(args: &[String]) -> i32 {
    if args.is_empty() {
        return shared::error_usage("", ":args requires a title argument"); // coverage:off - set_e_kills: :args always called with title from bash wrapper
    }

    let title = &args[0];
    let cli_args = &args[1..];

    // Read args array from shell scope
    let args_arr = shell::read_array("args");

    // Validate args array is pairs (REVIEW finding 4: use % for Rust <1.87 compat)
    #[allow(clippy::manual_is_multiple_of)]
    if args_arr.len() % 2 != 0 {
        return shared::error_usage("", "args array must have an even number of elements");
    }

    // Handle -h, --help
    if cli_args.first().map(|s| s.as_str()) == Some("-h")
        || cli_args.first().map(|s| s.as_str()) == Some("--help")
    {
        args_help_text(title, &args_arr);
        return shared::HELP_EXIT;
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
                    return shared::error_usage("", &format!("too many arguments: {}", cli[idx]));
                }
            };

            let field_str = &args_arr[pos_idx];
            let name = field::field_name(field_str, true);
            let def = field::parse_field(field_str);

            // Type convert
            let value = match field::convert_type(&def.type_name, &cli[idx], &name) {
                Ok(v) => v,
                Err(msg) => {
                    return shared::error_usage(field_str, &msg);
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

        // Flag argument -- set_bool for :args just sets scalar "1"
        match shared::parse_flag_at(&mut cli, idx, &args_arr, &mut matched, |name| {
            shell::set_scalar(name, "1");
        }) {
            Ok(true) => {
                // idx stays same since parse_flag_at modifies cli
            }
            Ok(false) => {
                return shared::error_usage("", &format!("unknown flag: {}", cli[idx]));
            }
            Err(code) => return code,
        }
    }

    // Check if next expected positional is required
    if let Some(pos_idx) = field::field_positional(positional_index, &args_arr) {
        let field_str = &args_arr[pos_idx];
        let name = field::field_name(field_str, true);
        if shell::is_uninitialized(&name) && !shell::is_array(&name) {
            return shared::error_usage(&name, &format!("missing required argument: {}", name));
        }
    }

    // Check required flags and set boolean defaults
    let ret = shared::check_required_flags(&args_arr, &matched);
    if ret != 0 {
        return ret;
    }

    // Error on remaining args
    if !cli.is_empty() { // coverage:off - dead_code: while loop exhausts cli or errors; this guard is defensive
        return shared::error_usage("", &format!("too many arguments: {}", cli.join(" "))); // coverage:off
    } // coverage:off

    0 // EXECUTION_SUCCESS
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
            if entry == "-" { // coverage:off - dead_code: "-" entries filtered out of positional_indices at line 184
                continue; // coverage:off
            } // coverage:off
            let desc = args_arr.get(i + 1).map(|s| s.as_str()).unwrap_or("");
            let def = field::parse_field(entry);
            let field_fmt = field::format_field(&def);

            let _ = writeln!(out, "   {:width$}{}", field_fmt, desc, width = fw);
        }
    }

    // Flags section
    crate::usage::print_flags_section(&mut out, args_arr, fw);

    let _ = writeln!(out);
}
