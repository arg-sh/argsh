//! Variable discovery from bash source code.
//!
//! Scans bash scripts for variable declarations and assignments across 8
//! patterns: assignment, `local`, `read`, `for`, array access, and arithmetic
//! pre/post increment/decrement.
//!
//! Supports the `# obfus ignore variable` annotation — when this comment
//! appears on a line, the **next** line's variable declarations are skipped
//! during discovery, preventing them from being obfuscated.

use anyhow::Result;
use regex::Regex;
use std::sync::LazyLock;
static RE_ASSIGNMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*([a-z][a-z0-9_]*)=").unwrap());
static RE_LOCAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[ \t]+)local\s(?:[ \t]|-\w)*([a-z][a-z0-9_]*)(?:=|\s|$)").unwrap()
});
static RE_LOCAL_HAS_DECLARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[ \t]+)declare\s").unwrap());
static RE_READ: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"read\s+(?:-\w\s+)*([a-z][a-z0-9_]*)").unwrap());
static RE_FOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*for\s+([a-z][a-z0-9_]*)\s").unwrap());
static RE_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*([a-z][a-z0-9_]*)\[.+\]=").unwrap());
static RE_PRE_INCR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*\(\(\s*[-+]{2}([a-z][a-z0-9_]*)\s*\)\)").unwrap());
static RE_POST_INCR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*\({2}\s*([a-z][a-z0-9_]*)[-+]{2}\s*\){2}").unwrap());
static RE_IGNORE_ANNOTATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*# obfus ignore variable").unwrap());
static RE_COMMENT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*#").unwrap());
static RE_BLANK_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*$").unwrap());
static RE_LOCAL_STRIP_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^.*local\s(?:[ \t]|-\w)*").unwrap());
static RE_PARENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[({].*?[)}]").unwrap());
static RE_QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:"[^"]*"|'[^']*')"#).unwrap());
static RE_EQUALS_VAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"=\S*").unwrap());
static RE_VAR_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").unwrap());

