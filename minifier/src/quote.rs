//! Quote-tracking utilities for bash parsing.
//!
//! Determines whether a line has unbalanced (open) single or double quotes,
//! used by the join phase to detect multi-line strings.
//!
//! ## Known limitations
//!
//! - **Multi-line strings inside `$()`**: The stack-based tracker isolates
//!   quotes inside `$()` from the outer level — they don't leak outward.
//!   However, if a multi-line quoted string starts inside a `$()` on one line
//!   (e.g. `x=$(echo "hello` on line 1, `world")` on line 2), the open quote
//!   is **not detected** by this line-based tracker. The join phase may
//!   incorrectly terminate such lines with `;` instead of preserving the
//!   newline. This is inherent to the line-based design of the minifier.
//!
//! - **Multi-line strings inside backticks**: Same as `$()` above — open
//!   quotes inside backtick substitutions are not detected. Backticks are
//!   deprecated in favor of `$()` and rarely contain multi-line strings.
//!
//! - **Escape detection is O(n) per character** (backward scan for consecutive
//!   backslashes). Worst case is O(n²) per line for pathological inputs with
//!   long backslash sequences. In practice this is bounded by line length which
//!   is small after minification (typically one logical statement per line).

/// Stateless quote analysis.
pub struct QuoteTracker;

/// Per-nesting-level quote state, used to track whether quotes inside `$()`
/// command substitutions belong to that level or the outer level.
struct QuoteState {
    in_single: bool,
    in_double: bool,
    paren_depth: usize, // tracks nested () within this $() level
}

impl QuoteTracker {
    /// Count unbalanced quotes in a line, respecting escapes, command substitution
    /// nesting (`$(...)` and backticks), so that quotes inside substitutions don't
    /// leak to the outer level.
    ///
    /// Uses a stack of `QuoteState` to track quote context per `$()` nesting
    /// level, so that `)` inside quotes within a substitution does not
    /// prematurely close it.
    ///
    /// `$((...))` arithmetic expansions are detected and skipped — they don't
    /// create a new quoting context in bash.
    ///
    /// Returns (single_open, double_open) — true if the outermost level has an
    /// unmatched quote of that type.
    pub fn line_has_open_quote(line: &str) -> (bool, bool) {
        // Stack: index 0 = outermost level, push on $( , pop on matching )
        let mut stack: Vec<QuoteState> = vec![QuoteState { in_single: false, in_double: false, paren_depth: 0 }];
        let mut backtick_depth: usize = 0; // backtick substitution depth
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        let mut i = 0;
        while i < len {
            let ch = chars[i];
            let cur = stack.len() - 1;

            // Inside single quotes at the current nesting level:
            // Everything is literal except the closing single quote.
            // No escapes, no command substitution detection.
            if stack[cur].in_single {
                if ch == '\'' {
                    stack[cur].in_single = false;
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

            // Detect $( for command substitution (but NOT $(( which is arithmetic)
            if ch == '$' && i + 1 < len && chars[i + 1] == '(' {
                if i + 2 < len && chars[i + 2] == '(' {
                    // $(( — arithmetic expansion, skip $
                    // Arithmetic doesn't create a new quoting context
                    i += 1;
                    continue;
                }
                stack.push(QuoteState { in_single: false, in_double: false, paren_depth: 0 });
                i += 2; // skip $(
                continue;
            }

            // Track ( for nested parentheses within this $() level
            // (e.g. arithmetic $((...)), subshells, etc.)
            if ch == '(' && stack.len() > 1 && !stack[cur].in_double {
                stack[cur].paren_depth += 1;
                i += 1;
                continue;
            }

            // Detect ) — either closes nested parens or the $() substitution
            if ch == ')' && stack.len() > 1 && !stack[cur].in_double {
                if stack[cur].paren_depth > 0 {
                    stack[cur].paren_depth -= 1;
                } else {
                    stack.pop();
                }
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

            // Track quotes at the current nesting level
            if backtick_depth == 0 {
                let cur = stack.len() - 1;
                match ch {
                    '\'' if !stack[cur].in_double => {
                        stack[cur].in_single = true;
                    }
                    '"' => {
                        stack[cur].in_double = !stack[cur].in_double;
                    }
                    _ => {}
                }
            }

            i += 1;
        }

        // Report from the outermost level (stack[0])
        (stack[0].in_single, stack[0].in_double)
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

    #[test]
    fn paren_inside_quotes_in_substitution() {
        // $(echo ")") — the ) inside quotes should not close the $()
        let (s, d) = QuoteTracker::line_has_open_quote(r#"x="$(echo ")")""#);
        assert!(!s);
        assert!(!d, "paren inside quotes should not close substitution");
    }

    #[test]
    fn arithmetic_expansion_not_confused() {
        // $((...)) should not be treated as command substitution
        let (s, d) = QuoteTracker::line_has_open_quote(r#"echo "$((1+2))""#);
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn arithmetic_inside_command_substitution() {
        let (s, d) = QuoteTracker::line_has_open_quote(r#"x="$(echo $((1+2)))""#);
        assert!(!s);
        assert!(!d);
    }

    #[test]
    fn arithmetic_closing_parens_dont_pop_substitution_stack() {
        // $(( 1 + 2 )) inside $() — the )) must not pop the $() stack
        let (s, d) = QuoteTracker::line_has_open_quote(r#"x="$(echo "$((1+2))" done)""#);
        assert!(!s);
        assert!(!d, "arithmetic )) inside $() should not pop substitution stack");
    }

    #[test]
    fn subshell_parens_inside_substitution() {
        // ( subshell ) inside $() — parens tracked but don't pop $()
        let (s, d) = QuoteTracker::line_has_open_quote(r#"x="$(if true; then (echo ok); fi)""#);
        assert!(!s);
        assert!(!d, "subshell parens inside $() should not pop substitution stack");
    }
}
