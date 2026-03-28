use tower_lsp::lsp_types::*;

use argsh_syntax::document::{DocumentAnalysis, FunctionInfo};

/// Provide hover information for the symbol under the cursor.
pub fn hover(
    analysis: &DocumentAnalysis,
    position: Position,
    content: &str,
) -> Option<Hover> {
    let line_idx = position.line as usize;
    let col = position.character as usize;
    let lines: Vec<&str> = content.lines().collect();

    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];

    // 1. Hover on :args or :usage call
    if let Some(h) = hover_builtin_call(analysis, line, line_idx) {
        return Some(h);
    }

    // 2. Hover on a modifier character
    if let Some(h) = hover_modifier(line, col, &lines, line_idx) {
        return Some(h);
    }

    // 3. Hover on an annotation
    if let Some(h) = hover_annotation(line, col) {
        return Some(h);
    }

    // 4. Hover on a function name
    let word = extract_word_at(line, col);
    if !word.is_empty() {
        // Check if it's a function
        if let Some(h) = hover_function(analysis, &word) {
            return Some(h);
        }

        // Check if it's an args entry spec
        if let Some(h) = hover_args_entry(analysis, &word, line_idx) {
            return Some(h);
        }

        // Check if it's a usage entry name
        if let Some(h) = hover_usage_entry(analysis, &word, line_idx) {
            return Some(h);
        }
    }

    None
}

/// Hover on `:args` or `:usage` calls.
fn hover_builtin_call(
    analysis: &DocumentAnalysis,
    line: &str,
    line_idx: usize,
) -> Option<Hover> {
    let trimmed = line.trim();

    if trimmed.starts_with(":args") || trimmed.starts_with(":usage") {
        // Find which function this line belongs to
        let func = analysis
            .functions
            .iter()
            .find(|f| line_idx >= f.line && line_idx <= f.end_line)?;

        let is_args = trimmed.starts_with(":args");
        let count = if is_args {
            func.args_entries.len()
        } else {
            func.usage_entries.len()
        };
        let kind = if is_args { "flags/args" } else { "subcommands" };

        let mut md = format!("**{}** in `{}`\n\n", if is_args { ":args" } else { ":usage" }, func.name);
        md.push_str(&format!("{} {} defined", count, kind));

        if let Some(ref title) = func.title {
            md.push_str(&format!("\n\n> {}", title));
        }

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        });
    }

    None
}

/// Hover on modifier characters like `:+`, `:~`, `:!`, `:#`.
fn hover_modifier(
    line: &str,
    col: usize,
    lines: &[&str],
    line_idx: usize,
) -> Option<Hover> {
    // Only relevant inside args arrays
    let array_kind = find_enclosing_array(lines, line_idx);
    if array_kind.as_deref() != Some("args") {
        return None;
    }

    if col >= line.len() {
        return None;
    }

    // Look at the character at cursor and one before
    let ch = line.as_bytes().get(col).copied().map(|b| b as char)?;
    let prev = if col > 0 {
        line.as_bytes().get(col - 1).copied().map(|b| b as char)
    } else {
        None
    };

    let doc = match (prev, ch) {
        (Some(':'), '+') | (_, '+') if is_after_colon_in_spec(line, col) => {
            Some(("`:+` Boolean flag", "Flag takes no value. Variable is set to `true` when the flag is present, empty otherwise."))
        }
        (Some(':'), '~') => {
            Some(("`:~type` Typed parameter", "Specifies a type validator. Built-in types: `int`, `float`, `file`, `boolean`, `string`, `stdin`. Custom types use `to::name` functions."))
        }
        (Some(':'), '!') | (_, '!') if is_after_colon_in_spec(line, col) => {
            Some(("`:!` Required field", "The argument must be provided. An error is raised if it is missing."))
        }
        (Some(':'), '#') | (_, '#') if is_after_colon_in_spec(line, col) => {
            Some(("`:# ` Hidden field", "The field is hidden from help text output but still functional."))
        }
        _ => None,
    };

    doc.map(|(title, desc)| Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("**{}**\n\n{}", title, desc),
        }),
        range: None,
    })
}

/// Check if the character at `col` is in the modifier section of a field spec.
fn is_after_colon_in_spec(line: &str, col: usize) -> bool {
    // Walk backwards from col to find if there's a ':' before us in a quoted context
    let prefix = &line[..col];
    // Check we're inside a quote
    let mut in_single = false;
    let mut in_double = false;
    let mut last_colon = false;
    for ch in prefix.chars() {
        match ch {
            '\'' => in_single = !in_single,
            '"' => in_double = !in_double,
            ':' if in_single || in_double => last_colon = true,
            _ => {}
        }
    }
    last_colon
}

