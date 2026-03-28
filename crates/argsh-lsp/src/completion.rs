use tower_lsp::lsp_types::*;

use argsh_syntax::document::DocumentAnalysis;

/// Provide contextual completions based on cursor position and trigger character.
pub fn completions(
    analysis: &DocumentAnalysis,
    position: Position,
    _trigger: Option<&str>,
    content: &str,
) -> Vec<CompletionItem> {
    let line_idx = position.line as usize;
    let col = position.character as usize;
    let lines: Vec<&str> = content.lines().collect();

    if line_idx >= lines.len() {
        return vec![];
    }

    let line = lines[line_idx];
    // Text up to the cursor on this line.
    let prefix = if col <= line.len() {
        &line[..col]
    } else {
        line
    };

    let ctx = detect_context(analysis, line_idx, prefix, &lines);

    match ctx {
        Context::ArgsModifier => complete_args_modifiers(),
        Context::ArgsType => complete_args_types(analysis, content),
        Context::UsageAnnotation => complete_usage_annotations(),
        Context::UsageFuncMapping => complete_function_names(analysis),
        Context::ImportKeyword => complete_import_modules(),
        Context::UsageCommandName => complete_function_names(analysis),
        Context::None => vec![],
    }
}

#[derive(Debug)]
enum Context {
    /// Inside an args array entry, after a `:` (suggest modifiers).
    ArgsModifier,
    /// Inside an args array entry, after `:~` (suggest type names).
    ArgsType,
    /// Inside a usage array entry, after `@` (suggest annotations).
    UsageAnnotation,
    /// Inside a usage array entry, after `:-` (suggest function names).
    UsageFuncMapping,
    /// After `import` keyword.
    ImportKeyword,
    /// Inside a usage array, at command name position.
    UsageCommandName,
    /// No recognizable context.
    None,
}

/// Determine what kind of completion context the cursor is in.
fn detect_context(
    analysis: &DocumentAnalysis,
    line_idx: usize,
    prefix: &str,
    lines: &[&str],
) -> Context {
    let trimmed = prefix.trim_start();

    // Check if on an import line
    if trimmed.starts_with("import ") {
        return Context::ImportKeyword;
    }

    // Determine if we are inside an args=(...) or usage=(...) block
    let array_kind = find_enclosing_array(lines, line_idx);

    match array_kind {
        Some(ArrayKind::Args) => {
            // Check what's immediately before the cursor
            if prefix.ends_with(":~") || prefix.ends_with("~") {
                return Context::ArgsType;
            }
            if prefix.ends_with(':') {
                return Context::ArgsModifier;
            }
            // If we're in a quoted string and last non-ws before cursor has a ':'
            if is_inside_quote(prefix) {
                let in_spec = extract_current_spec(prefix);
                if in_spec.contains(":~") {
                    // After :~<partial>, still suggest types
                    return Context::ArgsType;
                }
                if in_spec.contains(':') {
                    return Context::ArgsModifier;
                }
            }
            Context::None
        }
        Some(ArrayKind::Usage) => {
            if prefix.ends_with(":-") {
                return Context::UsageFuncMapping;
            }
            if prefix.ends_with('@') {
                return Context::UsageAnnotation;
            }
            // Check if at start of a new entry (command name position)
            if is_inside_quote(prefix) {
                let spec = extract_current_spec(prefix);
                if spec.contains(":-") {
                    return Context::UsageFuncMapping;
                }
                if spec.contains('@') {
                    return Context::UsageAnnotation;
                }
                // At the start of a fresh entry
                if !spec.contains('|') && !spec.contains(':') && !spec.contains('@') {
                    return Context::UsageCommandName;
                }
            }
            Context::None
        }
        _ => {
            // Check if the line references a function pattern like 'func::name'
            // or we are inside an args/usage array on the same line
            let _ = analysis;
            Context::None
        }
    }
}

#[derive(Debug, PartialEq)]
enum ArrayKind {
    Args,
    Usage,
}

/// Walk backwards from `line_idx` to find if we are inside an `args=(` or `usage=(` block.
fn find_enclosing_array(lines: &[&str], line_idx: usize) -> Option<ArrayKind> {
    // Search backwards for an opening array pattern without a matching close
    let mut paren_depth: i32 = 0;

    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();

        // Count parens on this line (rough — does not handle strings)
        for ch in trimmed.chars() {
            match ch {
                ')' => paren_depth += 1,
                '(' => paren_depth -= 1,
                _ => {}
            }
        }

        if paren_depth < 0 {
            // We found an unmatched opening paren — check if it's args or usage
            if trimmed.contains("args=(") || trimmed.ends_with("args=(") {
                return Some(ArrayKind::Args);
            }
            if trimmed.contains("usage=(") || trimmed.ends_with("usage=(") {
                return Some(ArrayKind::Usage);
            }
            // Some other array
            return None;
        }
    }

    None
}

