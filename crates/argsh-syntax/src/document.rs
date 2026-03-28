//! Source document analysis for argsh patterns.
//!
//! Scans a bash source file for function declarations, `args=(...)` arrays,
//! `usage=(...)` arrays, `local` declarations, `:args`/`:usage` calls,
//! `import` statements, and shebang/source-argsh detection.

use regex::Regex;

use crate::field::{self, FieldDef};
use crate::usage::{self, UsageEntry};

/// A function found in the source.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name (including `::` namespacing).
    pub name: String,
    /// 0-based line number of the declaration.
    pub line: usize,
    /// 0-based line number of the closing `}`.
    pub end_line: usize,
    /// Parsed args array entries if an `args=(...)` was found.
    pub args_entries: Vec<ArgsArrayEntry>,
    /// Parsed usage array entries if a `usage=(...)` was found.
    pub usage_entries: Vec<UsageEntry>,
    /// Local variable declarations.
    pub local_vars: Vec<LocalVar>,
    /// Whether the function body contains a `:args` call.
    pub calls_args: bool,
    /// Whether the function body contains a `:usage` call.
    pub calls_usage: bool,
    /// Title string (first argument to `:args`/`:usage`).
    pub title: Option<String>,
}

/// A local variable declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVar {
    /// Variable name.
    pub name: String,
    /// 0-based line number.
    pub line: usize,
    /// Declared with `local -a`.
    pub is_array: bool,
    /// Default value if assigned inline.
    pub default_value: Option<String>,
}

/// An entry from an `args=(...)` array.
#[derive(Debug, Clone)]
pub struct ArgsArrayEntry {
    /// Raw spec string.
    pub spec: String,
    /// Description string (the paired element).
    pub description: String,
    /// Parsed field definition or error message.
    pub parsed: Result<FieldDef, String>,
    /// 0-based line number where the spec appeared.
    pub line: usize,
    /// Whether the corresponding variable is declared as `local -a` (array).
    /// When true, the flag accepts multiple values (e.g. `--files a --files b`).
    pub is_array: bool,
}

/// Result of analysing a source file.
#[derive(Debug, Clone)]
pub struct DocumentAnalysis {
    /// All functions found in the file.
    pub functions: Vec<FunctionInfo>,
    /// Import statements.
    pub imports: Vec<ImportStatement>,
    /// Whether the file contains `source argsh` (or `. argsh`).
    pub has_source_argsh: bool,
    /// Whether the shebang is `#!/usr/bin/env argsh`.
    pub has_argsh_shebang: bool,
    /// Raw shebang line if present.
    pub shebang: Option<String>,
}

/// An import statement found in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportStatement {
    /// Module name (last positional argument to `import`).
    pub module: String,
    /// 0-based line number.
    pub line: usize,
    /// Selective imports (function names), if any.
    pub selective: Vec<String>,
}

