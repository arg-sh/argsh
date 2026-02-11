//! Aggressive newline joining for minified bash output.
//!
//! Joins all lines into minimal output while preserving:
//! - Heredoc content (verbatim between `<<DELIM` ... `DELIM`)
//! - Case statement structure (`case ... in` requires newline after `in`)
//! - Multi-line arrays, quoted strings, and backslash continuations
//!
//! Post-processes to fix `then;`/`do;`/`else;` into `then `/`do `/`else `.

use crate::quote::QuoteTracker;
use regex::Regex;
use std::io::BufRead;
use std::sync::LazyLock;

static RE_HEREDOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<<-?\s*['"]?(\w+)['"]?\s*"#).unwrap());
static RE_CASE_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*case\b").unwrap());
static RE_ESAC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"esac(\s|\t|;|$)").unwrap());
static RE_CASE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^()]+\))(?:\s|\t)*(.*)").unwrap());
static RE_DOUBLE_SEMI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\s|\t)*;;(?:\s|\t|\n)*$").unwrap());
static RE_ARRAY_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"=\([^)]*$").unwrap());
static RE_ARRAY_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\)(?:\s|\t)*$").unwrap());
static RE_BLANK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\s|\t)*$").unwrap());
static RE_BACKSLASH_CONT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\$").unwrap());
static RE_TRAILING_OPERATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([|&{(]{1,2}|;)\s*$").unwrap());
static RE_LEADING_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\s|\t)*[)]").unwrap());
static RE_THEN_DO_ELSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\s|\t)*(then|do|else)\s*$").unwrap());
static RE_TRAILING_WS_NL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\s|\t)*$").unwrap());

/// Find a heredoc delimiter (`<<DELIM`) outside of quoted strings.
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

/// Aggressively join newlines in bash source, producing single-line output.
///
/// Preserves only heredocs (content between `<< DELIM` ... `DELIM`).
/// Case statements, arrays, and all other constructs are joined onto one line.
///
/// Improvement over Perl version: case statements are fully joined (valid bash),
/// and `then;`/`do;`/`else;` are fixed to use spaces.
pub fn join_newlines(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let reader = std::io::Cursor::new(input);
    let mut lines = reader.lines().peekable();

    while let Some(Ok(raw_line)) = lines.next() {
        // Strip leading whitespace — flatten already ran, but be robust
        let line = raw_line.trim_start().to_string();

        // Heredoc? Must preserve content verbatim
        if let Some(delim) = heredoc_outside_quotes(&line) {
            output.push_str(&RE_TRAILING_WS_NL.replace(&line, ""));
            output.push('\n');
            for inner in lines.by_ref() {
                let inner = inner.unwrap_or_default();
                output.push_str(&inner);
                output.push('\n');
                if inner.trim() == delim {
                    break;
                }
            }
            continue;
        }

        // Case statement? Join into single line (improvement over Perl)
        if RE_CASE_START.is_match(&line) {
            process_case(&line, &mut lines, &mut output);
            continue;
        }

        // Array spanning multiple lines: `=(` without closing `)`
        if RE_ARRAY_OPEN.is_match(&line) {
            output.push_str(&RE_TRAILING_WS_NL.replace(&line, ""));
            for inner in lines.by_ref() {
                let inner = inner.unwrap_or_default();
                output.push(' ');
                output.push_str(inner.trim());
                if RE_ARRAY_CLOSE.is_match(&inner) {
                    break;
                }
            }
            join_terminate(&mut output);
            continue;
        }

        // Blank line → skip
        if RE_BLANK.is_match(&line) {
            continue;
        }

        // Backslash continuation → remove backslash, no separator
        if RE_BACKSLASH_CONT.is_match(&line) {
            let trimmed = &line[..line.len() - 1];
            output.push_str(trimmed);
            continue;
        }

        // Trailing operator: `||`, `&&`, `|`, `{`, `(`, `;` → space instead of newline
        if RE_TRAILING_OPERATOR.is_match(&line) {
            output.push_str(&RE_TRAILING_WS_NL.replace(&line, ""));
            output.push(' ');
            continue;
        }

        // Line starts with `)` → just a closing paren
        if RE_LEADING_CLOSE.is_match(&line) {
            output.push_str(&RE_TRAILING_WS_NL.replace(&line, ""));
            join_terminate(&mut output);
            continue;
        }

        // `then`, `do`, `else` at end of line → space instead of newline
        if let Some(cap) = RE_THEN_DO_ELSE.captures(&line) {
            let keyword = &cap[1];
            let prefix = RE_THEN_DO_ELSE.replace(&line, "");
            output.push_str(&prefix);
            output.push_str(keyword);
            output.push(' ');
            continue;
        }

        // Check for open quotes (multi-line string)
        let (sq_open, dq_open) = QuoteTracker::line_has_open_quote(&line);
        if sq_open || dq_open {
            let quote_char = if sq_open { '\'' } else { '"' };
            output.push_str(&line);
            output.push(quote_char);
            output.push_str("$'\\n'");
            output.push(quote_char);
            for inner in lines.by_ref() {
                let inner = inner.unwrap_or_default();
                output.push_str(&inner);
                // Close when we find the matching quote character
                if inner.contains(quote_char) {
                    break;
                }
                output.push(quote_char);
                output.push_str("$'\\n'");
                output.push(quote_char);
            }
            join_terminate(&mut output);
            continue;
        }

        // Default: replace trailing whitespace with `;`
        output.push_str(&RE_TRAILING_WS_NL.replace(&line, ""));
        join_terminate(&mut output);
    }

    // Post-process: fix `then;` / `do;` / `else;` → `then ` / `do ` / `else `
    fix_keyword_semicolons(&output)
}

