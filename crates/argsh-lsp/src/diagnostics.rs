use std::collections::HashSet;

use tower_lsp::lsp_types::*;

use argsh_syntax::document::{DocumentAnalysis, FunctionInfo};

use crate::resolver::ResolvedImports;

/// Diagnostic codes — like shellcheck's SC#### identifiers.
/// Users can suppress them with `# argsh-ignore=AG001,AG002` comments.
pub mod codes {
    pub const AG001: &str = "AG001"; // args entry missing description
    pub const AG002: &str = "AG002"; // usage entry missing description
    pub const AG003: &str = "AG003"; // invalid field spec (modifier error)
    pub const AG004: &str = "AG004"; // missing local variable declaration
    pub const AG005: &str = "AG005"; // args declared but :args not called
    pub const AG006: &str = "AG006"; // usage declared but :usage not called
    pub const AG007: &str = "AG007"; // usage target function not found
    pub const AG008: &str = "AG008"; // duplicate flag name
    pub const AG009: &str = "AG009"; // duplicate short alias
    pub const AG010: &str = "AG010"; // command resolves to bare function (not namespaced)
}

/// Generate LSP diagnostics from a document analysis.
pub fn generate_diagnostics(
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
    content: &str,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let suppressed = collect_suppressions(content);

    for func in &analysis.functions {
        check_args_array_pairing(func, &mut diags);
        check_usage_array_pairing(func, &mut diags);
        check_field_parse_errors(func, &mut diags);
        check_missing_variable_declarations(func, &mut diags);
        check_missing_args_call(func, &mut diags);
        check_missing_usage_call(func, &mut diags);
        check_usage_function_targets(func, analysis, imports, &mut diags);
        check_duplicate_flags(func, &mut diags);
        check_duplicate_short_aliases(func, &mut diags);
        check_bare_function_resolution(func, analysis, imports, &mut diags);
    }

    // Filter out suppressed diagnostics
    diags.retain(|d| !is_suppressed(d, &suppressed));
    diags
}

/// A suppression directive found in a comment.
#[derive(Debug)]
struct Suppression {
    codes: Vec<String>, // empty = suppress all
    scope: SuppressionScope,
}

#[derive(Debug)]
enum SuppressionScope {
    NextLine(usize),   // suppress diagnostics on line N+1
    Line(usize),       // suppress diagnostics on this line
    File,              // suppress for entire file
    Function(usize),   // suppress within function starting at this line
}

/// Collect all `# argsh-ignore` suppression comments from the source.
///
/// Supported formats (like shellcheck's `# shellcheck disable=SC1502`):
/// - `# argsh disable=AG001,AG004` — suppress specific codes on next line
/// - `# argsh disable=AG001` — suppress one code on next line
/// - `# argsh disable-file=AG007` — suppress specific codes for entire file
/// - `# argsh disable-file` — suppress all for entire file
/// - `some code # argsh disable=AG004` — suppress on this line (inline)
/// Also supports hyphenated form: `# argsh-ignore=AG001`
fn collect_suppressions(content: &str) -> Vec<Suppression> {
    let mut suppressions = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Try all directive forms: "argsh disable-file", "argsh-ignore-file"
        let file_directives = ["argsh disable-file", "argsh-ignore-file"];
        let line_directives = ["argsh disable", "argsh-ignore"];

        // Check for file-level suppression
        let mut found = false;
        for dir in &file_directives {
            if let Some(rest) = extract_directive(trimmed, dir) {
                suppressions.push(Suppression {
                    codes: parse_codes(rest),
                    scope: SuppressionScope::File,
                });
                found = true;
                break;
            }
        }
        if found { continue; }

        // Check for line-level suppression (standalone comment = next line)
        for dir in &line_directives {
            if let Some(rest) = extract_directive(trimmed, dir) {
                if trimmed.starts_with('#') {
                    suppressions.push(Suppression {
                        codes: parse_codes(rest),
                        scope: SuppressionScope::NextLine(i),
                    });
                    found = true;
                }
                break;
            }
        }
        if found { continue; }

        // Check for inline suppression (code before comment)
        if let Some(comment_pos) = line.rfind('#') {
            let comment = line[comment_pos + 1..].trim();
            for dir in &line_directives {
                if let Some(rest) = extract_directive(comment, dir) {
                    suppressions.push(Suppression {
                        codes: parse_codes(rest),
                        scope: SuppressionScope::Line(i),
                    });
                    break;
                }
            }
        }
    }

    suppressions
}