/// Hover on annotations like `@readonly`, `@destructive`.
fn hover_annotation(line: &str, col: usize) -> Option<Hover> {
    // Check if cursor is on or after an @ sign
    if col >= line.len() {
        return None;
    }

    // Find the @ and annotation word
    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 && bytes[start - 1] != b'@' && (bytes[start - 1] as char).is_ascii_alphanumeric() {
        start -= 1;
    }
    if start > 0 && bytes[start - 1] == b'@' {
        start -= 1;
    } else if bytes.get(col).copied() == Some(b'@') {
        start = col;
    } else {
        return None;
    }

    if bytes.get(start).copied() != Some(b'@') {
        return None;
    }

    // Extract annotation name
    let after_at = start + 1;
    let mut end = after_at;
    while end < bytes.len() && (bytes[end] as char).is_ascii_alphanumeric() {
        end += 1;
    }

    let annotation = &line[after_at..end];

    let doc = match annotation {
        "readonly" => Some((
            "@readonly",
            "MCP readOnlyHint: This command only reads data and is safe to auto-run.",
        )),
        "destructive" => Some((
            "@destructive",
            "MCP destructiveHint: This command may cause irreversible changes. Requires confirmation.",
        )),
        "json" => Some((
            "@json",
            "Output format hint: This command produces JSON output.",
        )),
        "idempotent" => Some((
            "@idempotent",
            "MCP idempotentHint: Repeated calls with the same arguments have the same effect as a single call.",
        )),
        "openworld" => Some((
            "@openworld",
            "MCP openWorldHint: This command interacts with external entities (network, third-party APIs).",
        )),
        _ => None,
    };

    doc.map(|(title, desc)| Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("**{}**\n\n{}", title, desc),
        }),
        range: None,
    })
}