/// Process a case statement — join into a single line.
/// bash syntax: `case WORD in [pattern) body ;;]... esac`
/// Improvement over Perl: fully single-line output.
fn process_case<I>(first_line: &str, lines: &mut std::iter::Peekable<I>, output: &mut String)
where
    I: Iterator<Item = Result<String, std::io::Error>>,
{
    // Emit `case ... in ` (newline required by bash after `in`)
    let trimmed = RE_TRAILING_WS_NL.replace(first_line, "");
    output.push_str(&trimmed);
    output.push('\n');

    while let Some(Ok(line)) = lines.next() {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        // esac?
        if RE_ESAC.is_match(&line) {
            let trimmed = RE_TRAILING_WS_NL.replace(&line, "");
            output.push_str(&trimmed);
            output.push(';');
            break;
        }

        // Case pattern: `pattern)` followed by body until `;;`
        if let Some(cap) = RE_CASE_PATTERN.captures(&line) {
            let pattern_part = cap[1].to_string();
            let mut body = cap[2].to_string();

            // Collect body lines until `;;`
            if !body.contains(";;") {
                for inner in lines.by_ref() {
                    let inner = inner.unwrap_or_default();
                    let trimmed = inner.trim();
                    body.push('\n');
                    body.push_str(trimmed);
                    if trimmed.contains(";;") {
                        break;
                    }
                }
            }

            output.push_str(&pattern_part);
            // Strip trailing ;; from body
            let body = RE_DOUBLE_SEMI.replace(&body, "").to_string();
            // Recursively join the body
            let mut joined_body = join_newlines(&body);
            // Strip trailing `;` to avoid `;;;` when we append `;;`
            while joined_body.ends_with(';') {
                joined_body.pop();
            }
            if !joined_body.is_empty() {
                output.push_str(&joined_body);
            }
            // `;;` + newline (newline before next pattern is required)
            output.push_str(";;\n");
        }
    }
}

fn join_terminate(output: &mut String) {
    output.push(';');
}

static RE_KEYWORD_SEMI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(then|do|else);").unwrap());