/// Extract the part after a directive keyword, e.g. "argsh-ignore=AG001" → Some("=AG001")
fn extract_directive<'a>(text: &'a str, directive: &str) -> Option<&'a str> {
    // Match "# argsh-ignore..." or just "argsh-ignore..."
    let stripped = text.strip_prefix('#').unwrap_or(text).trim();
    if stripped.starts_with(directive) {
        Some(&stripped[directive.len()..])
    } else {
        None
    }
}

/// Parse codes from "=AG001,AG004" or "" (empty = all).
fn parse_codes(rest: &str) -> Vec<String> {
    if let Some(codes_str) = rest.strip_prefix('=') {
        codes_str
            .split(',')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect()
    } else {
        vec![] // empty = suppress all
    }
}

/// Check if a diagnostic is suppressed by any suppression directive.
fn is_suppressed(diag: &Diagnostic, suppressions: &[Suppression]) -> bool {
    let diag_line = diag.range.start.line as usize;
    let diag_code = diag.code.as_ref().and_then(|c| match c {
        NumberOrString::String(s) => Some(s.as_str()),
        _ => None,
    });

    for sup in suppressions {
        let scope_matches = match sup.scope {
            SuppressionScope::File => true,
            SuppressionScope::NextLine(comment_line) => diag_line == comment_line + 1,
            SuppressionScope::Line(line) => diag_line == line,
            SuppressionScope::Function(func_line) => diag_line >= func_line,
        };

        if !scope_matches {
            continue;
        }

        // Check code match
        if sup.codes.is_empty() {
            return true; // suppress all
        }
        if let Some(code) = diag_code {
            if sup.codes.iter().any(|c| c == code) {
                return true;
            }
        }
    }

    false
}

/// Helper to create a diagnostic with a code.
fn make_diag(
    range: Range,
    severity: DiagnosticSeverity,
    code: &str,
    message: String,
) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(severity),
        source: Some("argsh".to_string()),
        code: Some(NumberOrString::String(code.to_string())),
        message: format!("{}: {}", code, message),
        ..Default::default()
    }
}

fn make_diag_tagged(
    range: Range,
    severity: DiagnosticSeverity,
    code: &str,
    message: String,
    tags: Vec<DiagnosticTag>,
) -> Diagnostic {
    let mut d = make_diag(range, severity, code, message);
    d.tags = Some(tags);
    d
}

// --- Individual checks ---

fn check_args_array_pairing(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    for entry in &func.args_entries {
        if entry.description.is_empty() && !entry.spec.is_empty() && entry.spec != "-" {
            diags.push(make_diag(
                line_range(entry.line),
                DiagnosticSeverity::ERROR,
                codes::AG001,
                format!("args entry '{}' is missing its description", entry.spec),
            ));
        }
    }
}

fn check_usage_array_pairing(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    for entry in &func.usage_entries {
        if entry.description.is_empty() && !entry.is_group_separator {
            diags.push(make_diag(
                line_range(entry.line),
                DiagnosticSeverity::ERROR,
                codes::AG002,
                format!("usage entry '{}' is missing its description", entry.name),
            ));
        }
    }
}

fn check_field_parse_errors(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    for entry in &func.args_entries {
        if let Err(ref msg) = entry.parsed {
            diags.push(make_diag(
                line_range(entry.line),
                DiagnosticSeverity::ERROR,
                codes::AG003,
                format!("invalid field spec '{}': {}", entry.spec, msg),
            ));
        }
    }
}

fn check_missing_variable_declarations(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    let local_names: HashSet<&str> = func.local_vars.iter().map(|v| v.name.as_str()).collect();
    for entry in &func.args_entries {
        if entry.spec == "-" { continue; }
        if let Ok(ref field) = entry.parsed {
            if !local_names.contains(field.name.as_str()) {
                diags.push(make_diag(
                    line_range(entry.line),
                    DiagnosticSeverity::WARNING,
                    codes::AG004,
                    format!("'{}' has no matching 'local {}' declaration", entry.spec, field.name),
                ));
            }
        }
    }
}

fn check_missing_args_call(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    if !func.args_entries.is_empty() && !func.calls_args && !func.calls_usage {
        diags.push(make_diag(
            line_range(func.line),
            DiagnosticSeverity::ERROR,
            codes::AG005,
            format!("function '{}' declares args=() but never calls :args or :usage", func.name),
        ));
    }
}

