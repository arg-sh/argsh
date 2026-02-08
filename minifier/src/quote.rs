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
            let prev = if i > 0 { chars[i - 1] } else { '\0' };
            match ch {
                '\'' if !in_double && prev != '\\' => {
                    in_single = !in_single;
                }
                '"' if !in_single && prev != '\\' => {
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
}
