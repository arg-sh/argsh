use tower_lsp::lsp_types::*;

use argsh_syntax::document::{ArgsArrayEntry, DocumentAnalysis, FunctionInfo};
use argsh_syntax::field::FieldDef;

use argsh_lsp::resolver::ResolvedImports;

use crate::util::extract_word_at;

/// Format a type string, appending `[]` when the field backs an array variable.
fn format_type(field: &FieldDef, is_array: bool) -> String {
    let base = if field.is_boolean {
        "boolean".to_string()
    } else {
        field.type_name.clone()
    };
    if is_array {
        format!("{}[]", base)
    } else {
        base
    }
}

/// Provide hover information for the symbol under the cursor.
pub fn hover(
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
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
    if let Some(h) = hover_modifier(analysis, line, col, &lines, line_idx) {
        return Some(h);
    }

    // 3. Hover on an annotation
    if let Some(h) = hover_annotation(line, col) {
        return Some(h);
    }

    // 4. Hover inside an args/usage array (single-quoted spec string)
    if let Some(h) = hover_inside_array(analysis, line, line_idx, col) {
        return Some(h);
    }

    // 5. Hover on 'args' or 'usage' variable keyword — show all entries
    let word = extract_word_at(line, col);
    if word == "args" || word == "usage" {
        if let Some(h) = hover_array_overview(analysis, &word, line_idx) {
            return Some(h);
        }
    }

    // 6. Hover on a function name
    if !word.is_empty() {
        // Check if it's a function
        if let Some(h) = hover_function(analysis, imports, &word) {
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
    analysis: &DocumentAnalysis,
    line: &str,
    col: usize,
    lines: &[&str],
    line_idx: usize,
) -> Option<Hover> {
    // Only relevant inside args arrays
    let array_kind = find_enclosing_array(lines, line_idx);
    if array_kind != Some("args") {
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
            // Extract the type name after :~
            let type_name = extract_type_after_tilde(line, col);
            let builtin_types = ["int", "float", "file", "boolean", "string", "stdin"];
            if let Some(ref tname) = type_name {
                if builtin_types.contains(&tname.as_str()) {
                    let desc = match tname.as_str() {
                        "int" => "Validates that the value is an integer",
                        "float" => "Validates that the value is a float",
                        "file" => "Validates that the file path exists",
                        "boolean" => "Converts to boolean (0 or 1)",
                        "string" => "Identity conversion (any string accepted)",
                        "stdin" => "Reads value from stdin if not provided",
                        _ => "",
                    };
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("**`:~{}` Built-in type**\n\n{}", tname, desc),
                        }),
                        range: None,
                    });
                } else {
                    // Custom type — check if to::typename exists
                    let func_name = format!("to::{}", tname);
                    let found = analysis.functions.iter().any(|f| f.name == func_name);
                    let status = if found {
                        format!("Validated by `{}()` *(defined in this file)*", func_name)
                    } else {
                        format!("Validated by `{}()` *(not found in this file — may be imported)*", func_name)
                    };
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("**`:~{}` Custom type**\n\n{}\n\n*Ctrl+Click to go to definition*", tname, status),
                        }),
                        range: None,
                    });
                }
            }
            Some(("`:~type` Typed parameter", "Specifies a type validator. Built-in types: `int`, `float`, `file`, `boolean`, `string`, `stdin`. Custom types use `to::name` functions."))
        }
        (Some(':'), '!') | (_, '!') if is_after_colon_in_spec(line, col) => {
            Some(("`:!` Required field", "The argument must be provided. An error is raised if it is missing."))
        }
        (Some(':'), '#') | (_, '#') if is_after_colon_in_spec(line, col) => {
            Some(("`:#` Hidden field", "The field is hidden from help text output but still functional."))
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
fn hover_function(analysis: &DocumentAnalysis, imports: &ResolvedImports, name: &str) -> Option<Hover> {
    let func = analysis.functions.iter()
        .chain(imports.functions.iter())
        .find(|f| f.name == name)?;
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
                    let type_str = if field.is_boolean && !entry.is_array {
                        String::new()
                    } else {
                        format!(" {}", format_type(field, entry.is_array))
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

/// Hover on the `args` or `usage` variable keyword to show all entries in a table.
fn hover_array_overview(
    analysis: &DocumentAnalysis,
    array_name: &str,
    line_idx: usize,
) -> Option<Hover> {
    // Find which function contains this line
    let func = analysis.functions.iter().find(|f|
        line_idx >= f.line && line_idx <= f.end_line
    )?;

    match array_name {
        "args" if !func.args_entries.is_empty() => {
            let mut md = String::from("### args — Field Definitions\n\n");
            md.push_str("| Field | Type | Description |\n");
            md.push_str("|-------|------|-------------|\n");

            let mut flag_count = 0;
            let mut positional_count = 0;
            let mut req_count = 0;

            for entry in &func.args_entries {
                if entry.spec == "-" { continue; }

                if let Ok(ref field) = entry.parsed {
                    if field.is_positional {
                        positional_count += 1;
                    } else {
                        flag_count += 1;
                    }

                    let flag_str = if let Some(ref short) = field.short {
                        format!("`--{}`, `-{}`", field.display_name, short)
                    } else if field.is_positional {
                        format!("`<{}>`", field.display_name)
                    } else {
                        format!("`--{}`", field.display_name)
                    };

                    let type_str = format!("`{}`", format_type(field, entry.is_array));

                    let mut desc = entry.description.clone();
                    if field.required {
                        desc.push_str(" *(required)*");
                        req_count += 1;
                    }
                    if field.hidden {
                        desc.push_str(" *(hidden)*");
                    }

                    md.push_str(&format!("| {} | {} | {} |\n", flag_str, type_str, desc));
                } else {
                    flag_count += 1;
                    md.push_str(&format!("| `{}` | parse error | {} |\n", entry.spec, entry.description));
                }
            }

            md.push_str("\n---\n*");
            let mut parts = Vec::new();
            if positional_count > 0 {
                parts.push(format!("{} param{}", positional_count, if positional_count == 1 { "" } else { "s" }));
            }
            if flag_count > 0 {
                parts.push(format!("{} flag{}", flag_count, if flag_count == 1 { "" } else { "s" }));
            }
            if parts.is_empty() {
                md.push_str("0 fields");
            } else {
                md.push_str(&parts.join(", "));
            }
            if req_count > 0 {
                md.push_str(&format!(", {} required", req_count));
            }
            md.push('*');

            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            })
        }
        "usage" if !func.usage_entries.is_empty() => {
            let mut md = String::from("### usage — Subcommands\n\n");
            md.push_str("| Command | Description |\n");
            md.push_str("|---------|-------------|\n");

            let mut cmd_count = 0;

            for entry in &func.usage_entries {
                if entry.is_group_separator { continue; }
                cmd_count += 1;

                let name = if !entry.annotations.is_empty() {
                    format!("`{}` {}", entry.name,
                        entry.annotations.iter()
                            .map(|a| format!("@{}", a))
                            .collect::<Vec<_>>()
                            .join(" "))
                } else {
                    format!("`{}`", entry.name)
                };

                let mut desc = entry.description.clone();
                if entry.hidden {
                    desc.push_str(" *(hidden)*");
                }
                if let Some(ref func_name) = entry.explicit_func {
                    desc.push_str(&format!(" → `{}`", func_name));
                }

                md.push_str(&format!("| {} | {} |\n", name, desc));
            }

            md.push_str(&format!("\n---\n*{} subcommands*", cmd_count));

            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            })
        }
        _ => None,
    }
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

    Some(render_args_entry_detail(entry))
}

