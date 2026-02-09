//! Parse argsh field definitions and format help text.
//!
//! Mirrors: libraries/args.sh (args::field_name function, field parsing internals)

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::shell;
use std::ffi::{c_char, c_int};

// ── args::field_name builtin registration ────────────────────────

static FIELD_NAME_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Extract variable name from an argsh field definition.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "args::field_name_struct"]
pub static mut FIELD_NAME_STRUCT: BashBuiltin = BashBuiltin {
    name: c"args::field_name".as_ptr(),
    function: field_name_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"args::field_name <field> [asref]".as_ptr(),
    long_doc: FIELD_NAME_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "args::field_name_builtin_load"]
pub extern "C" fn field_name_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "args::field_name_builtin_unload"]
pub extern "C" fn field_name_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn field_name_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        if args.is_empty() {
            return 2;
        }
        let asref = args.get(1).map(|s| s != "0").unwrap_or(true);
        let name = field_name(&args[0], asref);
        println!("{}", name);
        0
    })
    .unwrap_or(1)
}

// ── Field parsing ────────────────────────────────────────────────

/// Parsed field definition from the args array.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FieldDef {
    pub name: String,         // variable name (dashes → underscores)
    pub display_name: String, // original name (with dashes)
    pub short: Option<String>,
    pub is_boolean: bool,     // :+ modifier
    pub type_name: String,    // "string", "int", "float", "file", etc.
    pub required: bool,       // :! modifier
    pub is_positional: bool,  // no | in definition
    pub is_hidden: bool,      // # prefix
    pub is_array: bool,       // variable declared as array in shell
    pub has_default: bool,    // variable already initialized
    pub is_multiple: bool,    // array variable (collects multiple values)
    pub raw: String,          // raw field definition string
}

/// Extract the variable name from a field definition.
/// 'flag|f:~int!' → "flag"
/// '#hidden|h' → "hidden"
/// 'my-flag|m' → "my_flag" (with asref=true, dashes → underscores)
pub fn field_name(field: &str, asref: bool) -> String {
    let mut name = field;
    // Remove everything after first | or :
    if let Some(pos) = name.find(['|', ':']) {
        name = &name[..pos];
    }
    // Remove leading #
    let name = name.strip_prefix('#').unwrap_or(name);
    if asref {
        name.replace('-', "_")
    } else {
        name.to_string()
    }
}

/// Parse a field definition string into a FieldDef.
pub fn parse_field(field: &str) -> FieldDef {
    let raw = field.to_string();
    let name = field_name(field, true);
    let display_name = field_name(field, false);
    let is_hidden = field.starts_with('#');
    let is_positional = !field.contains('|') && field != "-";

    // Parse short name
    let short = if !is_positional {
        let without_mods = field.split(':').next().unwrap_or(field);
        let parts: Vec<&str> = without_mods.split('|').collect();
        if parts.len() > 1 && !parts[1].is_empty() {
            Some(parts[1].to_string())
        } else {
            None
        }
    } else {
        None
    };

    // Parse modifiers after ':'
    let mut is_boolean = false;
    let mut type_name = String::new();
    let mut required = false;

    if let Some(colon_pos) = field.find(':') {
        let mods = &field[colon_pos + 1..];
        let mut chars = mods.chars().peekable();
        while let Some(&c) = chars.peek() {
            match c {
                '+' => {
                    is_boolean = true;
                    chars.next();
                }
                '~' => {
                    chars.next();
                    // Collect type name until next modifier
                    let mut tname = String::new();
                    while let Some(&tc) = chars.peek() {
                        if tc == '+' || tc == '~' || tc == '!' {
                            break;
                        }
                        tname.push(tc);
                        chars.next();
                    }
                    type_name = tname;
                }
                '!' => {
                    required = true;
                    chars.next();
                }
                _ => {
                    chars.next();
                }
            }
        }
    }

    // Default type
    if type_name.is_empty() && !is_boolean {
        type_name = "string".to_string();
    }

    // Check shell variable state
    let is_arr = shell::is_array(&name);
    let is_uninit = shell::is_uninitialized(&name);
    let is_multiple = is_arr;
    let has_default = if is_arr {
        !is_uninit && {
            let arr = shell::read_array(&name);
            !arr.is_empty()
        }
    } else {
        !is_uninit
    };

    FieldDef {
        name,
        display_name,
        short,
        is_boolean,
        type_name,
        required,
        is_positional,
        is_hidden,
        is_array: is_arr,
        has_default,
        is_multiple,
        raw,
    }
}

