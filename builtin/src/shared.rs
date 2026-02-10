//! Shared helpers for :args and :usage builtins.
//!
//! Extracted from args.rs and usage.rs to eliminate code duplication (REVIEW finding 3).
//! All error functions return exit codes instead of calling std::process::exit() (REVIEW finding 2).

use crate::field;
use crate::shell;

/// Exit code for usage/argument errors.
pub const EXIT_USAGE: i32 = 2;

/// Sentinel: help/version was displayed, caller should exit with 0.
/// Distinct from 0 (success, continue script).
pub const HELP_EXIT: i32 = -1;

/// Print a usage error and return exit code 2.
/// Does NOT call std::process::exit() -- returns the code for the caller to propagate.
pub fn error_usage(_field: &str, msg: &str) -> i32 {
    let script = shell::get_script_name();
    eprintln!("Error: {}\n", msg);
    eprintln!("  Run \"{} -h\" for more information.", script);
    EXIT_USAGE
}

/// Print an argument error and return exit code 2.
/// Does NOT call std::process::exit() -- returns the code for the caller to propagate.
pub fn error_args(_field: &str, msg: &str) -> i32 {
    let script = shell::get_script_name();
    eprintln!("Error: {}\n", msg);
    eprintln!("  Run \"{} -h\" for more information.", script);
    EXIT_USAGE
}

/// Parse a flag at position `idx` in the cli args.
/// Returns Ok(true) if parsed, Ok(false) if not a known flag, Err(code) on error.
pub fn parse_flag_at(
    cli: &mut Vec<String>,
    idx: usize,
    args_arr: &[String],
    matched: &mut Vec<String>,
    set_bool: fn(&str),
) -> Result<bool, i32> {
    if idx >= cli.len() {
        return Ok(false);
    }

    let arg = cli[idx].clone();
    let flag_part = arg.split('=').next().unwrap_or(&arg);

    let (lookup_name, is_long) = if let Some(stripped) = flag_part.strip_prefix("--") {
        (stripped.to_string(), true)
    } else if flag_part.starts_with('-') && flag_part.len() >= 2 {
        (flag_part[1..2].to_string(), false)
    } else {
        return Ok(false);
    };

    // Find field in args array
    let field_idx = match field::field_lookup(&lookup_name, args_arr) {
        Some(i) => i,
        None => return Ok(false),
    };

    let field_str = &args_arr[field_idx];
    matched.push(field_str.clone());
    let def = field::parse_field(field_str);

    // Boolean flag (no value)
    if def.is_boolean {
        if def.is_multiple || shell::is_array(&def.name) {
            shell::array_append(&def.name, "1");
        } else {
            set_bool(&def.name);
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
        return Ok(true);
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
                return Err(error_args(
                    &def.name,
                    &format!("missing value for flag: {}", def.name),
                ));
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
                return Err(error_args(
                    &def.name,
                    &format!("missing value for flag: {}", def.name),
                ));
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
            return Err(error_usage(field_str, &msg));
        }
    };

    // Set variable
    if def.is_multiple {
        shell::array_append(&def.name, &converted);
    } else {
        shell::set_scalar(&def.name, &converted);
    }

    Ok(true)
}

/// Check required flags and set boolean defaults.
/// Returns 0 on success, or an error exit code.
pub fn check_required_flags(args_arr: &[String], matched: &[String]) -> i32 {
    for i in (0..args_arr.len()).step_by(2) {
        let field_str = &args_arr[i];
        if field_str == "-" {
            continue;
        }
        let def = field::parse_field(field_str);
        if def.is_positional {
            continue;
        }

        // Set boolean to false if not matched and no default
        if def.is_boolean && !def.has_default && !matched.contains(field_str) {
            // For arrays: sets arr[0]=0. For scalars: sets var=0.
            shell::set_scalar(&def.name, "0");
        }

        // Check required
        if def.required && !matched.contains(field_str) {
            let display = field_str.split('|').next().unwrap_or(field_str);
            return error_usage(field_str, &format!("missing required flag: {}", display));
        }
    }
    0
}

// NOTE: Unit tests cannot run via `cargo test` because this crate is a cdylib
// that links against bash symbols (dollar_vars, find_variable, etc.) which are
// only available inside the bash process. All testing is done via BATS:
//   ARGSH_SOURCE=argsh bats libraries/args.bats
