//! Parse argsh usage entries.
//!
//! Mirrors the usage-entry parsing logic from `builtin/src/usage/mod.rs`
//! but without any shell FFI — pure text-only parsing.

/// Parsed usage entry from a usage array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEntry {
    /// Primary command name (first name before `|`).
    pub name: String,
    /// All aliases (including the primary name).
    pub aliases: Vec<String>,
    /// Description from the paired array element (set externally, not parsed here).
    pub description: String,
    /// Explicit function mapping from `:-func`.
    pub explicit_func: Option<String>,
    /// Annotations extracted from the entry (e.g. `@readonly`, `@destructive`).
    pub annotations: Vec<String>,
    /// `#` prefix — hidden from help text.
    pub hidden: bool,
    /// Entry is `"-"` — a group separator.
    pub is_group_separator: bool,
    /// 0-based line number where this entry appeared in the source.
    pub line: usize,
}

/// Parse annotations from a usage entry name.
///
/// Annotations are `@word` suffixes on the entry name, e.g.
/// `cmd@readonly@destructive` yields `("cmd", vec!["readonly", "destructive"])`.
pub fn parse_annotations(name: &str) -> (String, Vec<String>) {
    let parts: Vec<&str> = name.split('@').collect();
    let clean_name = parts[0].to_string();
    let annotations: Vec<String> = parts[1..].iter().filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
    (clean_name, annotations)
}

