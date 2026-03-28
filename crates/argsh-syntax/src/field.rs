//! Parse argsh field definitions.
//!
//! Mirrors the parsing logic in `builtin/src/field.rs` but without any
//! shell FFI calls — pure text-only parsing suitable for static analysis.

use std::fmt;

/// Error returned when a field spec is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub message: String,
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FieldError {}

/// Parsed field definition from an args array entry like `'name|alias:~int:!'`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    /// Variable name (dashes replaced with underscores).
    pub name: String,
    /// Display name (preserves original dashes).
    pub display_name: String,
    /// Short alias (e.g. `"v"` from `"verbose|v"`).
    pub short: Option<String>,
    /// `:+` modifier — flag that takes no value.
    pub is_boolean: bool,
    /// Type after `:~` (int, float, file, boolean, string, or custom).
    pub type_name: String,
    /// `:!` modifier — field is required.
    pub required: bool,
    /// `#` prefix on name — field is hidden from help text.
    pub hidden: bool,
    /// No `|` separator in definition — positional parameter.
    pub is_positional: bool,
    /// Raw spec string, preserved for diagnostics.
    pub raw: String,
}

/// Extract the variable name from a field definition string.
///
/// `'flag|f:~int!'` -> `"flag"`
/// `'#hidden|h'`    -> `"hidden"`
/// `'my-flag|m'`    -> `"my_flag"` (with `asref=true`, dashes become underscores)
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

