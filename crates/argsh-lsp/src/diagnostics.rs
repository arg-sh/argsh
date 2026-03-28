use std::collections::HashSet;

use tower_lsp::lsp_types::*;

use argsh_syntax::document::{DocumentAnalysis, FunctionInfo};

/// Generate LSP diagnostics from a document analysis.
pub fn generate_diagnostics(analysis: &DocumentAnalysis) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for func in &analysis.functions {
        check_args_array_pairing(func, &mut diags);
        check_usage_array_pairing(func, &mut diags);
        check_field_parse_errors(func, &mut diags);
        check_missing_variable_declarations(func, &mut diags);
        check_missing_args_call(func, &mut diags);
        check_missing_usage_call(func, &mut diags);
        check_usage_function_targets(func, analysis, &mut diags);
        check_duplicate_flags(func, &mut diags);
        check_duplicate_short_aliases(func, &mut diags);
    }

    diags
}

/// Args array entries must come in pairs (spec + description).
/// An odd count means a description is missing.
fn check_args_array_pairing(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    // ArgsArrayEntry is already parsed as pairs by the syntax crate,
    // so we cannot detect odd counts from the entries themselves.
    // However, if the syntax crate's tokenizer found an odd count,
    // the last token would be dropped. We check for entries where
    // the description is empty as a proxy.
    for entry in &func.args_entries {
        if entry.description.is_empty() && !entry.spec.is_empty() && entry.spec != "-" {
            diags.push(Diagnostic {
                range: line_range(entry.line),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("argsh".to_string()),
                message: format!(
                    "args entry '{}' is missing its description (entries must be paired: spec + description)",
                    entry.spec
                ),
                ..Default::default()
            });
        }
    }
}

/// Usage array entries must come in pairs (spec + description).
fn check_usage_array_pairing(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    for entry in &func.usage_entries {
        if entry.description.is_empty() && !entry.is_group_separator {
            diags.push(Diagnostic {
                range: line_range(func.line),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("argsh".to_string()),
                message: format!(
                    "usage entry '{}' is missing its description (entries must be paired: name + description)",
                    entry.name
                ),
                ..Default::default()
            });
        }
    }
}

/// Report parse errors from invalid field modifiers.
fn check_field_parse_errors(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    for entry in &func.args_entries {
        if let Err(ref msg) = entry.parsed {
            diags.push(Diagnostic {
                range: line_range(entry.line),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("argsh".to_string()),
                message: format!("invalid field spec '{}': {}", entry.spec, msg),
                ..Default::default()
            });
        }
    }
}

/// Each args entry should have a corresponding `local <name>` declaration.
fn check_missing_variable_declarations(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    let local_names: HashSet<&str> = func.local_vars.iter().map(|v| v.name.as_str()).collect();

    for entry in &func.args_entries {
        if entry.spec == "-" {
            continue; // group separator
        }
        if let Ok(ref field) = entry.parsed {
            if !local_names.contains(field.name.as_str()) {
                diags.push(Diagnostic {
                    range: line_range(entry.line),
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("argsh".to_string()),
                    message: format!(
                        "args field '{}' has no matching 'local {}' declaration in function '{}'",
                        entry.spec, field.name, func.name
                    ),
                    ..Default::default()
                });
            }
        }
    }
}

/// Args array declared but no `:args` call in the function body.
fn check_missing_args_call(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    if !func.args_entries.is_empty() && !func.calls_args {
        // Only warn if there is no :usage call either (some patterns use :usage with args)
        if !func.calls_usage {
            diags.push(Diagnostic {
                range: line_range(func.line),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("argsh".to_string()),
                message: format!(
                    "function '{}' declares args=(...) but never calls :args or :usage",
                    func.name
                ),
                ..Default::default()
            });
        }
    }
}

/// Usage array declared but no `:usage` call in the function body.
fn check_missing_usage_call(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    if !func.usage_entries.is_empty() && !func.calls_usage {
        diags.push(Diagnostic {
            range: line_range(func.line),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("argsh".to_string()),
            message: format!(
                "function '{}' declares usage=(...) but never calls :usage",
                func.name
            ),
            ..Default::default()
        });
    }
}

/// Usage entry references a function via `:-func` that does not exist in the document.
fn check_usage_function_targets(
    func: &FunctionInfo,
    analysis: &DocumentAnalysis,
    diags: &mut Vec<Diagnostic>,
) {
    let known_funcs: HashSet<&str> = analysis.functions.iter().map(|f| f.name.as_str()).collect();

    for entry in &func.usage_entries {
        if entry.is_group_separator {
            continue;
        }

        // Determine the target function name
        let target = if let Some(ref explicit) = entry.explicit_func {
            explicit.clone()
        } else {
            // Implicit resolution: try caller::name, then bare name
            let prefixed = format!("{}::{}", func.name, entry.name);
            if known_funcs.contains(prefixed.as_str()) {
                continue; // Found via prefix — all good
            }
            if known_funcs.contains(entry.name.as_str()) {
                continue; // Found as bare name — all good
            }
            // Also check argsh:: prefix
            let argsh_prefixed = format!("argsh::{}", entry.name);
            if known_funcs.contains(argsh_prefixed.as_str()) {
                continue;
            }
            entry.name.clone()
        };

        if !known_funcs.contains(target.as_str()) {
            diags.push(Diagnostic {
                range: line_range(func.line),
                severity: Some(DiagnosticSeverity::HINT),
                source: Some("argsh".to_string()),
                message: format!(
                    "usage entry '{}' → function '{}' not found in this file (may be imported or defined elsewhere)",
                    entry.name, target
                ),
                ..Default::default()
            });
        }
    }
}

/// Duplicate long flag names in the args array.
fn check_duplicate_flags(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<String> = HashSet::new();

    for entry in &func.args_entries {
        if entry.spec == "-" {
            continue;
        }
        if let Ok(ref field) = entry.parsed {
            if field.is_positional {
                continue;
            }
            if !seen.insert(field.display_name.clone()) {
                diags.push(Diagnostic {
                    range: line_range(entry.line),
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("argsh".to_string()),
                    message: format!(
                        "duplicate flag '--{}' in args array of function '{}'",
                        field.display_name, func.name
                    ),
                    ..Default::default()
                });
            }
        }
    }
}

/// Duplicate short aliases in the args array.
fn check_duplicate_short_aliases(func: &FunctionInfo, diags: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<String> = HashSet::new();

    for entry in &func.args_entries {
        if entry.spec == "-" {
            continue;
        }
        if let Ok(ref field) = entry.parsed {
            if let Some(ref short) = field.short {
                if !short.is_empty() && !seen.insert(short.clone()) {
                    diags.push(Diagnostic {
                        range: line_range(entry.line),
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("argsh".to_string()),
                        message: format!(
                            "duplicate short alias '-{}' in args array of function '{}'",
                            short, func.name
                        ),
                        ..Default::default()
                    });
                }
            }
        }
    }
}

/// Create a range covering the entire line at a 0-based line number.
fn line_range(line: usize) -> Range {
    Range {
        start: Position {
            line: line as u32,
            character: 0,
        },
        end: Position {
            line: line as u32,
            character: u32::MAX,
        },
    }
}