/// Split a line on semicolons, but only those outside single and double quotes.
fn split_outside_quotes(line: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut in_single = false;
    let mut in_double = false;
    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ';' if !in_single && !in_double => {
                segments.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(&line[start..]);
    segments
}

/// Discover all local variable names from bash source.
///
/// Follows the same logic as the Perl `parse_vars_from_file`:
/// - Assignment: `var=...`
/// - Local: `local [-x] var ...` (but not if `declare` on same line)
/// - Read: `read [-s] var`
/// - For: `for var in ...`
/// - Array: `var[...]=`
/// - Arithmetic: `(( ++var ))` / `(( var++ ))`
///
/// Skips `IFS`, comment lines, blank lines, and lines after `# obfus ignore variable`.
pub fn discover_variables(source: &str, ignore_patterns: &[Regex]) -> Vec<String> {
    let mut vars = std::collections::HashSet::new();
    let mut skip_next = 0usize;

    for line in source.lines() {
        // Check for ignore annotation
        if RE_IGNORE_ANNOTATION.is_match(line) {
            skip_next += 1;
            continue;
        }
        if skip_next > 0 {
            skip_next -= 1;
            continue;
        }
        // Skip comments and blanks
        if RE_COMMENT_LINE.is_match(line) || RE_BLANK_LINE.is_match(line) {
            continue;
        }

        // Split on semicolons for multi-statement lines (respecting quotes)
        for segment in split_outside_quotes(line) {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            discover_from_segment(segment, &mut vars);
        }
    }

    // Remove ignored variables
    let mut result: Vec<String> = vars
        .into_iter()
        .filter(|v| {
            !ignore_patterns.iter().any(|re| re.is_match(v))
        })
        .collect();

    // Sort by length descending (longer names first for safe replacement)
    result.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    result
}

fn discover_from_segment(segment: &str, vars: &mut std::collections::HashSet<String>) {
    // Assignment: var=
    if let Some(cap) = RE_ASSIGNMENT.captures(segment) {
        let name = &cap[1];
        if name != "IFS" {
            vars.insert(name.to_string());
        }
        return;
    }

    // Local (but not declare on same line)
    if !RE_LOCAL_HAS_DECLARE.is_match(segment)
        && RE_LOCAL.is_match(segment)
    {
        // Extract all variable names from local declaration
        let stripped = RE_LOCAL_STRIP_PREFIX.replace(segment, "").to_string();
        let stripped = RE_PARENS.replace_all(&stripped, "").to_string();
        let stripped = RE_QUOTED.replace_all(&stripped, "").to_string();
        let stripped = RE_EQUALS_VAL.replace_all(&stripped, "").to_string();
        for word in stripped.split_whitespace() {
            if RE_VAR_NAME.is_match(word) {
                vars.insert(word.to_string());
            }
        }
        return;
    }

    // Read statement
    if let Some(cap) = RE_READ.captures(segment) {
        vars.insert(cap[1].to_string());
        return;
    }

    // For loop
    if let Some(cap) = RE_FOR.captures(segment) {
        vars.insert(cap[1].to_string());
        return;
    }

    // Array access
    if let Some(cap) = RE_ARRAY.captures(segment) {
        vars.insert(cap[1].to_string());
        return;
    }

    // Pre-increment/decrement
    if let Some(cap) = RE_PRE_INCR.captures(segment) {
        vars.insert(cap[1].to_string());
        return;
    }

    // Post-increment/decrement
    if let Some(cap) = RE_POST_INCR.captures(segment) {
        vars.insert(cap[1].to_string());
    }
}

/// Parse the ignore pattern string (comma-separated regexes) into compiled Regex objects.
///
/// Each pattern is anchored with `^...$` so it matches the full variable name,
/// not just a substring.  E.g. `usage` only matches `usage`, not `usage_count`.
pub fn parse_ignore_patterns(pattern: &str) -> Result<Vec<Regex>> {
    if pattern == "*" {
        // Special: ignore ALL variables
        return Ok(vec![Regex::new(".*")?]);
    }
    pattern
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| {
            // If user already supplied anchors, use pattern as-is
            if s.starts_with('^') || s.ends_with('$') {
                Regex::new(s).map_err(Into::into)
            } else {
                Regex::new(&format!("^(?:{s})$")).map_err(Into::into)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_assignment() {
        let vars = discover_variables("foo=bar\n", &[]);
        assert!(vars.contains(&"foo".to_string()));
    }

    #[test]
    fn discovers_local() {
        let vars = discover_variables("local name value\n", &[]);
        assert!(vars.contains(&"name".to_string()));
        assert!(vars.contains(&"value".to_string()));
    }

    #[test]
    fn skips_ifs() {
        let vars = discover_variables("IFS=:\n", &[]);
        assert!(!vars.contains(&"IFS".to_string()));
    }

    #[test]
    fn skips_ignored() {
        let vars = discover_variables("# obfus ignore variable\nfoo=1\nbar=2\n", &[]);
        assert!(!vars.contains(&"foo".to_string()));
        assert!(vars.contains(&"bar".to_string()));
    }

    #[test]
    fn respects_ignore_patterns() {
        let pats = parse_ignore_patterns("usage,args").unwrap();
        let vars = discover_variables("usage=1\nargs=2\nfoo=3\n", &pats);
        assert!(!vars.contains(&"usage".to_string()));
        assert!(!vars.contains(&"args".to_string()));
        assert!(vars.contains(&"foo".to_string()));
    }

    #[test]
    fn sorted_by_length_desc() {
        let vars = discover_variables("ab=1\nabcde=2\nabc=3\n", &[]);
        assert_eq!(vars[0], "abcde");
        assert_eq!(vars[1], "abc");
        assert_eq!(vars[2], "ab");
    }

    #[test]
    fn discovers_read() {
        let vars = discover_variables("read -r line\n", &[]);
        assert!(vars.contains(&"line".to_string()));
    }

    #[test]
    fn discovers_read_with_flags() {
        let vars = discover_variables("read -s -r passwd\n", &[]);
        assert!(vars.contains(&"passwd".to_string()));
    }

    #[test]
    fn discovers_for_loop() {
        let vars = discover_variables("for item in a b c; do\n  echo $item\ndone\n", &[]);
        assert!(vars.contains(&"item".to_string()));
    }

    #[test]
    fn discovers_array_access() {
        let vars = discover_variables("arr[0]=value\n", &[]);
        assert!(vars.contains(&"arr".to_string()));
    }

    #[test]
    fn discovers_pre_increment() {
        let vars = discover_variables("(( ++counter ))\n", &[]);
        assert!(vars.contains(&"counter".to_string()));
    }

    #[test]
    fn discovers_post_increment() {
        let vars = discover_variables("(( counter++ ))\n", &[]);
        assert!(vars.contains(&"counter".to_string()));
    }

    #[test]
    fn discovers_pre_decrement() {
        let vars = discover_variables("(( --idx ))\n", &[]);
        assert!(vars.contains(&"idx".to_string()));
    }

    #[test]
    fn discovers_post_decrement() {
        let vars = discover_variables("(( idx-- ))\n", &[]);
        assert!(vars.contains(&"idx".to_string()));
    }

    #[test]
    fn parse_ignore_star() {
        let pats = parse_ignore_patterns("*").unwrap();
        assert_eq!(pats.len(), 1);
        assert!(pats[0].is_match("anything"));
    }

    #[test]
    fn parse_ignore_empty_segment() {
        let pats = parse_ignore_patterns("foo,,bar").unwrap();
        assert_eq!(pats.len(), 2);
    }

    #[test]
    fn parse_ignore_anchored_regex() {
        let pats = parse_ignore_patterns("^u").unwrap();
        assert_eq!(pats.len(), 1);
        assert!(pats[0].is_match("usage"));
        assert!(pats[0].is_match("user"));
        assert!(!pats[0].is_match("args"));
    }

    #[test]
    fn skips_declare_local() {
        // local with declare on same line should be skipped
        let vars = discover_variables("declare local foo\n", &[]);
        assert!(!vars.contains(&"foo".to_string()));
    }

    #[test]
    fn semicolon_inside_quotes_not_split() {
        // `local msg="hello; world"; x=1` — semicolon inside quotes should not split
        let vars = discover_variables("local msg=\"hello; world\"\nx=1\n", &[]);
        assert!(vars.contains(&"msg".to_string()), "msg should be discovered");
        assert!(vars.contains(&"x".to_string()), "x should be discovered");
        // Should NOT discover "world" as a variable (from malformed split)
        assert!(!vars.contains(&"world".to_string()), "world should NOT be discovered");
    }

    #[test]
    fn split_outside_quotes_basic() {
        let result = split_outside_quotes("a=1; b=2");
        assert_eq!(result, vec!["a=1", " b=2"]);
    }

    #[test]
    fn split_outside_quotes_with_quoted_semicolons() {
        let result = split_outside_quotes(r#"msg="hello; world"; x=1"#);
        assert_eq!(result, vec![r#"msg="hello; world""#, " x=1"]);
    }
}
