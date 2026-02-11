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
            &format!(r#"^([^']*(?:(?:'[^']*')*(?:"[^"]*")*)*"[^"]*|[^'"]*){}([+\-]?=)"#, v),
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
        // \b prevents matching var name inside combined flags (e.g. `read -ra var`)
        add(
            &format!(r#"^(.*read\s.*)\b{v}([ ;}}'"\n])"#),
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
            &format!(r"^([ \t]*for\s+){v}\b"),
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
            &format!(r"^([ \t]*\({{2}}\s*[-+]{{2}}){v}\b"),
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

        // 14. Array index arithmetic: `${arr[i+1]}`, `${arr[i]}`, `${arr[i-1]}`
        add(
            &format!(r"(\$\{{[^}}]+[\[+\-]){v}([\]+\-][^}}]*\}})"),
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
            &format!(r#"^([^']*(?:(?:'[^']*')*(?:"[^"]*")*)*"[^"]*)\${}(\W)"#, v),
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
            &format!(r#"^([^']*(?:(?:'[^']*')*(?:"[^"]*")*)*"[^"]*\$\{{#?){}(\W)"#, v),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 19. [[ / (( ${#var}
        add(
            &format!(r"([(\[]{{2}}[^)]*\$\{{#?){v}([\[:}}])"),
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

    #[test]
    fn read_ra_does_not_corrupt_flag() {
        // `read -ra varname` — the `a` in `-ra` is a flag, not a variable.
        // Variable `a` being renamed must NOT match inside the flag string.
        let ob = make_obfuscator(&["varname", "a"], "a");
        // varname=a0, a=a1
        assert_eq!(
            ob.obfuscate_line("read -ra varname "),
            "read -ra a0 ",
            "flag -ra must stay intact"
        );
    }

    #[test]
    fn read_ra_combined_flag_preserved() {
        // Multiple variables with `read -ra` — flag must never be corrupted
        let ob = make_obfuscator(&["aliases", "a"], "a");
        // aliases=a0, a=a1
        assert_eq!(
            ob.obfuscate_line("IFS='|' read -ra aliases "),
            "IFS='|' read -ra a0 ",
            "-ra flag must not become -ra1"
        );
    }

    #[test]
    fn read_ra_with_herestring() {
        // Real-world pattern from args.sh: `IFS='|' read -ra var <<< "$str"`
        let ob = make_obfuscator(&["flags", "a"], "a");
        // flags=a0, a=a1
        let result = ob.obfuscate_line(r#"IFS='|' read -ra flags <<< "${field}""#);
        assert!(
            result.contains("read -ra a0"),
            "flag -ra must stay intact, got: {result}"
        );
        assert!(
            !result.contains("read -ra1"),
            "flag must not be corrupted, got: {result}"
        );
    }

    #[test]
    fn renames_array_subscript_simple() {
        let ob = make_obfuscator(&["i"], "a");
        assert_eq!(
            ob.obfuscate_line(r#"echo "${usage[i]}""#),
            r#"echo "${usage[a0]}""#,
        );
    }

    #[test]
    fn renames_array_subscript_arithmetic() {
        let ob = make_obfuscator(&["i"], "a");
        assert_eq!(
            ob.obfuscate_line(r#"echo "${arr[i+1]}""#),
            r#"echo "${arr[a0+1]}""#,
        );
    }

    #[test]
    fn renames_array_subscript_in_conditional() {
        let ob = make_obfuscator(&["i"], "a");
        assert_eq!(
            ob.obfuscate_line(r#"[[ "${args[i]}" != "-" ]]"#),
            r#"[[ "${args[a0]}" != "-" ]]"#,
        );
    }

    #[test]
    fn renames_var_in_hash_length() {
        // Rule 19: [[ / (( ${#var}
        let ob = make_obfuscator(&["arr"], "a");
        assert_eq!(
            ob.obfuscate_line(r#"(( ${#arr[@]} > 0 ))"#),
            r#"(( ${#a0[@]} > 0 ))"#,
        );
    }

    #[test]
    fn renames_arithmetic_context_assignment() {
        // Rule 20b: (( var+= ))
        let ob = make_obfuscator(&["i"], "a");
        assert_eq!(ob.obfuscate_line("(( a0+=2 ))"), "(( a0+=2 ))");
        // Actually test with the real var
        let ob2 = make_obfuscator(&["count"], "a");
        assert_eq!(ob2.obfuscate_line("(( count+=1 ))"), "(( a0+=1 ))");
    }

    #[test]
    fn all_patterns_compile() {
        // Ensure no regex patterns are silently skipped due to compile errors
        let ob = make_obfuscator(&["testvar"], "a");
        // If any pattern failed to compile, the obfuscator would have fewer rules.
        // Test a comprehensive input that exercises many patterns.
        let input = r#"local testvar=1; echo $testvar ${testvar} ${arr[testvar]} ${arr[testvar+1]} (( testvar+=1 ))"#;
        let result = ob.obfuscate_line(input);
        // All occurrences should be renamed
        assert!(
            !result.contains("testvar"),
            "All occurrences of testvar should be renamed, got: {result}"
        );
    }

    // ---- Multi-variable collision tests ----
    // These test that renaming short variables (like `a`) doesn't corrupt
    // already-renamed longer variables (like `alias` → `a0`).

    #[test]
    fn multi_var_for_loop_no_collision() {
        // `alias` (longer) is renamed first to `a0`, then `a` to `a1`.
        // Rule 7 must NOT match `a` inside `a0`.
        let ob = make_obfuscator(&["alias", "a"], "a");
        // alias=a0, a=a1 (sorted by length desc)
        assert_eq!(
            ob.obfuscate_line("for alias in x y z"),
            "for a0 in x y z",
        );
        assert_eq!(
            ob.obfuscate_line("for a in x y z"),
            "for a1 in x y z",
        );
    }

    #[test]
    fn multi_var_for_loop_preserves_renamed() {
        // After `alias` → `a0`, processing `a` must not corrupt `for a0 in`
        let ob = make_obfuscator(&["alias", "a"], "a");
        let lines = vec![
            "for alias in 1 2 3".to_string(),
            "echo $alias $a ".to_string(),
        ];
        let result = ob.obfuscate_lines(&lines);
        assert_eq!(result[0], "for a0 in 1 2 3");
        assert_eq!(result[1], "echo $a0 $a1 ");
    }

    #[test]
    fn multi_var_pre_increment_no_collision() {
        // Rule 11: `(( ++var` must not match `a` inside `a0`
        let ob = make_obfuscator(&["count", "c"], "a");
        // count=a0, c=a1
        assert_eq!(ob.obfuscate_line("(( ++count ))"), "(( ++a0 ))");
    }

    #[test]
    fn multi_var_assignment_no_collision() {
        // After `alias` → `a0`, `a0=val` should NOT be further mangled by rule 1 for `a`
        let ob = make_obfuscator(&["alias", "a"], "a");
        assert_eq!(ob.obfuscate_line("alias=val"), "a0=val");
        assert_eq!(ob.obfuscate_line("a=val"), "a1=val");
    }

    #[test]
    fn multi_var_dollar_no_collision() {
        // $a must not match inside $a0
        let ob = make_obfuscator(&["alias", "a"], "a");
        assert_eq!(ob.obfuscate_line("echo $alias "), "echo $a0 ");
        assert_eq!(ob.obfuscate_line("echo $a "), "echo $a1 ");
    }

    #[test]
    fn multi_var_brace_no_collision() {
        // ${a} must not match inside ${a0}
        let ob = make_obfuscator(&["alias", "a"], "a");
        assert_eq!(ob.obfuscate_line("echo ${alias}"), "echo ${a0}");
        assert_eq!(ob.obfuscate_line("echo ${a}"), "echo ${a1}");
    }

    #[test]
    fn multi_var_many_variables() {
        // Simulate realistic scenario with many vars of varying lengths
        let ob = make_obfuscator(&["field", "alias", "all", "cmd", "i", "a"], "a");
        // field=a0, alias=a1, all=a2, cmd=a3, i=a4, a=a5
        let lines = vec![
            "for alias in x y".to_string(),
            "for i in 1 2 3".to_string(),
            "for a in p q".to_string(),
            "echo $field $alias $all $cmd $i $a ".to_string(),
        ];
        let result = ob.obfuscate_lines(&lines);
        assert_eq!(result[0], "for a1 in x y", "alias for-loop");
        assert_eq!(result[1], "for a4 in 1 2 3", "i for-loop");
        assert_eq!(result[2], "for a5 in p q", "a for-loop");
        // Verify no original var names remain in the dollar-var line
        let dollar_line = &result[3];
        for var in &["field", "alias", "all", "cmd"] {
            assert!(
                !dollar_line.contains(var),
                "'{var}' should be renamed in: {dollar_line}"
            );
        }
    }

    #[test]
    fn multi_var_prefix_is_var_name() {
        // Edge case: the prefix itself is a variable name
        // With prefix "x" and variable "x", renamed to "x0".
        // Then `for x0 in` should not be corrupted by pattern for `x`.
        let ob = make_obfuscator(&["xvar", "x"], "x");
        // xvar=x0, x=x1
        assert_eq!(ob.obfuscate_line("for xvar in a b"), "for x0 in a b");
        assert_eq!(ob.obfuscate_line("for x in a b"), "for x1 in a b");
    }
}
