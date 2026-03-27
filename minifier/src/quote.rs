//! Quote-tracking utilities for bash parsing.
//!
//! Determines whether a line has unbalanced (open) single or double quotes,
//! used by the join phase to detect multi-line strings.

/// Stateless quote analysis.
pub struct QuoteTracker;

impl QuoteTracker {
    /// Count unbalanced quotes in a line, respecting escapes, command substitution
    /// nesting (`$(...)` and backticks), so that quotes inside substitutions don't
    /// leak to the outer level.
    /// Returns (single_open, double_open) — true if the outermost level has an
    /// unmatched quote of that type.
    pub fn line_has_open_quote(line: &str) -> (bool, bool) {
        let mut in_single = false;
        let mut in_double = false;
        let mut subst_depth: usize = 0; // $() nesting depth
        let mut backtick_depth: usize = 0; // backtick substitution depth
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        let mut i = 0;
        while i < len {
            let ch = chars[i];

            // Inside single quotes at the current nesting level:
            // Everything is literal except the closing single quote.
            // No escapes, no command substitution detection.
            if in_single {
                if ch == '\'' {
                    in_single = false;
                }
                i += 1;
                continue;
            }

            // Check for escaped characters (outside single quotes)
            let is_escaped = {
                let mut backslashes = 0;
                let mut j = i;
                while j > 0 && chars[j - 1] == '\\' {
                    backslashes += 1;
                    j -= 1;
                }
                backslashes % 2 == 1
            };

            if is_escaped {
                i += 1;
                continue;
            }

            // Detect $( for command substitution
            if ch == '$' && i + 1 < len && chars[i + 1] == '(' {
                subst_depth += 1;
                i += 2; // skip $(
                continue;
            }

            // Detect ) closing command substitution
            if ch == ')' && subst_depth > 0 {
                subst_depth -= 1;
                i += 1;
                continue;
            }

            // Detect backtick command substitution (backticks don't nest)
            if ch == '`' {
                if backtick_depth > 0 {
                    backtick_depth -= 1;
                } else {
                    backtick_depth += 1;
                }
                i += 1;
                continue;
            }

            // Only track quotes at the outermost level (no active substitution)
            if subst_depth == 0 && backtick_depth == 0 {
                match ch {
                    '\'' if !in_double => {
                        in_single = true;
                    }
                    '"' => {
                        in_double = !in_double;
                    }
                    _ => {}
                }
            }
            // Inside command substitution: quotes exist but they're at a deeper
            // level. We don't track them because we only care about whether the
            // LINE (outermost level) has an open quote.

            i += 1;
        }

        (in_single, in_double)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_quotes() {
        let (s, d) = QuoteTracker::line_has_open_quote("echo hello");
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn balanced_double() {
        let (s, d) = QuoteTracker::line_has_open_quote(r#"echo "hello world""#);
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn open_single() {
        let (s, d) = QuoteTracker::line_has_open_quote("echo 'hello");
        assert!(s);
        assert!(!d);
    }

    #[test]
    fn single_inside_double() {
        let (s, d) = QuoteTracker::line_has_open_quote(r#"echo "it's fine""#);
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn backslash_inside_single_quotes_is_literal() {
        // In bash, backslash has no special meaning inside single quotes.
        // So 'hello\' is: open-quote, h,e,l,l,o,\, close-quote → balanced.
        let (s, d) = QuoteTracker::line_has_open_quote(r"echo 'hello\'");
        assert!(!s, "backslash is literal inside single quotes");
        assert!(!d);
    }

    #[test]
    fn escaped_single_quote_outside_quotes() {
        // Outside quotes, \' is an escaped quote — NOT a string delimiter.
        let (s, d) = QuoteTracker::line_has_open_quote(r"echo \'hello");
        assert!(!s, "escaped single quote should not open a string");
        assert!(!d);
    }

    #[test]
    fn double_backslash_before_double_quote() {
        // `echo "test\\\\"` — the \\\\ is two escaped backslashes (literal \\),
        // so the final `"` is a real closing quote. The string is balanced.
        let (s, d) = QuoteTracker::line_has_open_quote(r#"echo "test\\""#);
        assert!(!s);
        assert!(!d, "even number of backslashes means quote is real");
    }

    #[test]
    fn triple_backslash_before_double_quote() {
        // `echo "test\\\"` — three backslashes: \\\\ = literal \, then \" = escaped quote.
        // The quote is escaped, so the string is still open.
        let (s, d) = QuoteTracker::line_has_open_quote(r#"echo "test\\\""#);
        assert!(!s);
        assert!(d, "odd number of backslashes means quote is escaped, string is open");
    }

    #[test]
    fn double_backslash_before_single_quote() {
        // Outside single quotes, `\\` is an escaped backslash, so `'` after it is real.
        let (s, d) = QuoteTracker::line_has_open_quote(r"echo \\'hello'");
        assert!(!s, "even backslashes, both single quotes are real — balanced");
        assert!(!d);
    }

    #[test]
    fn command_substitution_quotes_isolated() {
        // Quotes inside $() don't affect outer quote state
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"x="$(echo 'hello' "world")""#
        );
        assert!(!s, "single quotes inside $() should not leak");
        assert!(!d, "double quotes balanced at outer level");
    }

    #[test]
    fn nested_command_substitution() {
        // $() inside $()
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"x="$(echo "$(cat 'file')")""#
        );
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn grep_pattern_in_command_substitution() {
        // The exact pattern that triggered the bug
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"_pct="$(grep -o '"percent_covered": "[^"]*"' "${_cov_file}" | tail -1 | grep -o '[0-9.]*')" || _pct="?""#
        );
        assert!(!s, "quotes inside $() grep args should not leak to outer level");
        assert!(!d, "outer double quotes should be balanced");
    }

    #[test]
    fn or_true_after_command_substitution() {
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"_pct="$(grep -o 'foo' file)" || true"#
        );
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn or_assignment_after_command_substitution() {
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"_pct="$(grep -o 'foo' file)" || _pct="?""#
        );
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn backtick_substitution_quotes_isolated() {
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"x="`echo 'hello'`""#
        );
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn open_quote_after_closed_substitution() {
        // Genuinely open quote AFTER a $() — should be detected
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"x="$(echo done)" && echo 'open"#
        );
        assert!(s, "single quote after $() should be detected as open");
        assert!(!d);
    }

    #[test]
    fn open_double_quote_with_substitution() {
        // Outer double quote opened but not closed
        let (s, d) = QuoteTracker::line_has_open_quote(
            r#"echo "hello $(echo 'world')"#
        );
        assert!(!s);
        assert!(d, "outer double quote is open");
    }
}
