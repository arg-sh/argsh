//! Variable name obfuscation for bash scripts.
//!
//! Renames discovered local variables using short generated names (`a0`, `a1`, ...)
//! across 21 substitution contexts: assignments, `$var`, `${var}`, arithmetic,
//! arrays, parameter expansions, and more — while respecting single-quote boundaries.

use regex::Regex;
use std::collections::HashMap;

/// Build the variable rename map: `sorted_vars\[i\]` -> `prefix + i`.
pub fn build_rename_map(sorted_vars: &[String], prefix: &str) -> HashMap<String, String> {
    sorted_vars
        .iter()
        .enumerate()
        .map(|(i, var)| (var.clone(), format!("{prefix}{i}")))
        .collect()
}

/// A pre-compiled set of regex patterns + replacements for one variable.
struct VarPatterns {
    /// Each entry: (compiled regex, replacement string, use_loop)
    rules: Vec<(Regex, String, bool)>,
}

impl VarPatterns {
    fn compile(var: &str, rep: &str) -> Self {
        let v = regex::escape(var);
        // In regex replacement strings: `$$` = literal `$`, `${N}` = capture group N.
        // `r` is the replacement name (e.g. "a0"). For $var contexts, we need `$$rep`.
        let r = rep;
        let dollar_r = format!("$${r}"); // literal $ + replacement name

        let mut rules = Vec::new();
        let mut add = |pat: &str, repl: String, looped: bool| {
            if let Ok(re) = Regex::new(pat) {
                rules.push((re, repl, looped));
            }
        };

        // 1. Assignment: `var=`
        add(
            &format!(r"([ \t]*){v}="),
            format!("${{1}}{r}="),
            true,
        );

        // 2. local/declare: `local [-x] ... var[= ]`
        add(
            &format!(r"([ \t]*(?:local|declare)(?:[ \t]|-\w)*[^;]*\s){v}(\s|=|$)"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 3. Assignment in non-quote context
        add(
            &format!(r#"^([^']*(?:(?:'[^']*')*(?:"[^"]")*)*"[^"]*|[^'"]*){}([+\-]?=)"#, v),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 4. Assignment after pipe: `| var+=`
        add(
            &format!(r"([|]\s+){v}([+\-]?=)"),
            format!("${{1}}{r}${{2}}"),
            false,
        );

        // 5. read statement
        add(
            &format!(r#"^(.*read\s.*){v}([ ;}}'"\n])"#),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 6. printf -v / mapfile -t
        add(
            &format!(r"^(printf\s+-v\s+|mapfile\s+-t\s+){v}([^\w])"),
            format!("${{1}}{r}${{2}}"),
            false,
        );

        // 7. for loop
        add(
            &format!(r"^([ \t]*for\s+){v}"),
            format!("${{1}}{r}"),
            false,
        );

        // 8. Array write: `var[...]=`
        add(
            &format!(r"^([ \t]*){v}(\[.+\]=)"),
            format!("${{1}}{r}${{2}}"),
            false,
        );

        // 9. Array read: `${var[`
        add(
            &format!(r"^(.*\$\{{){v}(\[)"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 10. unset: `unset 'var[` or `unset "var[`
        add(
            &format!(r#"^(.*unset\s+['"]){v}(\[)"#),
            format!("${{1}}{r}${{2}}"),
            false,
        );

        // 11. Pre-increment: `(( ++var`
        add(
            &format!(r"^([ \t]*\({{2}}\s*[-+]{{2}}){v}"),
            format!("${{1}}{r}"),
            false,
        );

        // 12. Post-increment: `(( var++`
        add(
            &format!(r"^([ \t]*\({{2}}\s*){v}([-+]{{2}})"),
            format!("${{1}}{r}${{2}}"),
            false,
        );

        // 13. Parameter expansion modifiers: `:+`, `:-`, etc.
        add(
            &format!(r"([:+\- ]+){v}([:}}+])"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 14. Array index arithmetic: `${arr[i+1]}`
        add(
            &format!(r"(\$\{{[^}}]+[[+\-]){v}([]+\-][^}}]*\}})"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 15. General $var (not inside single quotes)
        // Note: `$$` in replacement = literal `$`
        add(
            &format!(r"^((?:[^']*(?:'[^']*')*[^']*)*)\${v}(\W)"),
            format!("${{1}}{dollar_r}${{2}}"),
            true,
        );

        // 16. $var inside "" that's inside '' context
        add(
            &format!(r#"^([^']*(?:(?:'[^']*')*(?:"[^"]")*)*"[^"]*)\${}(\W)"#, v),
            format!("${{1}}{dollar_r}${{2}}"),
            true,
        );

        // 17. ${var} not inside single quotes
        add(
            &format!(r"^((?:[^']*(?:'[^']*')*[^']*)*\$\{{[!#]?){v}(\W)"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 18. ${var} inside "" within '' context
        add(
            &format!(r#"^([^']*(?:(?:'[^']*')*(?:"[^"]")*)*"[^"]*\$\{{#?){}(\W)"#, v),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 19. [[ / (( ${#var}
        add(
            &format!(r"([(\[]{{2}}[^)]*\$\{{#?){v}([[:}}])"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 20a. Arithmetic context: spaces/operators
        add(
            &format!(r"(\(\([^)]*[\s;<>]){v}([;\s<>])"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 20b. Arithmetic context: equals/math
        add(
            &format!(r"(\(\([^)]*[=+\-\s]){v}([=+\-\s);\[])"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        Self { rules }
    }

    fn apply(&self, line: &str) -> String {
        let mut s = line.to_string();
        for (re, repl, looped) in &self.rules {
            if *looped {
                for _ in 0..100 {
                    let next = re.replace(&s, repl.as_str()).to_string();
                    if next == s {
                        break;
                    }
                    s = next;
                }
            } else {
                s = re.replace(&s, repl.as_str()).to_string();
            }
        }
        s
    }
}

/// Pre-compiled patterns for all variables.
pub struct Obfuscator {
    patterns: Vec<VarPatterns>,
}

impl Obfuscator {
    /// Pre-compile all patterns for all variables.
    pub fn new(sorted_vars: &[String], rename: &HashMap<String, String>) -> Self {
        let patterns = sorted_vars
            .iter()
            .map(|var| VarPatterns::compile(var, &rename[var]))
            .collect();
        Self { patterns }
    }

    /// Obfuscate a single line.
    /// Appends a newline sentinel so `\W` patterns match at end-of-line
    /// (Perl reads lines with trailing newlines; we emulate that).
    pub fn obfuscate_line(&self, line: &str) -> String {
        let mut s = format!("{line}\n");
        for pat in &self.patterns {
            s = pat.apply(&s);
        }
        // Strip the sentinel newline
        if s.ends_with('\n') {
            s.pop();
        }
        s
    }

    /// Obfuscate all lines.
    pub fn obfuscate_lines(&self, lines: &[String]) -> Vec<String> {
        lines.iter().map(|l| self.obfuscate_line(l)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_obfuscator(vars: &[&str], prefix: &str) -> Obfuscator {
        let sorted: Vec<String> = vars.iter().map(|s| s.to_string()).collect();
        let map = build_rename_map(&sorted, prefix);
        Obfuscator::new(&sorted, &map)
    }

    #[test]
    fn renames_assignment() {
        let ob = make_obfuscator(&["foo"], "a");
        assert_eq!(ob.obfuscate_line("foo=bar"), "a0=bar");
    }

    #[test]
    fn renames_dollar_var() {
        let ob = make_obfuscator(&["name"], "a");
        assert_eq!(ob.obfuscate_line("echo $name "), "echo $a0 ");
    }

    #[test]
    fn renames_brace_var() {
        let ob = make_obfuscator(&["name"], "a");
        assert_eq!(ob.obfuscate_line("echo ${name}"), "echo ${a0}");
    }

    #[test]
    fn preserves_single_quotes() {
        let ob = make_obfuscator(&["foo"], "a");
        assert_eq!(ob.obfuscate_line("echo '$foo'"), "echo '$foo'");
    }

    #[test]
    fn renames_in_double_quotes() {
        let ob = make_obfuscator(&["foo"], "a");
        let result = ob.obfuscate_line(r#"echo "$foo bar""#);
        assert_eq!(result, r#"echo "$a0 bar""#);
    }

    #[test]
    fn obfuscate_lines_works() {
        let ob = make_obfuscator(&["name"], "a");
        let lines: Vec<String> = vec![
            "name=hello".to_string(),
            "echo $name ".to_string(),
        ];
        let result = ob.obfuscate_lines(&lines);
        assert_eq!(result[0], "a0=hello");
        assert_eq!(result[1], "echo $a0 ");
    }

    #[test]
    fn renames_local_declaration() {
        let ob = make_obfuscator(&["myvar"], "a");
        assert_eq!(ob.obfuscate_line("local myvar=test"), "local a0=test");
    }

    #[test]
    fn renames_for_loop() {
        let ob = make_obfuscator(&["item"], "a");
        let result = ob.obfuscate_line("for item in a b c");
        assert!(result.contains("for a0"), "Got: {result}");
    }

    #[test]
    fn renames_array_write() {
        let ob = make_obfuscator(&["arr"], "a");
        assert_eq!(ob.obfuscate_line("arr[0]=val"), "a0[0]=val");
    }

    #[test]
    fn renames_brace_expansion() {
        let ob = make_obfuscator(&["name"], "a");
        assert_eq!(ob.obfuscate_line("echo ${name:-default}"), "echo ${a0:-default}");
    }

    #[test]
    fn renames_read() {
        let ob = make_obfuscator(&["line"], "a");
        let result = ob.obfuscate_line("read -r line ");
        assert!(result.contains("a0"), "Got: {result}");
    }
}