/// Render hover markdown for a single args entry.
fn render_args_entry_detail(entry: &ArgsArrayEntry) -> Hover {
    let field = match entry.parsed.as_ref() {
        Ok(f) => f,
        Err(e) => {
            return Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "**Parse error** in `{}`\n\n{}",
                        entry.spec, e
                    ),
                }),
                range: None,
            };
        }
    };
    let type_str = format_type(field, entry.is_array);

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
    md.push_str(&format!(
        "\n*Required: {}*",
        if field.required { "yes" } else { "no" }
    ));
    if field.hidden {
        md.push_str("\n*Hidden: yes*");
    }

    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    }
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

/// Check if the cursor is inside a single-quoted string within an args/usage array,
/// and if so, show hover info for that spec entry.
fn hover_inside_array(
    analysis: &DocumentAnalysis,
    line: &str,
    line_idx: usize,
    col: usize,
) -> Option<Hover> {
    // Find enclosing single-quoted string at cursor position
    let spec = extract_single_quoted_at(line, col)?;

    // Find which function this line belongs to
    let func = analysis.functions.iter().find(|f|
        line_idx >= f.line && line_idx <= f.end_line
    )?;

    // Check for group separator '-' — show the heading/description
    if spec == "-" {
        // Find which group separator on this line
        // Check args entries first
        for (i, entry) in func.args_entries.iter().enumerate() {
            if entry.spec == "-" && entry.line == line_idx {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("### Group separator\n\n**{}**\n\n*Groups the following flags under this heading in `--help` output*", entry.description),
                    }),
                    range: None,
                });
            }
            // Match by position if lines aren't precise
            if entry.spec == "-" && i < func.args_entries.len() {
                // Check if this is roughly the right separator
            }
        }
        // Check usage entries
        for entry in &func.usage_entries {
            if entry.is_group_separator && entry.line == line_idx {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("### Group separator\n\n**{}**\n\n*Groups the following subcommands under this heading in `--help` output*", entry.description),
                    }),
                    range: None,
                });
            }
        }
        // Fallback: find the closest separator description
        let all_seps: Vec<&str> = func.usage_entries.iter()
            .filter(|e| e.is_group_separator)
            .map(|e| e.description.as_str())
            .chain(func.args_entries.iter().filter(|e| e.spec == "-").map(|e| e.description.as_str()))
            .collect();
        if !all_seps.is_empty() {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("### Group separator\n\nGroup headings: {}\n\n*Used to organize `--help` output into sections*",
                        all_seps.iter().map(|s| format!("**{}**", s)).collect::<Vec<_>>().join(", ")),
                }),
                range: None,
            });
        }
    }

    // Try as args entry
    for entry in &func.args_entries {
        if entry.spec == spec {
            if entry.parsed.is_err() {
                continue;
            }
            return Some(render_args_entry_detail(entry));
        }
    }

    // Try as usage entry — show target function's full help preview
    for entry in &func.usage_entries {
        if entry.is_group_separator {
            continue;
        }
        if spec.contains(&entry.name) {
            let mut md = format!("### `{}`", entry.name);
            if !entry.description.is_empty() {
                md.push_str(&format!(" — {}", entry.description));
            }
            md.push('\n');

            if !entry.annotations.is_empty() {
                let badges: Vec<String> = entry.annotations.iter()
                    .map(|a| format!("`@{}`", a))
                    .collect();
                md.push_str(&format!("\n{}\n", badges.join(" ")));
            }

            if let Some(ref target) = entry.explicit_func {
                md.push_str(&format!("\n*→ `{}`*\n", target));
            }

            if entry.hidden {
                md.push_str("\n*Hidden from `--help`*\n");
            }

            // Find the target function and show its flags/subcommands
            let target_name = entry.explicit_func.as_deref().unwrap_or(&entry.name);
            let target_func = analysis.functions.iter().find(|f| {
                f.name == target_name
                    || f.name == format!("{}::{}", func.name, entry.name)
                    || f.name.ends_with(&format!("::{}", entry.name))
            });

            if let Some(target) = target_func {
                if let Some(ref title) = target.title {
                    if title != &entry.description {
                        md.push_str(&format!("\n> {}\n", title));
                    }
                }

                // Show flags
                let flags: Vec<_> = target.args_entries.iter()
                    .filter(|e| e.spec != "-" && e.parsed.is_ok())
                    .collect();
                if !flags.is_empty() {
                    md.push_str("\n**Options:**\n\n");
                    for flag_entry in &flags {
                        if let Ok(ref field) = flag_entry.parsed {
                            let type_str = format_type(field, flag_entry.is_array);
                            let flag_str = if field.is_positional {
                                format!("`<{}>`", field.display_name)
                            } else if let Some(ref short) = field.short {
                                format!("`--{}`, `-{}`", field.display_name, short)
                            } else {
                                format!("`--{}`", field.display_name)
                            };
                            let req = if field.required { " *(required)*" } else { "" };
                            md.push_str(&format!("- {} `{}` — {}{}\n",
                                flag_str, type_str, flag_entry.description, req));
                        }
                    }
                }

                // Show nested subcommands
                let cmds: Vec<_> = target.usage_entries.iter()
                    .filter(|e| !e.is_group_separator && !e.hidden)
                    .collect();
                if !cmds.is_empty() {
                    md.push_str("\n**Subcommands:**\n\n");
                    for cmd in &cmds {
                        md.push_str(&format!("- `{}` — {}\n", cmd.name, cmd.description));
                    }
                }
            }

            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            });
        }
    }

    None
}