/// Analyse a bash source file for argsh patterns.
pub fn analyze(source: &str) -> DocumentAnalysis {
    let lines: Vec<&str> = source.lines().collect();

    let shebang = lines.first().and_then(|l| {
        if l.starts_with("#!") {
            Some(l.to_string())
        } else {
            None
        }
    });
    let has_argsh_shebang = shebang
        .as_ref()
        .map(|s| s.contains("argsh"))
        .unwrap_or(false);

    let has_source_argsh = lines.iter().any(|l| {
        let trimmed = l.trim();
        trimmed == "source argsh"
            || trimmed.starts_with("source argsh ")
            || trimmed == ". argsh"
            || trimmed.starts_with(". argsh ")
    });

    let functions = find_functions(&lines);
    let imports = find_imports(&lines);

    DocumentAnalysis {
        functions,
        imports,
        has_source_argsh,
        has_argsh_shebang,
        shebang,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Find all function declarations and analyse their bodies.
fn find_functions(lines: &[&str]) -> Vec<FunctionInfo> {
    let re_func = Regex::new(r"^\s*([\w][\w:.:-]*)\s*\(\)\s*\{").unwrap();
    let re_func_kw = Regex::new(r"^\s*function\s+([\w][\w:.:-]*)\s*").unwrap();

    let mut functions = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let name = if let Some(cap) = re_func.captures(line) {
            cap.get(1).unwrap().as_str().to_string()
        } else if let Some(cap) = re_func_kw.captures(line) {
            cap.get(1).unwrap().as_str().to_string()
        } else {
            continue;
        };

        let end_line = find_closing_brace(lines, i);

        let body = if end_line > i {
            &lines[i + 1..end_line]
        } else {
            &[]
        };
        let body_start = i + 1;

        let mut args_entries = extract_array_entries(lines, body, body_start, "args");
        let usage_specs = extract_array_entries(lines, body, body_start, "usage");

        let usage_entries: Vec<UsageEntry> = usage_specs
            .iter()
            .map(|ae| {
                let mut ue = usage::parse_usage_entry(&ae.spec);
                ue.description = ae.description.clone();
                ue
            })
            .collect();

        let local_vars = extract_locals(body, body_start);

        // Enrich args entries: check if the variable is declared as an array
        for entry in &mut args_entries {
            if let Ok(ref field) = entry.parsed {
                entry.is_array = local_vars.iter().any(|v| v.name == field.name && v.is_array);
            }
        }

        let (calls_args, args_title) = find_call(body, ":args");
        let (calls_usage, usage_title) = find_call(body, ":usage");

        let title = args_title.or(usage_title);

        functions.push(FunctionInfo {
            name,
            line: i,
            end_line,
            args_entries,
            usage_entries,
            local_vars,
            calls_args,
            calls_usage,
            title,
        });
    }

    functions
}

/// Walk from an opening `{` line to find the matching `}`.
/// Handles nested braces at a basic level.
fn find_closing_brace(lines: &[&str], start: usize) -> usize {
    let mut depth: i32 = 0;
    for (i, line) in lines.iter().enumerate().skip(start) {
        let trimmed = line.trim();
        // Skip comments
        if trimmed.starts_with('#') {
            continue;
        }
        // Skip strings (very rough — good enough for function boundaries)
        for ch in trimmed.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
        }
    }
    // Fallback: end of file
    lines.len().saturating_sub(1)
}

/// Extract paired entries from a bash array like `args=(...)` or `usage=(...)`.
///
/// Looks for patterns like:
/// ```bash
/// local -a args=(
///     'spec' "description"
///     ...
/// )
/// ```
/// as well as `args=(` without `local`.
fn extract_array_entries(
    _all_lines: &[&str],
    body: &[&str],
    body_start: usize,
    array_name: &str,
) -> Vec<ArgsArrayEntry> {
    let mut entries = Vec::new();

    // Pattern: `local -a ... <name>=(` or `<name>=(` at start of statement
    let decl_pattern = format!(r"(?:^|\s){}=\(\s*$", regex::escape(array_name));
    let decl_inline_pattern = format!(r"(?:^|\s){}=\(", regex::escape(array_name));
    let re_decl = Regex::new(&decl_pattern).unwrap();
    let re_decl_inline = Regex::new(&decl_inline_pattern).unwrap();

    let mut i = 0;
    while i < body.len() {
        let line = body[i].trim();

        // Check for array declaration
        if re_decl.is_match(line) || re_decl_inline.is_match(line) {
            // If the opening `(` and closing `)` are on the same line, parse inline
            if line.contains('(') && line.contains(')') {
                let content = extract_between_parens(line);
                let tokens = tokenize_array_content(&content);
                add_paired_entries(&mut entries, &tokens, body_start + i);
                i += 1;
                continue;
            }

            // Multi-line: collect until closing `)`
            let mut content = String::new();
            let array_line = body_start + i;
            i += 1;
            while i < body.len() {
                let inner = body[i].trim();
                if inner.starts_with(')') || inner == ")" {
                    break;
                }
                content.push_str(inner);
                content.push('\n');
                i += 1;
            }
            let tokens = tokenize_array_content(&content);
            add_paired_entries(&mut entries, &tokens, array_line);
        }
        i += 1;
    }

    entries
}

/// Get content between the first `(` and last `)`.
fn extract_between_parens(line: &str) -> String {
    if let Some(start) = line.find('(') {
        if let Some(end) = line.rfind(')') {
            if end > start {
                return line[start + 1..end].to_string();
            }
        }
    }
    String::new()
}