fn check_missing_usage_call(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    if !func.usage_entries.is_empty() && !func.calls_usage {
        diags.push(make_diag(
            line_range(func.line),
            DiagnosticSeverity::ERROR,
            codes::AG006,
            format!("function '{}' declares usage=() but never calls :usage", func.name),
        ));
    }
}

fn check_usage_function_targets(
    func: &FunctionInfo,
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
    diags: &mut Vec<Diagnostic>,
) {
    let mut known_funcs: HashSet<&str> = analysis.functions.iter().map(|f| f.name.as_str()).collect();
    // Also include imported function names
    for f in &imports.functions {
        known_funcs.insert(&f.name);
    }

    for entry in &func.usage_entries {
        if entry.is_group_separator { continue; }

        let target = if let Some(ref explicit) = entry.explicit_func {
            explicit.clone()
        } else {
            let prefixed = format!("{}::{}", func.name, entry.name);
            if known_funcs.contains(prefixed.as_str()) { continue; }
            if known_funcs.contains(entry.name.as_str()) { continue; }
            let argsh_prefixed = format!("argsh::{}", entry.name);
            if known_funcs.contains(argsh_prefixed.as_str()) { continue; }
            entry.name.clone()
        };

        if !known_funcs.contains(target.as_str()) {
            diags.push(make_diag(
                line_range(func.line),
                DiagnosticSeverity::HINT,
                codes::AG007,
                format!("'{}' → '{}' not found in this file (may be imported)", entry.name, target),
            ));
            if entry.line != func.line && entry.line > 0 {
                diags.push(make_diag_tagged(
                    line_range(entry.line),
                    DiagnosticSeverity::HINT,
                    codes::AG007,
                    format!("'{}' not found in this file", target),
                    vec![DiagnosticTag::UNNECESSARY],
                ));
            }
        }
    }
}

fn check_duplicate_flags(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<String> = HashSet::new();
    for entry in &func.args_entries {
        if entry.spec == "-" { continue; }
        if let Ok(ref field) = entry.parsed {
            if field.is_positional { continue; }
            if !seen.insert(field.display_name.clone()) {
                diags.push(make_diag(
                    line_range(entry.line),
                    DiagnosticSeverity::WARNING,
                    codes::AG008,
                    format!("duplicate flag '--{}' in '{}'", field.display_name, func.name),
                ));
            }
        }
    }
}

fn check_duplicate_short_aliases(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<String> = HashSet::new();
    for entry in &func.args_entries {
        if entry.spec == "-" { continue; }
        if let Ok(ref field) = entry.parsed {
            if let Some(ref short) = field.short {
                if !seen.insert(short.clone()) {
                    diags.push(make_diag(
                        line_range(entry.line),
                        DiagnosticSeverity::WARNING,
                        codes::AG009,
                        format!("duplicate short alias '-{}' in '{}'", short, func.name),
                    ));
                }
            }
        }
    }
}

/// Warn when a usage entry resolves to a bare function name (not namespaced).
/// E.g. 'docs' resolves to `docs()` instead of `main::docs()` — potential collision.
fn check_bare_function_resolution(
    func: &FunctionInfo,
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
    diags: &mut Vec<Diagnostic>,
) {
    let mut known_funcs: HashSet<&str> = analysis.functions.iter().map(|f| f.name.as_str()).collect();
    for f in &imports.functions {
        known_funcs.insert(&f.name);
    }

    for entry in &func.usage_entries {
        if entry.is_group_separator { continue; }
        if entry.explicit_func.is_some() { continue; } // explicit mapping — user's choice

        // Check resolution order: prefixed is preferred, bare is a warning
        let prefixed = format!("{}::{}", func.name, entry.name);
        if known_funcs.contains(prefixed.as_str()) {
            continue; // properly namespaced — all good
        }
        let argsh_prefixed = format!("argsh::{}", entry.name);
        if known_funcs.contains(argsh_prefixed.as_str()) {
            continue; // argsh namespace — fine
        }

        // Resolves to bare function — warn about potential collision
        if known_funcs.contains(entry.name.as_str()) {
            diags.push(make_diag(
                line_range(entry.line),
                DiagnosticSeverity::WARNING,
                codes::AG010,
                format!(
                    "'{}' resolves to bare function '{}()' — consider '{}::{}()' to avoid collisions",
                    entry.name, entry.name, func.name, entry.name
                ),
            ));
        }
    }
}

fn line_range(line: usize) -> Range {
    Range {
        start: Position { line: line as u32, character: 0 },
        end: Position { line: line as u32, character: 999 },
    }
}