/// Parse a field spec string like `'name|alias:~int:!'` into a [`FieldDef`].
///
/// Handles:
/// - `name|alias` — name with short alias
/// - `name` (no `|`) — positional parameter
/// - `:+` — boolean flag
/// - `:~type` — typed parameter (int, float, file, boolean, string, or custom)
/// - `:!` — required
/// - `:#` — hidden (also `#` prefix on name)
/// - Error on conflicting modifiers (`:+` with `:~type`)
/// - Error on unknown modifiers
pub fn parse_field(spec: &str) -> Result<FieldDef, FieldError> {
    let raw = spec.to_string();
    let name = field_name(spec, true);
    let display_name = field_name(spec, false);
    let hidden = spec.starts_with('#');
    let is_positional = !spec.contains('|') && spec != "-";

    // Parse short name
    let short = if !is_positional {
        let without_mods = spec.split(':').next().unwrap_or(spec);
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
    let mut saw_hidden_mod = false;

    if let Some(colon_pos) = spec.find(':') {
        let mods = &spec[colon_pos + 1..];
        let mut chars = mods.chars().peekable();
        while let Some(&c) = chars.peek() {
            match c {
                '+' => {
                    if !type_name.is_empty() {
                        return Err(FieldError {
                            message: format!(
                                "cannot have multiple types: {} and boolean",
                                type_name
                            ),
                        });
                    }
                    is_boolean = true;
                    chars.next();
                }
                '~' => {
                    if is_boolean {
                        return Err(FieldError {
                            message: "already flagged as boolean".to_string(),
                        });
                    }
                    chars.next();
                    // Collect type name until next modifier
                    let mut tname = String::new();
                    while let Some(&tc) = chars.peek() {
                        if tc == '+' || tc == '~' || tc == '!' || tc == '#' {
                            break;
                        }
                        tname.push(tc);
                        chars.next();
                    }
                    type_name = tname;
                }
                '!' => {
                    if required {
                        return Err(FieldError {
                            message: "field already flagged as required".to_string(),
                        });
                    }
                    required = true;
                    chars.next();
                }
                '#' => {
                    saw_hidden_mod = true;
                    chars.next();
                }
                _ => {
                    return Err(FieldError {
                        message: format!("unknown modifier: {}", c),
                    });
                }
            }
        }
    }

    // Default type
    if type_name.is_empty() && !is_boolean {
        type_name = "string".to_string();
    }

    Ok(FieldDef {
        name,
        display_name,
        short,
        is_boolean,
        type_name,
        required,
        hidden: hidden || saw_hidden_mod,
        is_positional,
        raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_name_simple() {
        assert_eq!(field_name("verbose|v:+", true), "verbose");
        assert_eq!(field_name("verbose|v:+", false), "verbose");
    }

    #[test]
    fn test_field_name_hidden() {
        assert_eq!(field_name("#hidden|h", true), "hidden");
    }

    #[test]
    fn test_field_name_dashes() {
        assert_eq!(field_name("my-flag|m", true), "my_flag");
        assert_eq!(field_name("my-flag|m", false), "my-flag");
    }

    #[test]
    fn test_positional() {
        let def = parse_field("pos1").unwrap();
        assert_eq!(def.name, "pos1");
        assert!(def.is_positional);
        assert!(!def.is_boolean);
        assert_eq!(def.type_name, "string");
        assert!(!def.required);
        assert!(!def.hidden);
        assert!(def.short.is_none());
    }

    #[test]
    fn test_flag_with_short() {
        let def = parse_field("verbose|v:+").unwrap();
        assert_eq!(def.name, "verbose");
        assert_eq!(def.short, Some("v".to_string()));
        assert!(def.is_boolean);
        assert!(!def.is_positional);
    }

    #[test]
    fn test_flag_long_only() {
        let def = parse_field("longonly|:~string").unwrap();
        assert_eq!(def.name, "longonly");
        assert!(def.short.is_none());
        assert!(!def.is_positional);
        assert_eq!(def.type_name, "string");
    }

    #[test]
    fn test_typed_int() {
        let def = parse_field("count|c:~int").unwrap();
        assert_eq!(def.type_name, "int");
        assert!(!def.is_boolean);
    }

    #[test]
    fn test_typed_float() {
        let def = parse_field("val|:~float").unwrap();
        assert_eq!(def.type_name, "float");
    }

    #[test]
    fn test_required() {
        let def = parse_field("name|n:!").unwrap();
        assert!(def.required);
    }

    #[test]
    fn test_typed_required() {
        let def = parse_field("arg8|8:~string!").unwrap();
        assert_eq!(def.type_name, "string");
        assert!(def.required);
    }

    #[test]
    fn test_boolean_required() {
        let def = parse_field("arg9|9:+!").unwrap();
        assert!(def.is_boolean);
        assert!(def.required);
    }

    #[test]
    fn test_hidden_prefix() {
        let def = parse_field("#cmd3").unwrap();
        assert!(def.hidden);
        assert_eq!(def.name, "cmd3");
    }

    #[test]
    fn test_hidden_modifier() {
        let def = parse_field("secret|s:#").unwrap();
        assert!(def.hidden);
    }

    #[test]
    fn test_positional_typed() {
        let def = parse_field("pos3:~int").unwrap();
        assert!(def.is_positional);
        assert_eq!(def.type_name, "int");
    }

    #[test]
    fn test_custom_type() {
        let def = parse_field("arg7|7:~custom").unwrap();
        assert_eq!(def.type_name, "custom");
    }

    #[test]
    fn test_error_boolean_with_type() {
        let err = parse_field("bad|b:+~int").unwrap_err();
        assert!(err.message.contains("already flagged as boolean"));
    }

    #[test]
    fn test_error_type_with_boolean() {
        let err = parse_field("bad|b:~int+").unwrap_err();
        assert!(err.message.contains("cannot have multiple types"));
    }

    #[test]
    fn test_error_duplicate_required() {
        let err = parse_field("bad|b:!!").unwrap_err();
        assert!(err.message.contains("already flagged as required"));
    }

    #[test]
    fn test_error_unknown_modifier() {
        let err = parse_field("bad|b:x").unwrap_err();
        assert!(err.message.contains("unknown modifier: x"));
    }

    #[test]
    fn test_default_string_no_pipe() {
        let def = parse_field("arg1|").unwrap();
        assert_eq!(def.name, "arg1");
        assert!(!def.is_positional);
        assert_eq!(def.type_name, "string");
        assert!(def.short.is_none());
    }
}