/// Tokenize array content, handling both single-quoted and double-quoted strings,
/// bare words, and the `-` separator.
fn tokenize_array_content(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = content.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '#' => {
                // Skip comments to end of line
                while let Some(&c) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '\'' => {
                chars.next(); // consume opening quote
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '\'' {
                        chars.next();
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                tokens.push(s);
            }
            '"' => {
                chars.next(); // consume opening quote
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '"' {
                        chars.next();
                        break;
                    }
                    if c == '\\' {
                        chars.next();
                        if let Some(&escaped) = chars.peek() {
                            s.push(escaped);
                            chars.next();
                        }
                        continue;
                    }
                    s.push(c);
                    chars.next();
                }
                tokens.push(s);
            }
            _ => {
                // Bare word (e.g. `-` for group separator)
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                if !s.is_empty() {
                    tokens.push(s);
                }
            }
        }
    }

    tokens
}

/// Take tokens pairwise and create `ArgsArrayEntry` items.
fn add_paired_entries(entries: &mut Vec<ArgsArrayEntry>, tokens: &[String], base_line: usize) {
    let mut i = 0;
    while i + 1 < tokens.len() {
        let spec = tokens[i].clone();
        let desc = tokens[i + 1].clone();

        let parsed = field::parse_field(&spec).map_err(|e| e.message);

        entries.push(ArgsArrayEntry {
            spec,
            description: desc,
            parsed,
            line: base_line,
            is_array: false, // enriched later from local_vars
        });
        i += 2;
    }
}

/// Extract `local` variable declarations from a function body.
fn extract_locals(body: &[&str], body_start: usize) -> Vec<LocalVar> {
    let re_local = Regex::new(r"^\s*local\s+(.+)$").unwrap();
    let mut vars = Vec::new();

    for (i, line) in body.iter().enumerate() {
        if let Some(cap) = re_local.captures(line) {
            let decl = cap.get(1).unwrap().as_str();
            let is_array = decl.starts_with("-a ");
            let decl = if is_array {
                decl.strip_prefix("-a ").unwrap_or(decl)
            } else {
                decl
            };

            // Split on whitespace to handle `local a b c` or `local -a arr=(...)`.
            // Each segment may have `=value`.
            for part in split_local_declarations(decl) {
                let part = part.trim();
                if part.is_empty() || part.starts_with('-') || part.starts_with('#') {
                    continue;
                }

                if let Some((name, value)) = part.split_once('=') {
                    let name = name.trim().to_string();
                    if name.is_empty()
                        || !name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    vars.push(LocalVar {
                        name,
                        line: body_start + i,
                        is_array,
                        default_value: Some(value.to_string()),
                    });
                } else {
                    let name = part.to_string();
                    if name.is_empty()
                        || !name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        continue;
                    }
                    vars.push(LocalVar {
                        name,
                        line: body_start + i,
                        is_array,
                        default_value: None,
                    });
                }
            }
        }
    }

    vars
}

