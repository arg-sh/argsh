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
        // \b prevents matching var name as suffix of longer name (e.g. `_path=` when var is `path`)
        add(
            &format!(r"([ \t]*)\b{v}="),
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
        // \b prevents matching var name as suffix of longer name (e.g. `_path=` when var is `path`)
        add(
            &format!(r#"^([^']*(?:(?:'[^']*')*(?:"[^"]*")*)*"[^"]*|[^'"]*)\b{}([+\-]?=)"#, v),
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
        // No ^ anchor — array writes can appear mid-line (e.g. `do prev[j]=`).
        // \b prevents matching inside longer names (e.g. `_path[0]=`).
        add(
            &format!(r"([ \t]*)\b{v}(\[.+?\]=)"),
            format!("${{1}}{r}${{2}}"),
            true,
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

        // 14b. Substring offset: variable at start `${var:i-1:1}`
        // After the first `:`, the variable name starts immediately.
        // Modifiers (`:- :+ := :?`) start with a non-word char, so this won't match them.
        add(
            &format!(r"(\$\{{[^}}:]+:){v}\b([^}}]*\}})"),
            format!("${{1}}{r}${{2}}"),
            true,
        );

        // 14c. Substring offset/length: variable in middle `${var:0:i}`, `${var:i+j:1}`
        // `\w` after `:` ensures it's a substring context (not a modifier like `:-`).
        add(
            &format!(r"(\$\{{[^}}:]+:\w[^}}]*?)\b{v}\b([^}}]*\}})"),
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
    fn underscore_prefix_not_corrupted_assignment() {
        // `_path=` must NOT be renamed when `path` is a discovered variable.
        // Rule 1 (assignment) and rule 3 (non-quote assignment) must respect word boundaries.
        let ob = make_obfuscator(&["path"], "a");
        // `path` renamed to `a0`, but `_path` is a different variable — must stay intact.
        assert_eq!(ob.obfuscate_line("_path=val"), "_path=val", "_path must not be renamed");
        assert_eq!(ob.obfuscate_line("path=val"), "a0=val", "path must be renamed");
    }

    #[test]
    fn underscore_prefix_not_corrupted_local() {
        // `local _path=""` — the `path` inside `_path` must not be renamed.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(
            ob.obfuscate_line("local _force=0 _path=\"\""),
            "local _force=0 _path=\"\"",
            "_path in local declaration must not be renamed"
        );
        assert_eq!(
            ob.obfuscate_line("local path=\"\""),
            "local a0=\"\"",
            "path in local declaration must be renamed"
        );
    }

    #[test]
    fn underscore_prefix_not_corrupted_brace_ref() {
        // `${_path}` must not be renamed when `path` is discovered.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(ob.obfuscate_line("echo ${_path}"), "echo ${_path}");
        assert_eq!(ob.obfuscate_line("echo ${path}"), "echo ${a0}");
    }

    #[test]
    fn underscore_prefix_consistency() {
        // End-to-end: _path must stay _path everywhere — no partial rename.
        let ob = make_obfuscator(&["path"], "a");
        let lines = vec![
            "local path=\"${1}\"".to_string(),
            "local _force=0 _path=\"\"".to_string(),
            "_path=\"${1}\"".to_string(),
            "echo \"${_path}\"".to_string(),
            "echo \"${path}\"".to_string(),
        ];
        let result = ob.obfuscate_lines(&lines);
        assert_eq!(result[0], "local a0=\"${1}\"");
        assert_eq!(result[1], "local _force=0 _path=\"\"", "_path must not be renamed");
        assert_eq!(result[2], "_path=\"${1}\"", "_path assignment must not be renamed");
        assert_eq!(result[3], "echo \"${_path}\"", "${{_path}} must not be renamed");
        assert_eq!(result[4], "echo \"${a0}\"");
    }

    #[test]
    fn array_write_midline() {
        // Rule 8: `var[i]=` must be renamed even when mid-line (e.g. after `do`).
        let ob = make_obfuscator(&["prev"], "a");
        // Standalone line — always worked
        assert_eq!(ob.obfuscate_line("prev[0]=val"), "a0[0]=val");
        // Mid-line after `do` — was broken (^-anchored rule missed it)
        assert_eq!(
            ob.obfuscate_line("for (( j=0; j <= n; j++ )); do prev[j]=\"${j}\"; done"),
            "for (( j=0; j <= n; j++ )); do a0[j]=\"${j}\"; done",
            "mid-line array write must be renamed"
        );
        // Multiple array writes on one line
        assert_eq!(
            ob.obfuscate_line("prev[0]=1; prev[1]=2"),
            "a0[0]=1; a0[1]=2",
        );
    }

    #[test]
    fn array_write_midline_underscore_safe() {
        // Rule 8 with \b must not rename _prev[j]= when `prev` is discovered.
        let ob = make_obfuscator(&["prev"], "a");
        assert_eq!(ob.obfuscate_line("do _prev[j]=\"${j}\""), "do _prev[j]=\"${j}\"");
        assert_eq!(ob.obfuscate_line("do prev[j]=\"${j}\""), "do a0[j]=\"${j}\"");
    }

    #[test]
    fn underscore_prefix_not_corrupted_dollar_var() {
        // Rule 15/16: `$_path` must NOT be renamed when `path` is discovered.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(ob.obfuscate_line("echo $_path "), "echo $_path ");
        assert_eq!(ob.obfuscate_line("echo $path "), "echo $a0 ");
        // Inside double quotes
        assert_eq!(ob.obfuscate_line(r#"echo "$_path ""#), r#"echo "$_path ""#);
        assert_eq!(ob.obfuscate_line(r#"echo "$path ""#), r#"echo "$a0 ""#);
    }

    #[test]
    fn underscore_prefix_not_corrupted_param_expansion() {
        // Rule 13: `${var:-_path}` — `_path` as default value must stay.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(
            ob.obfuscate_line("echo ${var:-_path}"),
            "echo ${var:-_path}",
        );
        assert_eq!(
            ob.obfuscate_line("echo ${var:-path}"),
            "echo ${var:-a0}",
        );
    }

    #[test]
    fn underscore_prefix_not_corrupted_array() {
        // Rule 8/9: `_path[0]=` and `${_path[0]}` must stay intact.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(ob.obfuscate_line("_path[0]=val"), "_path[0]=val");
        assert_eq!(ob.obfuscate_line("path[0]=val"), "a0[0]=val");
        assert_eq!(ob.obfuscate_line("echo ${_path[0]}"), "echo ${_path[0]}");
        assert_eq!(ob.obfuscate_line("echo ${path[0]}"), "echo ${a0[0]}");
    }

    #[test]
    fn underscore_prefix_not_corrupted_read() {
        // Rule 5: `read _path` must NOT rename.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(ob.obfuscate_line("read -r _path "), "read -r _path ");
        assert_eq!(ob.obfuscate_line("read -r path "), "read -r a0 ");
    }

    #[test]
    fn underscore_prefix_not_corrupted_for_loop() {
        // Rule 7: `for _path in` must NOT rename.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(ob.obfuscate_line("for _path in a b"), "for _path in a b");
        assert_eq!(ob.obfuscate_line("for path in a b"), "for a0 in a b");
    }

    #[test]
    fn underscore_prefix_not_corrupted_arithmetic() {
        // Rules 11/12/20: arithmetic contexts with underscore prefix.
        let ob = make_obfuscator(&["count"], "a");
        assert_eq!(ob.obfuscate_line("(( ++_count ))"), "(( ++_count ))");
        assert_eq!(ob.obfuscate_line("(( ++count ))"), "(( ++a0 ))");
        assert_eq!(ob.obfuscate_line("(( _count++ ))"), "(( _count++ ))");
        assert_eq!(ob.obfuscate_line("(( count++ ))"), "(( a0++ ))");
    }

    #[test]
    fn long_underscore_prefix_not_corrupted() {
        // Real-world pattern: `_argsh_builtin_path` contains `path` at the end.
        let ob = make_obfuscator(&["path"], "a");
        let lines = vec![
            "local _argsh_builtin_path=\"\"".to_string(),
            "_argsh_builtin_path=\"${1}\"".to_string(),
            "echo \"${_argsh_builtin_path}\"".to_string(),
            "echo \"$_argsh_builtin_path \"".to_string(),
        ];
        let result = ob.obfuscate_lines(&lines);
        for (i, line) in result.iter().enumerate() {
            assert!(
                !line.contains("a0"),
                "line {i} corrupted — path found inside _argsh_builtin_path: {line}"
            );
            assert!(
                line.contains("_argsh_builtin_path"),
                "line {i} — _argsh_builtin_path must stay intact: {line}"
            );
        }
    }

    #[test]
    fn underscore_prefix_hash_length() {
        // Rule 19: `${#_path[@]}` must not rename path inside _path.
        let ob = make_obfuscator(&["path"], "a");
        assert_eq!(ob.obfuscate_line("[[ ${#_path[@]} -gt 0 ]]"), "[[ ${#_path[@]} -gt 0 ]]");
        assert_eq!(ob.obfuscate_line("[[ ${#path[@]} -gt 0 ]]"), "[[ ${#a0[@]} -gt 0 ]]");
    }

    #[test]
    fn substring_offset_renamed() {
        // Rule 14b: bare variables in `${var:offset}` and `${var:offset:length}` must be renamed.
        let ob = make_obfuscator(&["i"], "a");
        // Offset position: `${str:i-1:1}` → `${str:a0-1:1}`
        assert_eq!(ob.obfuscate_line("${str:i-1:1}"), "${str:a0-1:1}");
        // Length position: `${str:0:i}` → `${str:0:a0}`
        assert_eq!(ob.obfuscate_line("${str:0:i}"), "${str:0:a0}");
        // Both positions: `${str:i:i}` → `${str:a0:a0}`
        assert_eq!(ob.obfuscate_line("${str:i:i}"), "${str:a0:a0}");
        // Embedded in expression: `${arr:i+1:j-i}`
        let ob2 = make_obfuscator(&["j", "i"], "a");
        // j=a0, i=a1 (sorted by length, then alpha — both len 1, j before i alphabetically? No, j > i)
        // Actually sorted descending by length, then alpha: j and i both len 1; sorted: ["i", "j"] or ["j", "i"]
        // Let's just check the output
        let result = ob2.obfuscate_line("${arr:i+1:j-i}");
        assert!(!result.contains(":i"), "i not renamed in offset: {result}");
        assert!(!result.contains(":j"), "j not renamed in length: {result}");
    }

    #[test]
    fn substring_offset_not_param_expansion() {
        // Rules 14b/14c must NOT match parameter expansion modifiers: `${var:-i}`, `${var:+i}`
        // Modifiers start with non-word chars (`-`,`+`,`=`,`?`) so \w-based rules skip them.
        let ob = make_obfuscator(&["i"], "a");
        // `:-` modifier: `i` here is renamed by rule 13 (param expansion), not 14b
        assert_eq!(ob.obfuscate_line("${var:-i}"), "${var:-a0}");
        // `:+` modifier: same
        assert_eq!(ob.obfuscate_line("${var:+i}"), "${var:+a0}");
        // `:=` and `:?` modifiers: rule 13 doesn't cover `=`/`?` (pre-existing),
        // but 14b/14c must NOT touch them either (they start with non-word chars).
        assert_eq!(ob.obfuscate_line("${var:=i}"), "${var:=i}");
        assert_eq!(ob.obfuscate_line("${var:?i}"), "${var:?i}");
    }

    #[test]
    fn substring_offset_underscore_safe() {
        // Rule 14b with \b must not rename `_i` when `i` is the discovered variable.
        let ob = make_obfuscator(&["i"], "a");
        assert_eq!(ob.obfuscate_line("${str:_i-1:1}"), "${str:_i-1:1}");
        assert_eq!(ob.obfuscate_line("${str:0:_i}"), "${str:0:_i}");
        // But bare `i` must still be renamed
        assert_eq!(ob.obfuscate_line("${str:i-1:1}"), "${str:a0-1:1}");
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
