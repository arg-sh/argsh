use tower_lsp::lsp_types::*;

use argsh_syntax::document::DocumentAnalysis;

pub fn code_lenses(analysis: &DocumentAnalysis, uri: &Url) -> Vec<CodeLens> {
    let mut lenses = Vec::new();

    for func in &analysis.functions {
        let mut parts = Vec::new();

        if func.calls_args && !func.args_entries.is_empty() {
            let flag_count = func
                .args_entries
                .iter()
                .filter(|e| e.spec != "-")
                .count();
            let req_count = func
                .args_entries
                .iter()
                .filter(|e| e.parsed.as_ref().map(|f| f.required).unwrap_or(false))
                .count();
            if req_count > 0 {
                parts.push(format!("{} flags ({} required)", flag_count, req_count));
            } else {
                parts.push(format!("{} flags", flag_count));
            }
        }

        if func.calls_usage && !func.usage_entries.is_empty() {
            let cmd_count = func
                .usage_entries
                .iter()
                .filter(|e| !e.is_group_separator)
                .count();
            parts.push(format!("{} subcommands", cmd_count));
        }

        if parts.is_empty() {
            continue;
        }

        let title = format!("$(info) {}", parts.join(" · "));

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
