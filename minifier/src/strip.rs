//! Line-level stripping for bash minification.
//!
//! Removes entire lines that are unnecessary in minified output:
//! comments, shebangs, blank lines, `import` calls/definitions,
//! and `set -euo pipefail`. Also handles mid-line shebangs caused
//! by `cat` concatenation of files without trailing newlines.

use regex::Regex;
use std::sync::LazyLock;

static RE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*#.*").unwrap());
static RE_BLANK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*$").unwrap());
/// Matches simple import calls: `import string`, `import fmt`, etc.
/// Only matches when followed by a simple word — avoids stripping array
/// elements like `    import import::clear)`.
static RE_IMPORT_CALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*import\s+\w+\s*$").unwrap());
/// Matches complete single-line import function definitions: `import() { ... }`.
/// Multi-line definitions (e.g. ` import() { \n body \n }`) are kept intact
/// to avoid orphaning their function body.
static RE_IMPORT_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*import\(\)\s*\{.+\}\s*$").unwrap());
static RE_SET_PIPEFAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*set -euo pipefail").unwrap());
/// Matches a shebang `#!/` that appears mid-line (after a non-newline char).
/// This happens when files are concatenated without trailing newlines,
/// producing lines like `}#!/usr/bin/env bash`.
static RE_MIDLINE_SHEBANG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(.)#!/").unwrap());
static RE_HEREDOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<<-?\s*['"]?(\w+)['"]?"#).unwrap());

/// Returns true if the line should be stripped (removed entirely).
///
/// Strips:
/// - Full-line comments (including shebangs — template provides its own)
/// - Blank/whitespace-only lines
/// - Lines matching `import ...` or `import(...)` at start
/// - `set -euo pipefail`
pub fn should_strip(line: &str) -> bool {
    RE_COMMENT.is_match(line)
        || RE_BLANK.is_match(line)
        || RE_IMPORT_CALL.is_match(line)
        || RE_IMPORT_DEF.is_match(line)
        || RE_SET_PIPEFAIL.is_match(line)
}

/// Find a heredoc delimiter (`<<DELIM`) outside of quoted strings.
///
/// Returns the delimiter name if found, or `None` if the `<<` only
/// appears inside quotes (e.g. `echo "not a <<EOF"`).
fn heredoc_outside_quotes(line: &str) -> Option<String> {
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    for i in 0..len.saturating_sub(1) {
        match chars[i] {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '<' if !in_single && !in_double && chars[i + 1] == '<' => {
                // Found << outside quotes — apply regex from this position
                let byte_pos = line.char_indices().nth(i).map(|(p, _)| p).unwrap_or(0);
                if let Some(cap) = RE_HEREDOC.captures(&line[byte_pos..]) {
                    return Some(cap[1].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Strip lines from input, returning only lines that survive.
///
/// Pre-processes input to split mid-line shebangs caused by file
/// concatenation (e.g. `}#!/usr/bin/env bash` → `}` + `#!/usr/bin/env bash`).
/// Preserves heredoc content verbatim (comments, imports, blank lines inside
/// heredocs are kept).
pub fn strip_lines(input: &str) -> Vec<String> {
    // Split mid-line shebangs onto their own lines so they get stripped
    let normalized = RE_MIDLINE_SHEBANG.replace_all(input, "$1\n#!/");
    let mut result = Vec::new();
    let mut heredoc_delim: Option<String> = None;

    for line in normalized.lines() {
        if let Some(ref delim) = heredoc_delim {
            result.push(line.to_string());
            if line.trim() == delim.as_str() {
                heredoc_delim = None;
            }
            continue;
        }
        // Check for heredoc start BEFORE stripping — only if << is outside quotes
        if let Some(delim) = heredoc_outside_quotes(line) {
            heredoc_delim = Some(delim);
        }
        if !should_strip(line) {
            result.push(line.to_string());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_shebang() {
        assert!(should_strip("#!/usr/bin/env bash"));
    }

    #[test]
    fn strips_comment() {
        assert!(should_strip("# this is a comment"));
        assert!(should_strip("  # indented comment"));
    }

    #[test]
    fn strips_blank() {
        assert!(should_strip(""));
        assert!(should_strip("   "));
        assert!(should_strip("\t\t"));
    }

    #[test]
    fn strips_import_call() {
        assert!(should_strip("import string"));
        assert!(should_strip("import fmt"));
        assert!(should_strip("  import is"));
    }

    #[test]
    fn strips_import_function() {
        assert!(should_strip("import() { something; }"));
    }

    #[test]
    fn keeps_import_in_array() {
        // Array element like `    import import::clear)` must not be stripped
        assert!(!should_strip("    import import::clear)"));
    }

    #[test]
    fn keeps_import_dynamic() {
        // Dynamic import like `import "${_lib}"` — kept (harmless at runtime)
        assert!(!should_strip(r#"    import "${_lib}""#));
    }

    #[test]
    fn strips_set_pipefail() {
        assert!(should_strip("set -euo pipefail"));
        assert!(should_strip("  set -euo pipefail"));
        assert!(should_strip("\tset -euo pipefail"));
    }

    #[test]
    fn keeps_code() {
        assert!(!should_strip("echo hello"));
        assert!(!should_strip("local x=1"));
    }

    #[test]
    fn splits_midline_shebang() {
        let input = "}#!/usr/bin/env bash\necho hello";
        let result = strip_lines(input);
        assert_eq!(result, vec!["}", "echo hello"]);
    }

    #[test]
    fn handles_normal_concatenation() {
        let input = "echo a\n#!/usr/bin/env bash\necho b";
        let result = strip_lines(input);
        assert_eq!(result, vec!["echo a", "echo b"]);
    }

    #[test]
    fn preserves_heredoc_content() {
        let input = "cat <<EOF\n# not a comment\nimport something\n  indented\nEOF\necho after";
        let result = strip_lines(input);
        assert_eq!(
            result,
            vec![
                "cat <<EOF",
                "# not a comment",
                "import something",
                "  indented",
                "EOF",
                "echo after",
            ]
        );
    }

    #[test]
    fn preserves_quoted_heredoc() {
        let input = "cat <<'MARKER'\n# comment\nMARKER\necho done";
        let result = strip_lines(input);
        assert_eq!(
            result,
            vec!["cat <<'MARKER'", "# comment", "MARKER", "echo done"]
        );
    }

    #[test]
    fn heredoc_inside_double_quotes_not_detected() {
        // `echo "not a <<EOF"` — the <<EOF is inside double quotes, not a real heredoc.
        let input = "echo \"not a <<EOF\"\n# this is a comment\necho after";
        let result = strip_lines(input);
        // The comment should be stripped (not preserved as heredoc content)
        assert_eq!(
            result,
            vec!["echo \"not a <<EOF\"", "echo after"]
        );
    }

    #[test]
    fn heredoc_inside_single_quotes_not_detected() {
        let input = "echo 'not a <<EOF'\n# this is a comment\necho after";
        let result = strip_lines(input);
        assert_eq!(
            result,
            vec!["echo 'not a <<EOF'", "echo after"]
        );
    }

    #[test]
    fn heredoc_outside_quotes_still_works() {
        // Real heredoc after a quoted string on the same line
        let input = "echo \"hello\" && cat <<EOF\n# heredoc content\nEOF\necho after";
        let result = strip_lines(input);
        assert_eq!(
            result,
            vec![
                "echo \"hello\" && cat <<EOF",
                "# heredoc content",
                "EOF",
                "echo after",
            ]
        );
    }
}