/// Extract the content of the single-quoted string that contains the cursor.
fn extract_single_quoted_at(line: &str, col: usize) -> Option<String> {
    let bytes = line.as_bytes();
    let mut in_sq = false;
    let mut sq_start = 0;

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\'' {
            if in_sq {
                // Closing quote
                if col >= sq_start && col <= i {
                    return Some(line[sq_start + 1..i].to_string());
                }
                in_sq = false;
            } else {
                in_sq = true;
                sq_start = i;
            }
        }
    }
    None
}

/// Walk backwards to find if we're inside an args or usage array.
fn find_enclosing_array(lines: &[&str], line_idx: usize) -> Option<&'static str> {
    // Same-line fast path
    let current = lines[line_idx].trim();
    if current.contains("args=(") { return Some("args"); }
    if current.contains("usage=(") { return Some("usage"); }

    // Walk backwards with right-to-left paren counting
    let mut paren_depth: i32 = 0;
    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();
        for ch in trimmed.chars().rev() {
            match ch {
                ')' => paren_depth += 1,
                '(' => {
                    paren_depth -= 1;
                    if paren_depth < 0 {
                        if trimmed.contains("args=(") { return Some("args"); }
                        if trimmed.contains("usage=(") { return Some("usage"); }
                        return None;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Extract the type name after `:~` at cursor position.
fn extract_type_after_tilde(line: &str, col: usize) -> Option<String> {
    // col is on `~`, type name starts at col+1
    let start = col + 1;
    if start >= line.len() { return None; }
    let bytes = line.as_bytes();
    let mut end = start;
    while end < bytes.len() {
        let ch = bytes[end] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end += 1;
        } else {
            break;
        }
    }
    if end > start {
        Some(line[start..end].to_string())
    } else {
        None
    }
}
