//! Line-level flattening for bash minification.
//!
//! Removes leading whitespace, trailing standalone semicolons,
//! and end-of-line comments from each line.

use regex::Regex;
use std::sync::LazyLock;

static RE_LEADING_WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]+").unwrap());
static RE_TRAILING_SEMI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([^;]);$").unwrap());
static RE_EOL_COMMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\s+#\s+[^"]+$"#).unwrap());

/// Flatten a single line:
/// 1. Remove leading whitespace
/// 2. Remove trailing standalone semicolon (`;` at end, but not `;;`)
/// 3. Remove end-of-line comments (heuristic: ` # text` not inside quotes)
pub fn flatten_line(line: &str) -> String {
    let mut s = RE_LEADING_WS.replace(line, "").to_string();
    s = RE_TRAILING_SEMI.replace(&s, "$1").to_string();
    s = RE_EOL_COMMENT.replace(&s, " ").to_string();
    s
}

/// Flatten all lines.
pub fn flatten_lines(lines: &[String]) -> Vec<String> {
    lines.iter().map(|l| flatten_line(l)).collect()
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
}