/// Convert a value to the expected type. Returns the converted value or an error message.
pub fn convert_type(
    type_name: &str,
    value: &str,
    _field_name: &str,
) -> Result<String, String> {
    match type_name {
        "" | "string" => Ok(value.to_string()),
        "int" => {
            if value.parse::<i64>().is_ok() {
                Ok(value.to_string())
            } else {
                Err(format!("invalid type (int): {}", value))
            }
        }
        "float" => {
            // Matches bash regex: ^-?[0-9]+(\.[0-9]+)?$
            let valid = value
                .strip_prefix('-')
                .unwrap_or(value)
                .split_once('.')
                .map(|(a, b)| {
                    !a.is_empty()
                        && a.chars().all(|c| c.is_ascii_digit())
                        && !b.is_empty()
                        && b.chars().all(|c| c.is_ascii_digit())
                })
                .unwrap_or_else(|| {
                    let s = value.strip_prefix('-').unwrap_or(value);
                    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
                });
            if valid {
                Ok(value.to_string())
            } else {
                Err(format!("invalid type (float): {}", value))
            }
        }
        "boolean" => match value {
            "" | "false" | "0" => Ok("0".to_string()),
            _ => Ok("1".to_string()),
        },
        "file" => {
            if std::path::Path::new(value).is_file() {
                Ok(value.to_string())
            } else {
                Err(format!("file not found: {}", value))
            }
        }
        "stdin" => {
            if value == "-" {
                // Read from stdin
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf).ok();
                Ok(buf)
            } else {
                Ok(value.to_string())
            }
        }
        custom => {
            // Validate type name to prevent injection
            if !custom
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(format!("invalid type name: {}", custom));
            }
            // Try calling the bash function to::${custom}
            let func_name = format!("to::{}", custom);
            if shell::function_exists(&func_name) {
                let cmd = format!(
                    "\"to::{}\" \"$__argsh_v\" \"$__argsh_f\"",
                    custom
                );
                shell::set_scalar("__argsh_v", value);
                shell::set_scalar("__argsh_f", _field_name);
                if let Some(result) = shell::exec_capture(&cmd, "__argsh_r") {
                    Ok(result)
                } else {
                    Err(format!("invalid type ({}): {}", custom, value))
                }
            } else {
                Err(format!("unknown type: {}", custom))
            }
        }
    }
}

/// Format a field for help text display.
/// For positionals: "name type"
/// For flags: "   -s, --name type (default: val)"
pub fn format_field(def: &FieldDef) -> String {
    if def.is_positional {
        return format!("{} {}", def.display_name, def.type_name);
    }

    let mut out = String::new();

    // Required marker
    if def.required {
        out.push_str(" ! ");
    } else {
        out.push_str("   ");
    }

    // Short + long
    if let Some(ref short) = def.short {
        out.push_str(&format!("-{}, --{}", short, def.display_name));
    } else {
        out.push_str(&format!("    --{}", def.display_name));
    }

    out.push(' ');

    // Multiple
    if def.is_multiple {
        out.push_str("...");
    }

    // Type
    out.push_str(&def.type_name);

    // Default value (only for non-boolean with existing value)
    if def.has_default && !def.is_boolean {
        if let Some(display) = shell::get_var_display(&def.name) {
            out.push_str(&format!(" (default: {})", display));
        }
    }

    out
}

/// Lookup a flag by name/short in the args array.
/// Returns the index of the matching definition.
pub fn field_lookup(flag: &str, args: &[String]) -> Option<usize> {
    for i in (0..args.len()).step_by(2) {
        let def = &args[i];
        let without_mods = def.split(':').next().unwrap_or(def);
        let without_hash = without_mods.strip_prefix('#').unwrap_or(without_mods);
        let parts: Vec<&str> = without_hash.split('|').collect();

        // Check long name (first part) or short name (second part)
        for part in &parts {
            if *part == flag {
                return Some(i);
            }
        }
    }
    None
}

/// Find the nth positional field. If a field is an array, it always matches
/// (catch-all behavior for remaining positionals).
pub fn field_positional(position: usize, args: &[String]) -> Option<usize> {
    let mut pos = position;
    for i in (0..args.len()).step_by(2) {
        let def = &args[i];
        // Positional = no | and not '-'
        if !def.contains('|') && def != "-" {
            let name = field_name(def, true);
            if shell::is_array(&name) || {
                pos -= 1;
                pos == 0
            } {
                return Some(i);
            }
        }
    }
    None
}
