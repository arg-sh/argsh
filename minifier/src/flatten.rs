//! Line-level flattening for bash minification.
//!
//! Removes leading whitespace, trailing standalone semicolons,
//! and end-of-line comments from each line. Preserves heredoc
//! content verbatim.

use regex::Regex;
use std::sync::LazyLock;

static RE_LEADING_WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]+").unwrap());
static RE_TRAILING_SEMI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([^;]);$").unwrap());
static RE_HEREDOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<<-?\s*['"]?(\w+)['"]?"#).unwrap());

/// Strip an end-of-line comment, respecting single and double quotes.
///
/// Matches ` # ...` only when the `#` is outside any quoted string.
/// Returns the line unchanged if no strippable comment is found.
fn strip_eol_comment(line: &str) -> String {
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    for i in 0..len {
        let ch = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { '\0' };

        match ch {
            // Inside single quotes backslash is literal, so always toggle.
            // Outside single quotes, skip escaped quotes (prev == '\\').
            '\'' if !in_double && (in_single || prev != '\\') => in_single = !in_single,
            '"' if !in_single && prev != '\\' => in_double = !in_double,
            '#' if !in_single && !in_double => {
                // Must be preceded by whitespace and followed by whitespace (` # ...`)
                if i > 0
                    && chars[i - 1].is_whitespace()
                    && (i + 1 < len && chars[i + 1].is_whitespace())
                {
                    return chars[..i].iter().collect::<String>().trim_end().to_string()
                        + " ";
                }
            }
            _ => {}
        }
    }
    line.to_string()
}

/// Flatten a single line:
/// 1. Remove leading whitespace
/// 2. Remove trailing standalone semicolon (`;` at end, but not `;;`)
/// 3. Remove end-of-line comments (` # text` not inside quotes)
pub fn flatten_line(line: &str) -> String {
    let mut s = RE_LEADING_WS.replace(line, "").to_string();
    s = RE_TRAILING_SEMI.replace(&s, "$1").to_string();
    s = strip_eol_comment(&s);
    s
}

/// Flatten all lines, preserving heredoc content verbatim.
pub fn flatten_lines(lines: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut heredoc_delim: Option<String> = None;

    for line in lines {
        if let Some(ref delim) = heredoc_delim {
            result.push(line.clone());
            if line.trim() == delim.as_str() {
                heredoc_delim = None;
            }
            continue;
        }
        if let Some(cap) = RE_HEREDOC.captures(line) {
            heredoc_delim = Some(cap[1].to_string());
        }
        result.push(flatten_line(line));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_leading_ws() {
        assert_eq!(flatten_line("  echo hello"), "echo hello");
        assert_eq!(flatten_line("\t\tlocal x"), "local x");
    }

    #[test]
    fn removes_trailing_semi() {
        assert_eq!(flatten_line("echo hello;"), "echo hello");
    }

    #[test]
    fn preserves_double_semi() {
        assert_eq!(flatten_line("pattern);;"), "pattern);;");
    }

    #[test]
    fn removes_eol_comment() {
        assert_eq!(flatten_line("echo hello # comment"), "echo hello ");
    }

    #[test]
    fn preserves_hash_in_single_quotes() {
        assert_eq!(flatten_line("echo 'a # b'"), "echo 'a # b'");
    }

    #[test]
    fn preserves_hash_in_double_quotes() {
        assert_eq!(flatten_line(r#"echo "a # b""#), r#"echo "a # b""#);
    }

    #[test]
    fn strips_comment_after_quoted_string() {
        assert_eq!(
            flatten_line(r#"echo "hello" # comment"#),
            r#"echo "hello" "#
        );
    }

    #[test]
    fn strips_comment_after_single_quoted_string() {
        assert_eq!(flatten_line("echo 'hello' # comment"), "echo 'hello' ");
    }

    #[test]
    fn flatten_lines_works() {
        let lines: Vec<String> = vec![
            "  echo hello;".to_string(),
            "\tlocal x=1".to_string(),
            "echo world # comment".to_string(),
        ];
        let result = flatten_lines(&lines);
        assert_eq!(result[0], "echo hello");
        assert_eq!(result[1], "local x=1");
        assert_eq!(result[2], "echo world ");
    }

    #[test]
    fn backslash_inside_single_quotes_is_literal() {
        // 'hello\' is a complete single-quoted string (backslash is literal).
        // The ` # comment` after it should be stripped.
        assert_eq!(
            flatten_line(r"echo 'hello\' # comment"),
            r"echo 'hello\' "
        );
    }

    #[test]
    fn preserves_heredoc_whitespace() {
        let lines = vec![
            "  cat <<EOF".to_string(),
            "  indented content".to_string(),
            "  # looks like comment".to_string(),
            "EOF".to_string(),
            "  echo after".to_string(),
        ];
        let result = flatten_lines(&lines);
        assert_eq!(result[0], "cat <<EOF");
        assert_eq!(result[1], "  indented content");
        assert_eq!(result[2], "  # looks like comment");
        assert_eq!(result[3], "EOF");
        assert_eq!(result[4], "echo after");
    }
}