/// Check if the cursor is currently inside a quote (single or double).
fn is_inside_quote(prefix: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut prev = '\0';

    for ch in prefix.chars() {
        match ch {
            '\'' if !in_double && prev != '\\' => in_single = !in_single,
            '"' if !in_single && prev != '\\' => in_double = !in_double,
            _ => {}
        }
        prev = ch;
    }

    in_single || in_double
}

/// Extract the spec string being edited (text after the last opening quote).
fn extract_current_spec(prefix: &str) -> &str {
    // Find last unmatched quote
    if let Some(pos) = prefix.rfind('\'') {
        return &prefix[pos + 1..];
    }
    if let Some(pos) = prefix.rfind('"') {
        return &prefix[pos + 1..];
    }
    ""
}

fn complete_args_modifiers() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: ":+".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Boolean flag (takes no value)".to_string()),
            insert_text: Some("+".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":~int".to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some("Integer type".to_string()),
            insert_text: Some("~int".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":~float".to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some("Float type".to_string()),
            insert_text: Some("~float".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":~file".to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some("File path type".to_string()),
            insert_text: Some("~file".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":~boolean".to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some("Boolean type".to_string()),
            insert_text: Some("~boolean".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":~string".to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some("String type (default)".to_string()),
            insert_text: Some("~string".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":~stdin".to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some("Read from stdin".to_string()),
            insert_text: Some("~stdin".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":!".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Required field".to_string()),
            insert_text: Some("!".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: ":#".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Hidden from help text".to_string()),
            insert_text: Some("#".to_string()),
            ..Default::default()
        },
    ]
}

fn complete_args_types(analysis: &DocumentAnalysis, content: &str) -> Vec<CompletionItem> {
    let mut items = vec![
        make_type_item("int", "Integer type"),
        make_type_item("float", "Float type"),
        make_type_item("file", "File path type"),
        make_type_item("boolean", "Boolean type"),
        make_type_item("string", "String type"),
        make_type_item("stdin", "Read from stdin"),
    ];

    // Add custom `to::` function names found in the file as custom type validators.
    let re = regex::Regex::new(r"(?m)^\s*(to::\w[\w:]*)\s*\(\)").unwrap();
    for cap in re.captures_iter(content) {
        let fname = cap.get(1).unwrap().as_str();
        // Strip the `to::` prefix for the type name
        let tname = fname.strip_prefix("to::").unwrap_or(fname);
        items.push(make_type_item(
            tname,
            &format!("Custom validator: {}", fname),
        ));
    }

    // Also check analysis functions for to:: prefixed ones
    for func in &analysis.functions {
        if func.name.starts_with("to::") {
            let tname = func.name.strip_prefix("to::").unwrap_or(&func.name);
            // Avoid duplicates
            if !items.iter().any(|i| i.label == tname) {
                items.push(make_type_item(
                    tname,
                    &format!("Custom validator: {}", func.name),
                ));
            }
        }
    }

    items
}

fn make_type_item(name: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::TYPE_PARAMETER),
        detail: Some(detail.to_string()),
        ..Default::default()
    }
}

fn complete_usage_annotations() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "readonly".to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("MCP readOnlyHint: safe to auto-run".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "destructive".to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("MCP destructiveHint: may cause irreversible changes".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "json".to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("Output format: JSON".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "idempotent".to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("MCP idempotentHint: repeated calls have same effect".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "openworld".to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("MCP openWorldHint: interacts with external entities".to_string()),
            ..Default::default()
        },
    ]
}

fn complete_function_names(analysis: &DocumentAnalysis) -> Vec<CompletionItem> {
    analysis
        .functions
        .iter()
        .map(|f| CompletionItem {
            label: f.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: f.title.clone(),
            ..Default::default()
        })
        .collect()
}

fn complete_import_modules() -> Vec<CompletionItem> {
    let modules = [
        ("string", "String manipulation utilities"),
        ("array", "Array manipulation utilities"),
        ("fmt", "Formatting utilities"),
        ("error", "Error handling utilities"),
        ("is", "Type checking predicates"),
        ("to", "Type conversion functions"),
        ("binary", "Binary data utilities"),
        ("docker", "Docker helper functions"),
        ("github", "GitHub API utilities"),
        ("bash", "Bash compatibility utilities"),
    ];

    modules
        .iter()
        .map(|(name, desc)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(desc.to_string()),
            ..Default::default()
        })
        .collect()
}