/// Fix `then;` → `then `, `do;` → `do `, `else;` → `else `.
fn fix_keyword_semicolons(input: &str) -> String {
    RE_KEYWORD_SEMI.replace_all(input, "$1 ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_simple() {
        let input = "echo a\necho b\n";
        let result = join_newlines(input);
        assert!(result.contains("echo a;echo b"));
    }

    #[test]
    fn preserves_heredoc() {
        let input = "cat <<EOF\nhello world\nEOF\necho done\n";
        let result = join_newlines(input);
        assert!(result.contains("hello world\n"));
    }

    #[test]
    fn then_gets_space() {
        let input = "if true; then\n  echo yes\nfi\n";
        let result = join_newlines(input);
        assert!(!result.contains("then;"), "Got: {result}");
        assert!(result.contains("then "), "Got: {result}");
    }

    #[test]
    fn do_gets_space() {
        let input = "for x in a b; do\n  echo $x\ndone\n";
        let result = join_newlines(input);
        assert!(!result.contains("do;"), "Got: {result}");
    }

    #[test]
    fn case_statement() {
        let input = "case \"$x\" in\n  a)\n    echo a\n    ;;\n  b)\n    echo b\n    ;;\nesac\n";
        let result = join_newlines(input);
        assert!(!result.contains("then;"), "Got: {result}");
        assert!(result.contains("esac"), "Got: {result}");
    }

    #[test]
    fn operator_continuation() {
        let input = "echo a ||\n  echo b\n";
        let result = join_newlines(input);
        assert!(result.contains("echo a || echo b"), "Got: {result}");
    }

    #[test]
    fn backslash_continuation() {
        let input = "echo hello \\\nworld\n";
        let result = join_newlines(input);
        assert!(result.contains("echo hello world"), "Got: {result}");
    }

    #[test]
    fn array_spanning_multiline() {
        let input = "arr=(\n  one\n  two\n  three\n)\n";
        let result = join_newlines(input);
        assert!(result.contains("arr=( one two three )"), "Got: {result}");
        assert!(!result.contains('\n'), "Should be single line, got: {result}");
    }

    #[test]
    fn leading_close_paren() {
        let input = "func(\n)\necho done\n";
        let result = join_newlines(input);
        assert!(result.contains(")"), "Got: {result}");
    }

    #[test]
    fn multiline_single_quote() {
        let input = "echo 'hello\nworld'\n";
        let result = join_newlines(input);
        assert!(result.contains("hello"), "Got: {result}");
        assert!(result.contains("world"), "Got: {result}");
    }

    #[test]
    fn multiline_double_quote() {
        let input = "echo \"hello\nworld\"\n";
        let result = join_newlines(input);
        assert!(result.contains("hello"), "Got: {result}");
        assert!(result.contains("world"), "Got: {result}");
    }

    #[test]
    fn case_empty_body() {
        let input = "case \"$1\" in\n  disabled)\n    ;;\n  active)\n    echo yes\n    ;;\nesac\n";
        let result = join_newlines(input);
        assert!(result.contains("disabled);;"), "Got: {result}");
        assert!(result.contains("active)echo yes;;"), "Got: {result}");
    }

    #[test]
    fn case_inline_body() {
        let input = "case \"$x\" in\n  a) echo a ;;\n  b) echo b ;;\nesac\n";
        let result = join_newlines(input);
        assert!(result.contains("a)echo a;;"), "Got: {result}");
    }

    #[test]
    fn blank_lines_skipped() {
        let input = "echo a\n\n\necho b\n";
        let result = join_newlines(input);
        assert_eq!(result, "echo a;echo b;");
    }

    #[test]
    fn else_gets_space() {
        let input = "if true; then\n  echo a\nelse\n  echo b\nfi\n";
        let result = join_newlines(input);
        assert!(!result.contains("else;"), "Got: {result}");
        assert!(result.contains("else "), "Got: {result}");
    }

    #[test]
    fn and_continuation() {
        let input = "cmd1 &&\ncmd2\n";
        let result = join_newlines(input);
        assert!(result.contains("cmd1 && cmd2"), "Got: {result}");
    }

    #[test]
    fn pipe_continuation() {
        let input = "cmd1 |\ncmd2\n";
        let result = join_newlines(input);
        assert!(result.contains("cmd1 | cmd2"), "Got: {result}");
    }

    #[test]
    fn brace_open_continuation() {
        let input = "func() {\necho body\n}\n";
        let result = join_newlines(input);
        assert!(result.contains("func() { echo body"), "Got: {result}");
    }

    #[test]
    fn multiline_double_quote_three_lines() {
        // 3+ lines: the middle line has no quotes, so the continuation loop
        // must NOT break early — only break when we see the closing `"`.
        let input = "echo \"hello\nmiddle\nworld\"\n";
        let result = join_newlines(input);
        assert!(result.contains("hello"), "Got: {result}");
        assert!(result.contains("middle"), "Got: {result}");
        assert!(result.contains("world"), "Got: {result}");
        // The middle line must be joined with $'\n' concatenation, not a `;`
        assert!(!result.contains("middle;"), "middle should not be a separate statement, got: {result}");
    }

    #[test]
    fn multiline_single_quote_three_lines() {
        let input = "echo 'hello\nmiddle\nworld'\n";
        let result = join_newlines(input);
        assert!(result.contains("hello"), "Got: {result}");
        assert!(result.contains("middle"), "Got: {result}");
        assert!(result.contains("world"), "Got: {result}");
        assert!(!result.contains("middle;"), "middle should not be a separate statement, got: {result}");
    }

    #[test]
    fn case_with_blank_lines() {
        // Blank lines between patterns should be skipped
        let input = "case \"$1\" in\n\n  a)\n    echo a\n    ;;\n\n  b)\n    echo b\n    ;;\n\nesac\n";
        let result = join_newlines(input);
        assert!(result.contains("esac"), "Got: {result}");
    }

    #[test]
    fn case_multiline_body() {
        // Body that spans multiple lines before ;;
        let input = "case \"$1\" in\n  start)\n    echo one\n    echo two\n    ;;\nesac\n";
        let result = join_newlines(input);
        assert!(result.contains("start)echo one;echo two;;"), "Got: {result}");
    }

    #[test]
    fn heredoc_after_single_quoted_string() {
        // Exercises the single-quote toggle (line 51) and heredoc capture (line 57)
        // in join's heredoc_outside_quotes. The `<<EOF` comes after a single-quoted string.
        let input = "echo 'hi' && cat <<EOF\nheredoc content\nEOF\necho after\n";
        let result = join_newlines(input);
        assert!(result.contains("heredoc content\n"), "Got: {result}");
        assert!(result.contains("EOF"), "Got: {result}");
    }

    #[test]
    fn case_without_esac_eof() {
        // Case statement where input ends without `esac`.
        let input = "case \"$1\" in\n  a)\n    echo a\n    ;;\n";
        let result = join_newlines(input);
        assert!(result.contains("a)echo a;;"), "Got: {result}");
    }

    #[test]
    fn heredoc_double_lt_no_delimiter() {
        // `<<` found outside quotes but no valid \w+ delimiter follows.
        // Exercises the None path (line 57) in join's heredoc_outside_quotes.
        let input = "echo << ;\necho after\n";
        let result = join_newlines(input);
        // No heredoc detected — both lines get joined normally
        assert!(result.contains("echo <<"), "Got: {result}");
        assert!(result.contains("echo after"), "Got: {result}");
    }

    #[test]
    fn case_with_non_pattern_line() {
        // A line inside case that's not esac and not a pattern (no closing `)`)
        // exercises the else branch of the `if let Some(cap)` (line 243).
        let input = "case \"$1\" in\n  not_a_pattern\n  a)\n    echo a\n    ;;\nesac\n";
        let result = join_newlines(input);
        assert!(result.contains("esac"), "Got: {result}");
    }
}
