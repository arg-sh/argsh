use tower_lsp::lsp_types::*;

use argsh_syntax::document::DocumentAnalysis;

/// Build document symbols for the outline view.
///
/// Creates a hierarchy where:
/// - Top-level functions are `Function` symbols
/// - Nested functions (via `::` prefix) are children of their parent
/// - Args entries are `Property` children of the function
/// - Usage entries are `Enum` children of the function
#[allow(deprecated)] // DocumentSymbol::deprecated is deprecated but required by the type
pub fn document_symbols(analysis: &DocumentAnalysis) -> Vec<DocumentSymbol> {
    if analysis.functions.is_empty() {
        return Vec::new();
    }

    let mut top_level: Vec<DocumentSymbol> = Vec::new();

    for func in &analysis.functions {
        let mut children = Vec::new();

        // Add args entries as Property children
        for entry in &func.args_entries {
            if entry.spec == "-" {
                continue; // skip group separators
            }
            let detail = if let Ok(ref field) = entry.parsed {
                let type_str = if field.is_boolean {
                    "boolean"
                } else {
                    &field.type_name
                };
                let req = if field.required { ", required" } else { "" };
                Some(format!("{}{}", type_str, req))
            } else {
                Some("parse error".to_string())
            };

            children.push(DocumentSymbol {
                name: entry.spec.clone(),
                detail,
                kind: SymbolKind::PROPERTY,
                tags: None,
                deprecated: None,
                range: line_range(entry.line),
                selection_range: line_range(entry.line),
                children: None,
            });
        }

        // Add usage entries as Enum children
        for entry in &func.usage_entries {
            if entry.is_group_separator {
                continue;
            }
            let detail = if entry.description.is_empty() {
                None
            } else {
                Some(entry.description.clone())
            };

            children.push(DocumentSymbol {
                name: entry.name.clone(),
                detail,
                kind: SymbolKind::ENUM,
                tags: None,
                deprecated: None,
                range: line_range(func.line), // usage entries share the function line range
                selection_range: line_range(func.line),
                children: None,
            });
        }

        let func_detail = func.title.clone();
        let end_line = if func.end_line >= func.line {
            func.end_line as u32
        } else {
            func.line as u32
        };

        let sym = DocumentSymbol {
            name: func.name.clone(),
            detail: func_detail,
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: func.line as u32,
                    character: 0,
                },
                end: Position {
                    line: end_line,
                    character: 999,
                },
            },
            selection_range: Range {
                start: Position {
                    line: func.line as u32,
                    character: 0,
                },
                end: Position {
                    line: func.line as u32,
                    character: func.name.len() as u32,
                },
            },
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        };

        // Try to nest under a parent function based on `::` namespacing
        let nested = try_nest_symbol(&mut top_level, &func.name, sym);
        if !nested {
            // Not nested: it's a top-level function.
            // (We already consumed `sym` in try_nest_symbol, which returned false
            //  meaning it was not consumed — but since we moved it, we need to rebuild.)
        }
    }

    // Second pass: rebuild with nesting awareness.
    // The first-pass approach above has a move issue; let's use a simpler strategy.
    build_nested_symbols(analysis)
}

/// Build the symbol tree with `::` namespace nesting.
#[allow(deprecated)]
fn build_nested_symbols(analysis: &DocumentAnalysis) -> Vec<DocumentSymbol> {
    // Collect all symbols first
    let all_symbols: Vec<(&argsh_syntax::FunctionInfo, DocumentSymbol)> = analysis
        .functions
        .iter()
        .map(|func| {
            let mut children = Vec::new();

            for entry in &func.args_entries {
                if entry.spec == "-" {
                    continue;
                }
                let detail = if let Ok(ref field) = entry.parsed {
                    let type_str = if field.is_boolean {
                        "boolean"
                    } else {
                        &field.type_name
                    };
                    let req = if field.required { ", required" } else { "" };
                    Some(format!("{}{}", type_str, req))
                } else {
                    Some("parse error".to_string())
                };

                children.push(DocumentSymbol {
                    name: entry.spec.clone(),
                    detail,
                    kind: SymbolKind::PROPERTY,
                    tags: None,
                    deprecated: None,
                    range: line_range(entry.line),
                    selection_range: line_range(entry.line),
                    children: None,
                });
            }

            for entry in &func.usage_entries {
                if entry.is_group_separator {
                    continue;
                }
                let detail = if entry.description.is_empty() {
                    None
                } else {
                    Some(entry.description.clone())
                };

                children.push(DocumentSymbol {
                    name: entry.name.clone(),
                    detail,
                    kind: SymbolKind::ENUM,
                    tags: None,
                    deprecated: None,
                    range: line_range(func.line),
                    selection_range: line_range(func.line),
                    children: None,
                });
            }

            // Ensure end_line is never before start line (guards against end_line == 0).
            let end_line = if func.end_line >= func.line {
                func.end_line as u32
            } else {
                func.line as u32
            };

            let sym = DocumentSymbol {
                name: func.name.clone(),
                detail: func.title.clone(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position {
                        line: func.line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: end_line,
                        character: 999,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: func.line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: func.line as u32,
                        character: func.name.len() as u32,
                    },
                },
                children: if children.is_empty() {
                    None
                } else {
                    Some(children)
                },
            };

            (func, sym)
        })
        .collect();

    // Build parent-child relationships based on `::` namespacing
    let func_names: Vec<&str> = analysis.functions.iter().map(|f| f.name.as_str()).collect();
    let mut result: Vec<DocumentSymbol> = Vec::new();
    let mut used: Vec<bool> = vec![false; all_symbols.len()];

    for (i, (_func, _sym)) in all_symbols.iter().enumerate() {
        if used[i] {
            continue;
        }

        let name = &all_symbols[i].0.name;

        // Find children: functions whose name starts with `name::` and where
        // `name` is itself a known function.
        let mut sym = all_symbols[i].1.clone();
        let prefix = format!("{}::", name);

        for (j, (child_func, child_sym)) in all_symbols.iter().enumerate() {
            if i == j || used[j] {
                continue;
            }
            if child_func.name.starts_with(&prefix) {
                // Only direct children (one `::` level deeper)
                let rest = &child_func.name[prefix.len()..];
                if !rest.contains("::") || func_names.iter().any(|n| *n == child_func.name) {
                    let children = sym.children.get_or_insert_with(Vec::new);
                    children.push(child_sym.clone());
                    used[j] = true;
                }
            }
        }

        result.push(sym);
    }

    result
}

#[allow(deprecated)]
fn try_nest_symbol(
    _top_level: &mut [DocumentSymbol],
    _name: &str,
    _sym: DocumentSymbol,
) -> bool {
    // This function is unused in favor of build_nested_symbols.
    false
}

fn line_range(line: usize) -> Range {
    Range {
        start: Position {
            line: line as u32,
            character: 0,
        },
        end: Position {
            line: line as u32,
            character: 999,
        },
    }
}
