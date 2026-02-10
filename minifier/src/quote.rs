//! Quote-tracking utilities for bash parsing.
//!
//! Determines whether a line has unbalanced (open) single or double quotes,
//! used by the join phase to detect multi-line strings.

/// Stateless quote analysis.
pub struct QuoteTracker;

impl QuoteTracker {
    /// Count unbalanced quotes in a line, respecting escapes and nesting.
    /// Returns (single_open, double_open) — true if an odd number of that quote type.
    pub fn line_has_open_quote(line: &str) -> (bool, bool) {
        let mut in_single = false;
        let mut in_double = false;
        let chars: Vec<char> = line.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            // Count consecutive preceding backslashes. Odd = escaped, even = not.
            // Inside single quotes backslash is literal, so skip escape check.
            let is_escaped = if !in_single {
                let mut backslashes = 0;
                let mut j = i;
                while j > 0 && chars[j - 1] == '\\' {
                    backslashes += 1;
                    j -= 1;
                }
                backslashes % 2 == 1
            } else {
                false
            };

            match ch {
                // Inside single quotes backslash is literal, so always toggle.
                // Outside single quotes, skip escaped quotes.
                '\'' if !in_double && (in_single || !is_escaped) => {
                    in_single = !in_single;
                }
                '"' if !in_single && !is_escaped => {
                    in_double = !in_double;
                }
                _ => {}
            }
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
}