/// Parse a usage entry spec like `'cmd|alias:-func@readonly'`.
///
/// The `description` field is left empty — callers should populate it from
/// the paired array element.
pub fn parse_usage_entry(spec: &str) -> UsageEntry {
    // Group separator
    if spec == "-" {
        return UsageEntry {
            name: "-".to_string(),
            aliases: vec![],
            description: String::new(),
            explicit_func: None,
            annotations: vec![],
            hidden: false,
            is_group_separator: true,
            line: 0,
        };
    }

    let hidden = spec.starts_with('#');
    let work = if hidden { &spec[1..] } else { spec };

    // Split off explicit function mapping `:-func`
    let (name_part, explicit_func) = if let Some(pos) = work.find(":-") {
        let func_and_rest = &work[pos + 2..];
        // The function name may itself have annotations
        let (func_name, _) = parse_annotations(func_and_rest);
        (&work[..pos], Some(func_name))
    } else {
        (work, None)
    };

    // Remove colon-modifiers that are NOT `:-` (field modifiers on usage entries
    // are uncommon but let's strip them to get a clean name).
    let name_part = name_part.split(':').next().unwrap_or(name_part);

    // Extract annotations from the name part
    let (clean_name, annotations) = parse_annotations(name_part);

    // Split aliases
    let alias_parts: Vec<&str> = clean_name.split('|').collect();
    let primary = alias_parts[0].to_string();
    let aliases: Vec<String> = alias_parts.iter().map(|s| s.to_string()).collect();

    UsageEntry {
        name: primary,
        aliases,
        description: String::new(),
        explicit_func,
        annotations,
        hidden,
        is_group_separator: false,
        line: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let entry = parse_usage_entry("cmd1");
        assert_eq!(entry.name, "cmd1");
        assert_eq!(entry.aliases, vec!["cmd1"]);
        assert!(!entry.hidden);
        assert!(!entry.is_group_separator);
        assert!(entry.explicit_func.is_none());
        assert!(entry.annotations.is_empty());
    }

    #[test]
    fn test_command_with_alias() {
        let entry = parse_usage_entry("cmd1|alias");
        assert_eq!(entry.name, "cmd1");
        assert_eq!(entry.aliases, vec!["cmd1", "alias"]);
    }

    #[test]
    fn test_explicit_func() {
        let entry = parse_usage_entry("cmd2:-main::cmd2");
        assert_eq!(entry.name, "cmd2");
        assert_eq!(entry.explicit_func, Some("main::cmd2".to_string()));
    }

    #[test]
    fn test_hidden() {
        let entry = parse_usage_entry("#cmd3");
        assert_eq!(entry.name, "cmd3");
        assert!(entry.hidden);
    }

    #[test]
    fn test_group_separator() {
        let entry = parse_usage_entry("-");
        assert!(entry.is_group_separator);
        assert_eq!(entry.name, "-");
    }

    #[test]
    fn test_annotations() {
        let (name, annotations) = parse_annotations("cmd@readonly@destructive");
        assert_eq!(name, "cmd");
        assert_eq!(annotations, vec!["readonly", "destructive"]);
    }

    #[test]
    fn test_annotations_none() {
        let (name, annotations) = parse_annotations("cmd");
        assert_eq!(name, "cmd");
        assert!(annotations.is_empty());
    }

    #[test]
    fn test_annotations_in_entry() {
        let entry = parse_usage_entry("deploy@destructive");
        assert_eq!(entry.name, "deploy");
        assert_eq!(entry.annotations, vec!["destructive"]);
    }

    #[test]
    fn test_alias_with_explicit_func() {
        let entry = parse_usage_entry("cmd1:-fmt::args1");
        assert_eq!(entry.name, "cmd1");
        assert_eq!(entry.explicit_func, Some("fmt::args1".to_string()));
    }

    #[test]
    fn test_hidden_with_explicit_func() {
        let entry = parse_usage_entry("#internal:-secret::func");
        assert!(entry.hidden);
        assert_eq!(entry.name, "internal");
        assert_eq!(entry.explicit_func, Some("secret::func".to_string()));
    }

    // --- Additional annotation combination tests ---

    #[test]
    fn test_annotations_readonly_json() {
        let entry = parse_usage_entry("list@readonly@json");
        assert_eq!(entry.name, "list");
        assert_eq!(entry.annotations, vec!["readonly", "json"]);
        assert!(!entry.hidden);
    }

    #[test]
    fn test_annotations_destructive_idempotent() {
        let entry = parse_usage_entry("reset@destructive@idempotent");
        assert_eq!(entry.name, "reset");
        assert_eq!(entry.annotations, vec!["destructive", "idempotent"]);
    }

    #[test]
    fn test_explicit_mapping_with_name() {
        // cmd:-func::name
        let entry = parse_usage_entry("cmd:-func::name");
        assert_eq!(entry.name, "cmd");
        assert_eq!(entry.explicit_func, Some("func::name".to_string()));
        assert!(entry.aliases.contains(&"cmd".to_string()));
    }

    #[test]
    fn test_alias_with_explicit_func_mapping() {
        // cmd|alias:-func
        let entry = parse_usage_entry("cmd|alias:-func");
        assert_eq!(entry.name, "cmd");
        assert_eq!(entry.aliases, vec!["cmd", "alias"]);
        assert_eq!(entry.explicit_func, Some("func".to_string()));
    }

    #[test]
    fn test_hidden_entry() {
        let entry = parse_usage_entry("#hidden");
        assert!(entry.hidden);
        assert_eq!(entry.name, "hidden");
        assert!(entry.annotations.is_empty());
    }

    #[test]
    fn test_group_separator_dash() {
        let entry = parse_usage_entry("-");
        assert!(entry.is_group_separator);
        assert_eq!(entry.name, "-");
        assert!(entry.aliases.is_empty());
    }

    #[test]
    fn test_plain_command_no_extras() {
        let entry = parse_usage_entry("deploy");
        assert_eq!(entry.name, "deploy");
        assert_eq!(entry.aliases, vec!["deploy"]);
        assert!(!entry.hidden);
        assert!(!entry.is_group_separator);
        assert!(entry.explicit_func.is_none());
        assert!(entry.annotations.is_empty());
    }

    #[test]
    fn test_no_aliases() {
        let entry = parse_usage_entry("single");
        assert_eq!(entry.aliases.len(), 1);
        assert_eq!(entry.aliases[0], "single");
    }

    #[test]
    fn test_multiple_aliases() {
        let entry = parse_usage_entry("cmd|a|b|c");
        assert_eq!(entry.name, "cmd");
        assert_eq!(entry.aliases, vec!["cmd", "a", "b", "c"]);
    }

    #[test]
    fn test_hidden_with_annotations() {
        let entry = parse_usage_entry("#secret@readonly");
        assert!(entry.hidden);
        assert_eq!(entry.name, "secret");
        assert_eq!(entry.annotations, vec!["readonly"]);
    }

    #[test]
    fn test_annotations_openworld() {
        let entry = parse_usage_entry("fetch@openworld");
        assert_eq!(entry.annotations, vec!["openworld"]);
    }
}