/// Hover on a function name: show signature, flags, and title with a help preview.
fn hover_function(analysis: &DocumentAnalysis, name: &str) -> Option<Hover> {
    let func = analysis.functions.iter().find(|f| f.name == name)?;
    let md = render_help_preview(func, analysis);

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

/// Render a help preview for a function, mimicking `--help` output.
///
/// For `:args` functions, shows flags/options in a table-like format.
/// For `:usage` functions, shows subcommands.
fn render_help_preview(func: &FunctionInfo, _analysis: &DocumentAnalysis) -> String {
    let title = func.title.as_deref().unwrap_or("");
    let mut md = format!("**{}**", func.name);
    if !title.is_empty() {
        md.push_str(&format!(" — {}", title));
    }
    md.push('\n');

    // Usage line
    if func.calls_usage && !func.usage_entries.is_empty() {
        md.push_str(&format!("\n```\nUsage: {} <command> [args]\n```\n", func.name));
    } else if func.calls_args {
        // Build usage synopsis from args
        let mut synopsis = format!("Usage: {}", func.name);
        let mut has_options = false;
        for entry in &func.args_entries {
            if entry.spec == "-" {
                continue;
            }
            if let Ok(ref field) = entry.parsed {
                if field.is_positional {
                    if field.required {
                        synopsis.push_str(&format!(" <{}>", field.display_name));
                    } else {
                        synopsis.push_str(&format!(" [{}]", field.display_name));
                    }
                } else {
                    has_options = true;
                }
            }
        }
        if has_options {
            synopsis.push_str(" [options]");
        }
        md.push_str(&format!("\n```\n{}\n```\n", synopsis));
    }

    // Subcommands section
    if !func.usage_entries.is_empty() {
        let visible: Vec<_> = func
            .usage_entries
            .iter()
            .filter(|e| !e.is_group_separator && !e.hidden)
            .collect();
        if !visible.is_empty() {
            md.push_str("\n**Commands:**\n\n");
            // Find max name width for alignment
            let max_width = visible
                .iter()
                .map(|e| e.name.len())
                .max()
                .unwrap_or(0)
                .max(4);
            for entry in &visible {
                let padded = format!("{:<width$}", entry.name, width = max_width);
                md.push_str(&format!("    {}    {}\n", padded, entry.description));
            }
        }

        let subcmd_count = func
            .usage_entries
            .iter()
            .filter(|e| !e.is_group_separator)
            .count();
        md.push_str(&format!("\n---\n*{} subcommand{}*\n", subcmd_count, if subcmd_count == 1 { "" } else { "s" }));
    }

    // Flags/options section
    if !func.args_entries.is_empty() {
        let flags: Vec<_> = func
            .args_entries
            .iter()
            .filter(|e| e.spec != "-" && e.parsed.is_ok())
            .collect();
        if !flags.is_empty() {
            md.push_str("\n**Options:**\n\n");
            for entry in &flags {
                if let Ok(ref field) = entry.parsed {
                    let type_str = if field.is_boolean {
                        String::new()
                    } else {
                        format!(" {}", field.type_name)
                    };

                    let flag_str = if field.is_positional {
                        format!("  <{}>", field.display_name)
                    } else if let Some(ref short) = field.short {
                        format!("  -{}, --{}{}", short, field.display_name, type_str)
                    } else {
                        format!("      --{}{}", field.display_name, type_str)
                    };

                    let desc = &entry.description;
                    md.push_str(&format!("    {}    {}\n", flag_str, desc));
                }
            }
        }

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
        md.push_str(&format!(
            "\n---\n*{} flag{}, {} required*\n",
            flag_count,
            if flag_count == 1 { "" } else { "s" },
            req_count
        ));
    }

    md
}

/// Hover on an args entry spec string.
fn hover_args_entry(
    analysis: &DocumentAnalysis,
    word: &str,
    line_idx: usize,
) -> Option<Hover> {
    // Find the function containing this line
    let func = analysis
        .functions
        .iter()
        .find(|f| line_idx >= f.line && line_idx <= f.end_line)?;

    // Find the matching args entry
    let entry = func
        .args_entries
        .iter()
        .find(|e| e.spec == word || e.spec.starts_with(&format!("{}|", word)) || e.spec.starts_with(&format!("{}:", word)))?;

    let Ok(ref field) = entry.parsed else {
        return None;
    };

    let type_str = if field.is_boolean {
        "boolean".to_string()
    } else {
        field.type_name.clone()
    };

    // Build the flag header like: **--port, -p** `int`
    let flag_header = if field.is_positional {
        format!("**<{}>**", field.display_name)
    } else if let Some(ref short) = field.short {
        format!("**--{}, -{}**", field.display_name, short)
    } else {
        format!("**--{}**", field.display_name)
    };

    let mut md = format!("{} `{}`\n\n", flag_header, type_str);
    md.push_str(&format!("{}\n", entry.description));
    md.push_str(&format!("\n*Required: {}*", if field.required { "yes" } else { "no" }));
    if field.hidden {
        md.push_str("\n*Hidden: yes*");
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

/// Hover on a usage entry name.
fn hover_usage_entry(
    analysis: &DocumentAnalysis,
    word: &str,
    line_idx: usize,
) -> Option<Hover> {
    let func = analysis
        .functions
        .iter()
        .find(|f| line_idx >= f.line && line_idx <= f.end_line)?;

    let entry = func
        .usage_entries
        .iter()
        .find(|e| e.name == word || e.aliases.contains(&word.to_string()))?;

    let mut md = format!("**subcommand** `{}`\n\n", entry.name);

    if entry.aliases.len() > 1 {
        let aliases: Vec<&str> = entry.aliases.iter().map(|s| s.as_str()).collect();
        md.push_str(&format!("- **Aliases:** {}\n", aliases.join(", ")));
    }

    if let Some(ref target) = entry.explicit_func {
        md.push_str(&format!("- **Target:** `{}`\n", target));
    }

    if entry.hidden {
        md.push_str("- **Hidden**\n");
    }

    if !entry.annotations.is_empty() {
        let anns: Vec<String> = entry.annotations.iter().map(|a| format!("@{}", a)).collect();
        md.push_str(&format!("- **Annotations:** {}\n", anns.join(", ")));
    }

    md.push_str(&format!("\n{}", entry.description));

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

/// Extract word at cursor (same as goto_def).
fn extract_word_at(line: &str, col: usize) -> String {
    if col > line.len() {
        return String::new();
    }
    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 {
        let ch = bytes[start - 1] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' || ch == '|' || ch == '#' {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = col;
    while end < bytes.len() {
        let ch = bytes[end] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' || ch == '|' || ch == '#' || ch == '~' || ch == '+' || ch == '!' {
            end += 1;
        } else {
            break;
        }
    }
    if start < end {
        line[start..end].to_string()
    } else {
        String::new()
    }
}

/// Walk backwards to find if we're inside an args or usage array.
fn find_enclosing_array<'a>(lines: &[&str], line_idx: usize) -> Option<&'a str> {
    let mut paren_depth: i32 = 0;
    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();
        for ch in trimmed.chars() {
            match ch {
                ')' => paren_depth += 1,
                '(' => paren_depth -= 1,
                _ => {}
            }
        }
        if paren_depth < 0 {
            if trimmed.contains("args=(") {
                return Some("args");
            }
            if trimmed.contains("usage=(") {
                return Some("usage");
            }
            return None;
        }
    }
    None
}
