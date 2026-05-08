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

/// Split mid-line shebangs (`}#!/...`) onto their own lines, but only when
/// the `#!/` appears outside of quoted strings. This prevents mangling
/// patterns like `'^#!/.*(ba)?sh'` inside single-quoted regexes (issue #76).
fn split_midline_shebangs(input: &str) -> String {
    // Work line-by-line to handle heredocs correctly — heredoc content is
    // passed through without quote tracking (quotes inside are literal).
    let mut result = String::with_capacity(input.len());
    let mut heredoc_delim: Option<String> = None;

    for line in input.split_inclusive('\n') {
        // Inside heredoc — pass through verbatim
        if let Some(ref delim) = heredoc_delim {
            result.push_str(line);
            if line.trim_end_matches('\n').trim() == delim.as_str() {
                heredoc_delim = None;
            }
            continue;
        }

        // Check for heredoc start on this line (reuse the module-level function)
        if let Some(delim) = heredoc_outside_quotes(line.trim_end_matches('\n')) {
            heredoc_delim = Some(delim);
        }

        // Process this line character by character for quote-aware shebang splitting
        split_line_shebangs(line, &mut result);
    }
    result
}

/// Process a single line for mid-line shebang splitting, respecting quotes.
fn split_line_shebangs(line: &str, result: &mut String) {
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    let mut i = 0;
    while i < len {
        let ch = chars[i];

        // Newline — just pass through
        if ch == '\n' {
            in_comment = false;
            result.push(ch);
            i += 1;
            continue;
        }

        // Inside a comment, skip quote tracking (e.g. "# don't")
        if in_comment {
            result.push(ch);
            i += 1;
            continue;
        }

        // Detect comment start outside quotes — # only starts a comment at a
        // word boundary (after whitespace or BOL), not mid-word like foo#bar
        // or in parameter expansions like ${#var}
        let at_word_start = i == 0 || matches!(chars[i - 1], ' ' | '\t' | ';' | '\n');
        if !in_single && !in_double && ch == '#' && at_word_start
            && !(i + 1 < len && chars[i + 1] == '!' && i + 2 < len && chars[i + 2] == '/')
        {
            in_comment = true;
            result.push(ch);
            i += 1;
            continue;
        }

        // Count preceding backslashes for escape detection
        let is_escaped = if !in_single {
            let mut b = 0;
            let mut j = i;
            while j > 0 && chars[j - 1] == '\\' { b += 1; j -= 1; }
            b % 2 == 1
        } else {
            false // backslash is literal inside single quotes
        };

        // Track quotes (backslash is literal inside single quotes)
        if ch == '\'' && !in_double && (in_single || !is_escaped) {
            in_single = !in_single;
        } else if ch == '"' && !in_single && !is_escaped {
            in_double = !in_double;
        }

        // Detect #!/ outside quotes — split onto new line
        if !in_single && !in_double
            && ch == '#'
            && i + 1 < len && chars[i + 1] == '!'
            && i + 2 < len && chars[i + 2] == '/'
            && i > 0 && chars[i - 1] != '\n'
        {
            result.push('\n');
        }

        result.push(ch);
        i += 1;
    }
}

/// Strip lines from input, returning only lines that survive.
///
/// Pre-processes input to split mid-line shebangs caused by file
/// concatenation (e.g. `}#!/usr/bin/env bash` → `}` + `#!/usr/bin/env bash`).
/// Preserves heredoc content verbatim (comments, imports, blank lines inside
/// heredocs are kept).
pub fn strip_lines(input: &str) -> Vec<String> {
    // Split mid-line shebangs onto their own lines so they get stripped.
    // Only replace outside of quoted strings to avoid mangling patterns like '^#!/'
    let normalized = split_midline_shebangs(input);
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
    fn shebang_inside_single_quotes_not_split() {
        // Issue #76: '^#!/' inside single-quoted regex must not be treated as mid-line shebang
        let input = "grep -qE '^#!/.*(ba)?sh([[:space:]]|$)|^#!/.*argsh'\necho after";
        let result = strip_lines(input);
        assert_eq!(
            result,
            vec![
                "grep -qE '^#!/.*(ba)?sh([[:space:]]|$)|^#!/.*argsh'",
                "echo after",
            ]
        );
    }

    #[test]
    fn shebang_inside_double_quotes_not_split() {
        let input = "echo \"^#!/bin/bash\"\necho after";
        let result = strip_lines(input);
        assert_eq!(
            result,
            vec!["echo \"^#!/bin/bash\"", "echo after"]
        );
    }

    #[test]
    fn comment_apostrophe_does_not_break_shebang_split() {
        // Apostrophe in a comment must not toggle quote state
        let input = "# don't break\n}#!/usr/bin/env bash\necho hello";
        let result = strip_lines(input);
        // Comment stripped, shebang split and stripped, code kept
        assert_eq!(result, vec!["}", "echo hello"]);
    }

    #[test]
    fn hash_in_parameter_expansion_not_comment() {
        // ${#var} is string length, not a comment
        let input = "echo ${#arr[@]}\n}#!/usr/bin/env bash\necho after";
        let result = strip_lines(input);
        assert_eq!(result, vec!["echo ${#arr[@]}", "}", "echo after"]);
    }

    #[test]
    fn escaped_single_quote_outside_quotes() {
        // \' outside quotes is a literal quote, not a string delimiter
        let input = "echo \\'hello\\'\n}#!/usr/bin/env bash\necho after";
        let result = strip_lines(input);
        assert_eq!(result, vec!["echo \\'hello\\'", "}", "echo after"]);
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

    #[test]
    fn heredoc_double_lt_no_delimiter() {
        // `<<` found outside quotes but no valid \w+ delimiter follows.
        // Exercises the None path (line 67) in strip's heredoc_outside_quotes.
        let input = "echo << ;\n# this is a comment\necho after";
        let result = strip_lines(input);
        // No heredoc detected, so the comment line IS stripped normally
        assert_eq!(result, vec!["echo << ;", "echo after"]);
    }

    #[test]
    fn heredoc_after_single_quoted_string() {
        // Exercises the single-quote toggle and heredoc capture (line 67)
        // in strip's heredoc_outside_quotes.
        let input = "echo 'hi' && cat <<EOF\n# heredoc content\nEOF\necho after";
        let result = strip_lines(input);
        assert_eq!(
            result,
            vec![
                "echo 'hi' && cat <<EOF",
                "# heredoc content",
                "EOF",
                "echo after",
            ]
        );
    }
}