/// Split a `local` declaration body into individual variable segments,
/// respecting parenthesised array initialisers like `arr=("a" "b")`.
fn split_local_declarations(decl: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;
    let mut in_quote = false;
    let mut quote_char = ' ';

    for ch in decl.chars() {
        match ch {
            '\'' | '"' if paren_depth > 0 => {
                if in_quote && ch == quote_char {
                    in_quote = false;
                } else if !in_quote {
                    in_quote = true;
                    quote_char = ch;
                }
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            ' ' | '\t' if paren_depth == 0 && !in_quote => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

/// Find a `:args` or `:usage` call in the body and extract the title string.
fn find_call(body: &[&str], builtin: &str) -> (bool, Option<String>) {
    let pattern = format!(r#"^\s*{}\s+"#, regex::escape(builtin));
    let re = Regex::new(&pattern).unwrap();

    for line in body {
        let trimmed = line.trim();
        if re.is_match(trimmed) {
            // Extract the first quoted argument as the title
            let title = extract_first_quoted_arg(trimmed);
            return (true, title);
        }
    }
    (false, None)
}

/// Extract the first quoted argument from a command line.
fn extract_first_quoted_arg(line: &str) -> Option<String> {
    // Find first " that starts the title
    if let Some(start) = line.find('"') {
        let rest = &line[start + 1..];
        // Find closing quote (handling escaped quotes)
        let mut result = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            } else if ch == '"' {
                return Some(result);
            } else {
                result.push(ch);
            }
        }
    }
    None
}

/// Find top-level `import` statements.
fn find_imports(lines: &[&str]) -> Vec<ImportStatement> {
    let re = Regex::new(r"^\s*import\s+(.+)$").unwrap();
    let mut imports = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = re.captures(line) {
            let args_str = cap.get(1).unwrap().as_str().trim();
            // Tokenize arguments
            let tokens = tokenize_import_args(args_str);

            // Skip --force, --list flags
            let positionals: Vec<&str> = tokens
                .iter()
                .filter(|t| !t.starts_with('-'))
                .map(|s| s.as_str())
                .collect();

            if positionals.is_empty() {
                continue;
            }

            let module = positionals.last().unwrap().to_string();
            let selective: Vec<String> = if positionals.len() > 1 {
                positionals[..positionals.len() - 1]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                vec![]
            };

            imports.push(ImportStatement {
                module,
                line: i,
                selective,
            });
        }
    }

    imports
}

/// Simple tokenizer for import arguments (handles quoted strings).
fn tokenize_import_args(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' => {
                chars.next();
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '"' {
                        chars.next();
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                tokens.push(s);
            }
            _ => {
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' || c == '\t' {
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                tokens.push(s);
            }
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SCRIPT: &str = r#"#!/usr/bin/env argsh
set -euo pipefail

import string

my::command() {
  local config
  local -a verbose args=(
    'verbose|v:+' "Description of verbose"
    'config|f'    "Description of flag"
  )
  local -a usage=(
    'cmd1|alias'       "Description of cmd1"
    'cmd2:-main::cmd2' "Description of cmd2"
    '#cmd3'            "Description of hidden cmd3"
  )
  :usage "Simple description of the command" "${@}"
  "${usage[@]}"
}

main::cmd2() {
  :args "Description of cmd2" "${@}"
  echo "cmd2"
}
"#;

    #[test]
    fn test_analyze_shebang() {
        let doc = analyze(SAMPLE_SCRIPT);
        assert!(doc.has_argsh_shebang);
        assert_eq!(doc.shebang.as_deref(), Some("#!/usr/bin/env argsh"));
    }

    #[test]
    fn test_analyze_functions() {
        let doc = analyze(SAMPLE_SCRIPT);
        assert_eq!(doc.functions.len(), 2);
        assert_eq!(doc.functions[0].name, "my::command");
        assert_eq!(doc.functions[1].name, "main::cmd2");
    }

    #[test]
    fn test_analyze_args_entries() {
        let doc = analyze(SAMPLE_SCRIPT);
        let func = &doc.functions[0];
        assert_eq!(func.args_entries.len(), 2);
        assert_eq!(func.args_entries[0].spec, "verbose|v:+");
        assert_eq!(func.args_entries[0].description, "Description of verbose");
        assert!(func.args_entries[0].parsed.is_ok());
    }

    #[test]
    fn test_analyze_usage_entries() {
        let doc = analyze(SAMPLE_SCRIPT);
        let func = &doc.functions[0];
        assert_eq!(func.usage_entries.len(), 3);
        assert_eq!(func.usage_entries[0].name, "cmd1");
        assert_eq!(func.usage_entries[0].aliases, vec!["cmd1", "alias"]);
        assert_eq!(func.usage_entries[1].explicit_func, Some("main::cmd2".to_string()));
        assert!(func.usage_entries[2].hidden);
    }

    #[test]
    fn test_analyze_calls() {
        let doc = analyze(SAMPLE_SCRIPT);
        assert!(doc.functions[0].calls_usage);
        assert!(!doc.functions[0].calls_args);
        assert!(doc.functions[1].calls_args);
        assert!(!doc.functions[1].calls_usage);
    }

    #[test]
    fn test_analyze_title() {
        let doc = analyze(SAMPLE_SCRIPT);
        assert_eq!(
            doc.functions[0].title.as_deref(),
            Some("Simple description of the command")
        );
        assert_eq!(
            doc.functions[1].title.as_deref(),
            Some("Description of cmd2")
        );
    }

    #[test]
    fn test_analyze_imports() {
        let doc = analyze(SAMPLE_SCRIPT);
        assert_eq!(doc.imports.len(), 1);
        assert_eq!(doc.imports[0].module, "string");
        assert!(doc.imports[0].selective.is_empty());
    }

    #[test]
    fn test_analyze_local_vars() {
        let doc = analyze(SAMPLE_SCRIPT);
        let func = &doc.functions[0];
        // Should find: config, verbose, args, usage
        let names: Vec<&str> = func.local_vars.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"config"), "missing 'config' in {:?}", names);
        assert!(names.contains(&"verbose"), "missing 'verbose' in {:?}", names);
        assert!(names.contains(&"args"), "missing 'args' in {:?}", names);
        assert!(names.contains(&"usage"), "missing 'usage' in {:?}", names);
    }

    #[test]
    fn test_local_multiple_vars_before_args_array() {
        // local -a files ignore_variable args=(...) should extract all three names
        let src = r#"
myfunc() {
  local -a files ignore_variable args=(
    'port|p:~int' "Port"
  )
  :args "Test" "${@}"
}
"#;
        let doc = analyze(src);
        let func = &doc.functions[0];
        let names: Vec<&str> = func.local_vars.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"files"), "missing 'files' in {:?}", names);
        assert!(names.contains(&"ignore_variable"), "missing 'ignore_variable' in {:?}", names);
        assert!(names.contains(&"args"), "missing 'args' in {:?}", names);
    }

    #[test]
    fn test_source_argsh() {
        let src = "#!/usr/bin/env bash\nsource argsh\nmain() { echo hi; }\n";
        let doc = analyze(src);
        assert!(doc.has_source_argsh);
        assert!(!doc.has_argsh_shebang);
    }

    #[test]
    fn test_selective_import() {
        let src = "import myfunc string\n";
        let doc = analyze(src);
        assert_eq!(doc.imports.len(), 1);
        assert_eq!(doc.imports[0].module, "string");
        assert_eq!(doc.imports[0].selective, vec!["myfunc"]);
    }

    #[test]
    fn test_group_separator_in_args() {
        let src = r#"
test_func() {
  local pos1 flag1
  local -a args=(
    'pos1'    "Positional 1"
    -         "Group Flags"
    'flag1|f' "Description of flag1"
  )
  :args "Test" "${@}"
}
"#;
        let doc = analyze(src);
        assert_eq!(doc.functions[0].args_entries.len(), 3);
        assert_eq!(doc.functions[0].args_entries[1].spec, "-");
    }

    #[test]
    fn test_tokenize_array_content_comments() {
        let content = "'spec1' \"desc1\"\n# comment\n'spec2' \"desc2\"";
        let tokens = tokenize_array_content(content);
        assert_eq!(tokens, vec!["spec1", "desc1", "spec2", "desc2"]);
    }

    #[test]
    fn test_function_keyword_syntax() {
        let src = "function my_func {\n  echo hello\n}\n";
        let doc = analyze(src);
        assert_eq!(doc.functions.len(), 1);
        assert_eq!(doc.functions[0].name, "my_func");
    }

    #[test]
    fn test_multiline_title() {
        let src = r#"
test_func() {
  :usage "Multi-line description
with a blank line

and more text" "${@}"
}
"#;
        let doc = analyze(src);
        // The title extraction grabs from the first quote to the next unescaped quote
        // on the same line, so multiline titles in the find_call helper only capture
        // the first line portion. This is an acceptable limitation for static analysis.
        assert!(doc.functions[0].calls_usage);
    }

    // --- Additional document analysis tests ---

    #[test]
    fn test_multi_function_with_nested_namespaces() {
        let src = r#"#!/usr/bin/env argsh

app::server::start() {
  local -a args=(
    'port|p:~int' "Port"
  )
  :args "Start server" "${@}"
}

app::server::stop() {
  :args "Stop server" "${@}"
}

app::client::connect() {
  local host
  local -a args=(
    'host|h:!' "Hostname"
  )
  :args "Connect" "${@}"
}
"#;
        let doc = analyze(src);
        assert_eq!(doc.functions.len(), 3);
        assert_eq!(doc.functions[0].name, "app::server::start");
        assert_eq!(doc.functions[1].name, "app::server::stop");
        assert_eq!(doc.functions[2].name, "app::client::connect");
        assert_eq!(doc.functions[0].args_entries.len(), 1);
        assert_eq!(doc.functions[2].args_entries.len(), 1);
    }

    #[test]
    fn test_file_with_imports() {
        let src = r#"#!/usr/bin/env argsh
import string
import --force utils
import myfunc otherfunc library

main() {
  echo "hello"
}
"#;
        let doc = analyze(src);
        assert_eq!(doc.imports.len(), 3);
        assert_eq!(doc.imports[0].module, "string");
        assert!(doc.imports[0].selective.is_empty());
        assert_eq!(doc.imports[1].module, "utils");
        assert_eq!(doc.imports[2].module, "library");
        assert_eq!(doc.imports[2].selective, vec!["myfunc", "otherfunc"]);
    }

    #[test]
    fn test_non_argsh_file() {
        let src = "#!/usr/bin/env bash\necho hello world\nfor i in 1 2 3; do echo $i; done\n";
        let doc = analyze(src);
        assert!(!doc.has_argsh_shebang);
        assert!(!doc.has_source_argsh);
        // No argsh markers, so not detected as argsh
        let is_argsh = doc.has_source_argsh
            || doc.has_argsh_shebang
            || doc.functions.iter().any(|f| f.calls_args || f.calls_usage);
        assert!(!is_argsh);
    }

    #[test]
    fn test_function_with_both_args_and_usage() {
        let src = r#"
hybrid() {
  local name
  local -a args=(
    'name|n:!' "Name"
  )
  local -a usage=(
    'sub1' "Sub one"
    'sub2' "Sub two"
  )
  :usage "Hybrid command" "${@}"
}
"#;
        let doc = analyze(src);
        let func = &doc.functions[0];
        assert_eq!(func.name, "hybrid");
        assert_eq!(func.args_entries.len(), 1);
        assert_eq!(func.usage_entries.len(), 2);
        assert!(func.calls_usage);
        assert!(!func.calls_args);
    }

    #[test]
    fn test_function_with_only_locals() {
        let src = r#"
helper() {
  local result=""
  local -a items
  echo "no args or usage"
}
"#;
        let doc = analyze(src);
        let func = &doc.functions[0];
        assert_eq!(func.name, "helper");
        assert!(func.args_entries.is_empty());
        assert!(func.usage_entries.is_empty());
        assert!(!func.calls_args);
        assert!(!func.calls_usage);
        assert!(func.title.is_none());
        // Should have local vars
        let names: Vec<&str> = func.local_vars.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"result"));
        assert!(names.contains(&"items"));
    }

    #[test]
    fn test_args_call_title_extraction() {
        let src = r#"
myfunc() {
  local -a args=()
  :args "My Function Title" "${@}"
}
"#;
        let doc = analyze(src);
        assert!(doc.functions[0].calls_args);
        assert_eq!(
            doc.functions[0].title.as_deref(),
            Some("My Function Title")
        );
    }

    #[test]
    fn test_usage_call_title_extraction() {
        let src = r#"
main() {
  local -a usage=()
  :usage "Main Application" "${@}"
}
"#;
        let doc = analyze(src);
        assert!(doc.functions[0].calls_usage);
        assert_eq!(
            doc.functions[0].title.as_deref(),
            Some("Main Application")
        );
    }

    #[test]
    fn test_function_end_line_is_correct() {
        let src = r#"first() {
  echo one
  echo two
}
second() {
  echo three
}
"#;
        let doc = analyze(src);
        assert_eq!(doc.functions.len(), 2);
        assert_eq!(doc.functions[0].line, 0);
        assert_eq!(doc.functions[0].end_line, 3);
        assert_eq!(doc.functions[1].line, 4);
        assert_eq!(doc.functions[1].end_line, 6);
    }

    #[test]
    fn test_dot_source_argsh() {
        let src = ". argsh\nmain() { echo hi; }\n";
        let doc = analyze(src);
        assert!(doc.has_source_argsh);
    }

    #[test]
    fn test_args_entry_is_array_from_local() {
        let src = r#"
f() {
  local -a files
  local output
  local -a args=(
    'files' "Input files"
    'output|o' "Output path"
  )
  :args "T" "${@}"
}
"#;
        let doc = analyze(src);
        let func = &doc.functions[0];
        // 'files' is local -a -> is_array should be true
        assert!(
            func.args_entries[0].is_array,
            "files should be detected as array"
        );
        // 'output' is plain local -> is_array should be false
        assert!(
            !func.args_entries[1].is_array,
            "output should not be array"
        );
    }
}
