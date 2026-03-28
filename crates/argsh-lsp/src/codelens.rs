use tower_lsp::lsp_types::*;

use argsh_syntax::document::DocumentAnalysis;

pub fn code_lenses(analysis: &DocumentAnalysis, uri: &Url) -> Vec<CodeLens> {
    let mut lenses = Vec::new();

    // Build a set of function names that have :usage (dispatchers/branches)
    let dispatcher_names: Vec<&str> = analysis
        .functions
        .iter()
        .filter(|f| f.calls_usage && !f.usage_entries.is_empty())
        .map(|f| f.name.as_str())
        .collect();

    for func in &analysis.functions {
        let has_args = func.calls_args && !func.args_entries.is_empty();
        let has_usage = func.calls_usage && !func.usage_entries.is_empty();

        if !has_args && !has_usage {
            continue;
        }

        // Determine if this is a leaf (has :args, no :usage) or branch (has :usage)
        let icon = if has_usage { "$(git-merge)" } else { "$(terminal)" };

        let mut parts = Vec::new();

        // Show "argsh:" prefix for brand consistency
        parts.push("argsh:".to_string());

        if has_args {
            let flag_count = func
                .args_entries
                .iter()
                .filter(|e| e.spec != "-")
                .count();
            let positional_count = func
                .args_entries
                .iter()
                .filter(|e| e.parsed.as_ref().map(|f| f.is_positional).unwrap_or(false))
                .count();
            let option_count = flag_count - positional_count;
            let req_count = func
                .args_entries
                .iter()
                .filter(|e| e.parsed.as_ref().map(|f| f.required).unwrap_or(false))
                .count();

            let mut flag_parts = Vec::new();
            if positional_count > 0 {
                flag_parts.push(format!("{} param{}", positional_count, if positional_count == 1 { "" } else { "s" }));
            }
            if option_count > 0 {
                let mut s = format!("{} flag{}", option_count, if option_count == 1 { "" } else { "s" });
                if req_count > 0 {
                    s.push_str(&format!(" ({} required)", req_count));
                }
                flag_parts.push(s);
            }
            if !flag_parts.is_empty() {
                parts.push(flag_parts.join(", "));
            }
        }

        if has_usage {
            let cmd_count = func
                .usage_entries
                .iter()
                .filter(|e| !e.is_group_separator)
                .count();
            parts.push(format!("{} subcommand{}", cmd_count, if cmd_count == 1 { "" } else { "s" }));
        }

        // Find parent function (who dispatches to this one via :usage)
        let parent = find_parent_dispatcher(analysis, &func.name, &dispatcher_names);
        if let Some(parent_name) = parent {
            parts.push(format!("← {}", parent_name));
        }

        let title = format!("{} {}", icon, parts.join(" · "));

        lenses.push(CodeLens {
            range: Range {
                start: Position {
                    line: func.line as u32,
                    character: 0,
                },
                end: Position {
                    line: func.line as u32,
                    character: 0,
                },
            },
            command: Some(Command {
                title,
                command: "argsh.showPreview".to_string(),
                arguments: Some(vec![serde_json::json!(uri.to_string())]),
            }),
            data: None,
        });
    }

    lenses
}

/// Find which dispatcher function would route to `func_name`.
/// Checks :usage entries for matching command names using the :: namespace convention.
fn find_parent_dispatcher<'a>(
    analysis: &'a DocumentAnalysis,
    func_name: &str,
    _dispatcher_names: &[&str],
) -> Option<&'a str> {
    // Check if func_name matches a usage entry in any dispatcher function
    for parent in &analysis.functions {
        if !parent.calls_usage || parent.usage_entries.is_empty() {
            continue;
        }
        for entry in &parent.usage_entries {
            if entry.is_group_separator {
                continue;
            }
            // Explicit mapping: :-func_name
            if let Some(ref target) = entry.explicit_func {
                if target == func_name {
                    return Some(&parent.name);
                }
            }
            // Implicit: parent::cmd or bare cmd
            let prefixed = format!("{}::{}", parent.name, entry.name);
            if prefixed == func_name || entry.name == func_name {
                return Some(&parent.name);
            }
        }
    }
    None
}
