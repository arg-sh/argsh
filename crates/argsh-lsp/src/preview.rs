//! Generate an HTML preview of an argsh script for the VSCode webview.

use std::collections::HashMap;

use argsh_syntax::document::DocumentAnalysis;
use argsh_syntax::field::FieldDef;

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

/// Generate a self-contained HTML preview of the analysed argsh script.
///
/// Includes: script overview, command tree, flags per command, MCP tool schema
/// preview, and docgen YAML preview. Styled with inline CSS using a dark theme.
pub fn generate_preview(analysis: &DocumentAnalysis, _content: &str, script_name: &str) -> String {
    let mut html = String::new();

    // Document title from the first function with a title, or fallback
    let script_title = analysis
        .functions
        .iter()
        .find_map(|f| f.title.as_deref())
        .unwrap_or("argsh Script");

    html.push_str(&format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} - argsh Preview</title>
<style>
:root {{
    --bg: #1e1e1e;
    --bg-card: #252526;
    --bg-code: #1a1a1a;
    --border: #3c3c3c;
    --text: #cccccc;
    --text-dim: #888888;
    --text-bright: #e0e0e0;
    --accent: #569cd6;
    --accent2: #4ec9b0;
    --accent3: #dcdcaa;
    --required: #f44747;
    --hidden: #6a6a6a;
}}
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, sans-serif;
    background: var(--bg);
    color: var(--text);
    padding: 24px;
    line-height: 1.6;
}}
h1 {{
    color: var(--text-bright);
    font-size: 1.5em;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 12px;
}}
h1 small {{
    font-weight: 400;
    color: var(--text-dim);
    font-size: 0.65em;
    margin-left: 8px;
}}
h2 {{
    color: var(--accent);
    font-size: 1.1em;
    margin: 24px 0 12px 0;
}}
.card {{
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 16px;
    margin-bottom: 16px;
}}
.description {{
    color: var(--text-dim);
    font-style: italic;
    margin-bottom: 16px;
}}
.cmd-tree {{
    list-style: none;
    padding-left: 0;
}}
.cmd-tree li {{
    padding: 4px 0;
}}
.cmd-tree li::before {{
    content: "\25B8 ";
    color: var(--accent);
}}
.cmd-name {{
    color: var(--accent3);
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    font-weight: 600;
}}
.cmd-desc {{
    color: var(--text-dim);
    margin-left: 8px;
}}
.cmd-hidden {{
    opacity: 0.5;
}}
table {{
    width: 100%;
    border-collapse: collapse;
    margin: 8px 0;
    font-size: 0.9em;
}}
th {{
    text-align: left;
    color: var(--text-dim);
    font-weight: 600;
    border-bottom: 1px solid var(--border);
    padding: 6px 12px 6px 0;
    font-size: 0.85em;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}}
td {{
    padding: 5px 12px 5px 0;
    border-bottom: 1px solid #2a2a2a;
    vertical-align: top;
}}
td:first-child {{
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    color: var(--accent2);
    white-space: nowrap;
}}
.type-badge {{
    display: inline-block;
    background: #2d2d2d;
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 6px;
    font-size: 0.85em;
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    color: var(--accent);
}}
.req {{
    color: var(--required);
    font-weight: 600;
}}
pre {{
    background: var(--bg-code);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 12px;
    overflow-x: auto;
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    font-size: 0.85em;
    line-height: 1.5;
    color: var(--text);
}}
.section-count {{
    color: var(--text-dim);
    font-size: 0.85em;
    margin-left: 8px;
}}
</style>
</head>
<body>
<h1>{title}<small>{name}</small></h1>
"#,
        title = html_escape(script_title),
        name = html_escape(script_name),
    ));

    // Description
    if let Some(desc) = analysis.functions.iter().find_map(|f| f.title.as_deref()) {
        html.push_str(&format!(
            "<p class=\"description\">{}</p>\n",
            html_escape(desc)
        ));
    }

    // Shebang / source info
    if let Some(ref shebang) = analysis.shebang {
        html.push_str(&format!(
            "<p style=\"color: var(--text-dim); font-size: 0.85em;\"><code>{}</code></p>\n",
            html_escape(shebang)
        ));
    }

    // Command tree
    let usage_funcs: Vec<_> = analysis
        .functions
        .iter()
        .filter(|f| !f.usage_entries.is_empty())
        .collect();

    if !usage_funcs.is_empty() {
        let total_cmds: usize = usage_funcs
            .iter()
            .map(|f| {
                f.usage_entries
                    .iter()
                    .filter(|e| !e.is_group_separator)
                    .count()
            })
            .sum();
        html.push_str(&format!(
            "<h2>Command Tree<span class=\"section-count\">{} command{}</span></h2>\n<div class=\"card\">\n",
            total_cmds,
            if total_cmds == 1 { "" } else { "s" }
        ));

        for func in &usage_funcs {
            html.push_str(&format!(
                "<p><span class=\"cmd-name\">{}</span></p>\n<ul class=\"cmd-tree\">\n",
                html_escape(&func.name)
            ));
            for entry in &func.usage_entries {
                if entry.is_group_separator {
                    continue;
                }
                let class = if entry.hidden { " class=\"cmd-hidden\"" } else { "" };
                let aliases = if entry.aliases.len() > 1 {
                    let a: Vec<&str> = entry.aliases[1..].iter().map(|s| s.as_str()).collect();
                    format!(" <span style=\"color: var(--text-dim);\">({})</span>", html_escape(&a.join(", ")))
                } else {
                    String::new()
                };
                html.push_str(&format!(
                    "  <li{}><span class=\"cmd-name\">{}</span>{}<span class=\"cmd-desc\"> &mdash; {}</span></li>\n",
                    class,
                    html_escape(&entry.name),
                    aliases,
                    html_escape(&entry.description)
                ));
            }
            html.push_str("</ul>\n");
        }
        html.push_str("</div>\n");
    }

    // Flags per command
    let args_funcs: Vec<_> = analysis
        .functions
        .iter()
        .filter(|f| !f.args_entries.is_empty())
        .collect();

    if !args_funcs.is_empty() {
        html.push_str("<h2>Flags &amp; Options</h2>\n");
        for func in &args_funcs {
            let flag_count = func.args_entries.iter().filter(|e| e.spec != "-").count();
            html.push_str(&format!(
                "<div class=\"card\">\n<p><span class=\"cmd-name\">{}</span><span class=\"section-count\">{} flag{}</span></p>\n",
                html_escape(&func.name),
                flag_count,
                if flag_count == 1 { "" } else { "s" }
            ));
            html.push_str("<table>\n<tr><th>Flag</th><th>Type</th><th>Required</th><th>Description</th></tr>\n");
            for entry in &func.args_entries {
                if entry.spec == "-" {
                    continue;
                }
                if let Ok(ref field) = entry.parsed {
                    let flag_str = if field.is_positional {
                        format!("&lt;{}&gt;", html_escape(&field.display_name))
                    } else if let Some(ref short) = field.short {
                        format!("--{}, -{}", html_escape(&field.display_name), html_escape(short))
                    } else {
                        format!("--{}", html_escape(&field.display_name))
                    };
                    let type_str = format_type(field, entry.is_array);
                    let req_str = if field.required {
                        "<span class=\"req\">yes</span>"
                    } else {
                        "no"
                    };
                    let mut desc = html_escape(&entry.description);
                    if field.is_inherited {
                        desc.push_str(" <span style=\"color:var(--text-dim);font-style:italic\">inherited</span>");
                    }
                    html.push_str(&format!(
                        "<tr><td>{}</td><td><span class=\"type-badge\">{}</span></td><td>{}</td><td>{}</td></tr>\n",
                        flag_str,
                        html_escape(&type_str),
                        req_str,
                        desc
                    ));
                }
            }
            html.push_str("</table>\n</div>\n");
        }
    }

    // MCP tools — leaf functions only (have :args, no :usage)
    let leaf_funcs: Vec<_> = analysis.functions.iter()
        .filter(|f| f.calls_args && f.usage_entries.is_empty())
        .collect();
    if !leaf_funcs.is_empty() {
        html.push_str(&format!(
            "<h2>MCP Tools<span class=\"section-count\">{} tool{}</span></h2>\n<div class=\"card\">\n",
            leaf_funcs.len(),
            if leaf_funcs.len() == 1 { "" } else { "s" }
        ));
        html.push_str("<p style=\"color: var(--text-dim); font-size: 0.85em; margin-bottom: 12px;\">Commands exposed via <code>./script mcp</code></p>\n");

        // Pre-build annotation lookup: function_name -> Vec<annotation>
        let mut annotation_map: HashMap<String, Vec<String>> = HashMap::new();
        for parent in &analysis.functions {
            for entry in &parent.usage_entries {
                if entry.is_group_separator || entry.annotations.is_empty() { continue; }
                let targets = [
                    entry.explicit_func.clone().unwrap_or_default(),
                    format!("{}::{}", parent.name, entry.name),
                    entry.name.clone(),
                ];
                for target in targets {
                    if !target.is_empty() {
                        annotation_map.entry(target).or_default().extend(entry.annotations.clone());
                    }
                }
            }
        }

        for func in &leaf_funcs {
            // MCP tool name: script_funcname (:: replaced with _)
            let tool_name = format!("{}_{}", script_name, func.name.replace("::", "_"));
            let desc = func.title.as_deref().unwrap_or("");

            // Collect annotations from pre-built map
            let annotations = annotation_map.get(&func.name).cloned().unwrap_or_default();

            let annotation_badges = if annotations.is_empty() {
                String::new()
            } else {
                let badges: Vec<String> = annotations.iter().map(|a| {
                    let color = match a.as_str() {
                        "readonly" => "#4ec9b0",
                        "destructive" => "#f44747",
                        "idempotent" => "#569cd6",
                        "json" => "#dcdcaa",
                        _ => "#888888",
                    };
                    format!("<span style=\"background:{}22;color:{};border:1px solid {}44;border-radius:3px;padding:1px 6px;font-size:0.8em;margin-left:6px;\">@{}</span>", color, color, color, html_escape(a))
                }).collect();
                badges.join("")
            };

            html.push_str(&format!(
                "<p style=\"margin-top:10px;\"><span class=\"cmd-name\">{}</span>{} <span class=\"cmd-desc\">&mdash; {}</span></p>\n",
                html_escape(&tool_name), annotation_badges, html_escape(desc)
            ));

            if !func.args_entries.is_empty() {
                html.push_str("<table style=\"margin-left: 16px; width: calc(100% - 16px);\">\n");
                for entry in &func.args_entries {
                    if entry.spec == "-" { continue; }
                    if let Ok(ref field) = entry.parsed {
                        let flag = if field.is_positional {
                            format!("&lt;{}&gt;", html_escape(&field.display_name))
                        } else if let Some(ref s) = field.short {
                            format!("--{}, -{}", html_escape(&field.display_name), html_escape(s))
                        } else {
                            format!("--{}", html_escape(&field.display_name))
                        };
                        let typ = format_type(field, entry.is_array);
                        html.push_str(&format!(
                            "<tr><td>{}</td><td><span class=\"type-badge\">{}</span></td><td>{}</td></tr>\n",
                            flag, html_escape(&typ), html_escape(&entry.description)
                        ));
                    }
                }
                html.push_str("</table>\n");
            }
        }
        html.push_str("</div>\n");
    }

    // Export links
    html.push_str("<h2>Exports</h2>\n<div class=\"card\">\n");
    html.push_str("<p style=\"color: var(--text-dim); font-size: 0.85em;\">Use the command palette to view export previews:</p>\n");
    html.push_str("<ul style=\"list-style: none; padding: 8px 0;\">\n");
    html.push_str("<li style=\"padding: 4px 0;\">&#x1F4CB; <strong>argsh: Export MCP JSON</strong> &mdash; MCP tool schema (JSON-RPC format)</li>\n");
    html.push_str("<li style=\"padding: 4px 0;\">&#x1F4C4; <strong>argsh: Export YAML</strong> &mdash; Docgen YAML output</li>\n");
    html.push_str("<li style=\"padding: 4px 0;\">&#x1F4C3; <strong>argsh: Export JSON</strong> &mdash; Docgen JSON output</li>\n");
    html.push_str("</ul>\n</div>\n");

    html.push_str("</body>\n</html>");
    html
}

/// Build a JSON string representing what `tools/list` would return.
fn build_mcp_tools(analysis: &DocumentAnalysis, script_name: &str) -> String {
    let mut tools = Vec::new();

    // Pre-build annotation lookup: function_name -> Vec<annotation>
    let mut annotation_map: HashMap<String, Vec<String>> = HashMap::new();
    for parent in &analysis.functions {
        for entry in &parent.usage_entries {
            if entry.is_group_separator || entry.annotations.is_empty() { continue; }
            let targets = [
                entry.explicit_func.clone().unwrap_or_default(),
                format!("{}::{}", parent.name, entry.name),
                entry.name.clone(),
            ];
            for target in targets {
                if !target.is_empty() {
                    annotation_map.entry(target).or_default().extend(entry.annotations.clone());
                }
            }
        }
    }

    for func in &analysis.functions {
        // Only leaf functions (has :args, no :usage dispatching)
        if !func.calls_args || !func.usage_entries.is_empty() {
            continue;
        }

        let description = func.title.as_deref().unwrap_or("");

        let mut properties = serde_json::Map::new();
        let mut required_list = Vec::new();

        for entry in &func.args_entries {
            if entry.spec == "-" {
                continue;
            }
            if let Ok(ref field) = entry.parsed {
                let json_type = if field.is_boolean {
                    "boolean"
                } else {
                    match field.type_name.as_str() {
                        "int" => "integer",
                        "float" => "number",
                        _ => "string",
                    }
                };
                let mut prop = serde_json::Map::new();
                prop.insert("type".to_string(), serde_json::Value::String(json_type.to_string()));
                prop.insert("description".to_string(), serde_json::Value::String(entry.description.clone()));
                properties.insert(field.name.clone(), serde_json::Value::Object(prop));
                if field.required {
                    required_list.push(serde_json::Value::String(field.name.clone()));
                }
            }
        }

        let mut schema = serde_json::Map::new();
        schema.insert("type".to_string(), serde_json::Value::String("object".to_string()));
        schema.insert("properties".to_string(), serde_json::Value::Object(properties));
        if !required_list.is_empty() {
            schema.insert("required".to_string(), serde_json::Value::Array(required_list));
        }
        schema.insert("additionalProperties".to_string(), serde_json::Value::Bool(false));

        let mut tool = serde_json::Map::new();
        let tool_name = format!("{}_{}", script_name, func.name.replace("::", "_"));
        tool.insert("name".to_string(), serde_json::Value::String(tool_name));
        // Match runtime: both title and description
        tool.insert("title".to_string(), serde_json::Value::String(description.to_string()));
        tool.insert("description".to_string(), serde_json::Value::String(description.to_string()));
        tool.insert("inputSchema".to_string(), serde_json::Value::Object(schema));

        // Annotations from pre-built map
        let mut annotations_obj = serde_json::Map::new();
        let mut has_json = false;

        let annotations = annotation_map.get(&func.name).cloned().unwrap_or_default();
        for ann in &annotations {
            match ann.as_str() {
                "readonly" => { annotations_obj.insert("readOnlyHint".to_string(), serde_json::Value::Bool(true)); }
                "destructive" => { annotations_obj.insert("destructiveHint".to_string(), serde_json::Value::Bool(true)); }
                "idempotent" => { annotations_obj.insert("idempotentHint".to_string(), serde_json::Value::Bool(true)); }
                "openworld" => { annotations_obj.insert("openWorldHint".to_string(), serde_json::Value::Bool(true)); }
                "json" => { has_json = true; }
                _ => {}
            }
        }

        if !annotations_obj.is_empty() {
            tool.insert("annotations".to_string(), serde_json::Value::Object(annotations_obj));
        }
        if has_json {
            tool.insert("outputSchema".to_string(), serde_json::json!({}));
        }

        tools.push(serde_json::Value::Object(tool));
    }

    if tools.is_empty() {
        return String::new();
    }

    let output = serde_json::json!({ "tools": tools });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

/// Build a YAML-like string representing docgen output.
fn build_docgen_yaml(analysis: &DocumentAnalysis, _content: &str) -> String {
    let mut yaml = String::new();

    for func in &analysis.functions {
        if !func.calls_args && !func.calls_usage {
            continue;
        }

        yaml.push_str(&format!("{}:\n", func.name));
        if let Some(ref title) = func.title {
            yaml.push_str(&format!("  description: \"{}\"\n", yaml_escape(title)));
        }

        if !func.args_entries.is_empty() {
            yaml.push_str("  args:\n");
            for entry in &func.args_entries {
                if entry.spec == "-" {
                    continue;
                }
                if let Ok(ref field) = entry.parsed {
                    yaml.push_str(&format!("    {}:\n", field.name));
                    let type_str = format_type(field, entry.is_array);
                    yaml.push_str(&format!("      type: {}\n", type_str));
                    yaml.push_str(&format!("      description: \"{}\"\n", yaml_escape(&entry.description)));
                    if field.required {
                        yaml.push_str("      required: true\n");
                    }
                    if field.hidden {
                        yaml.push_str("      hidden: true\n");
                    }
                    if field.is_inherited {
                        yaml.push_str("      inherited: true\n");
                    }
                    if let Some(ref short) = field.short {
                        yaml.push_str(&format!("      short: {}\n", short));
                    }
                }
            }
        }

        if !func.usage_entries.is_empty() {
            yaml.push_str("  commands:\n");
            for entry in &func.usage_entries {
                if entry.is_group_separator {
                    continue;
                }
                yaml.push_str(&format!("    {}:\n", entry.name));
                yaml.push_str(&format!("      description: \"{}\"\n", yaml_escape(&entry.description)));
                if entry.hidden {
                    yaml.push_str("      hidden: true\n");
                }
                if let Some(ref target) = entry.explicit_func {
                    yaml.push_str(&format!("      function: {}\n", target));
                }
                if !entry.annotations.is_empty() {
                    yaml.push_str(&format!(
                        "      annotations: [{}]\n",
                        entry.annotations.join(", ")
                    ));
                }
            }
        }
    }

    yaml
}

/// Export MCP tool schema as pretty-printed JSON.
pub fn export_mcp_json(analysis: &DocumentAnalysis, script_name: &str) -> String {
    build_mcp_tools(analysis, script_name)
}

/// Export docgen YAML.
pub fn export_yaml(analysis: &DocumentAnalysis, content: &str) -> String {
    build_docgen_yaml(analysis, content)
}

/// Export docgen as JSON.
pub fn export_docgen_json(analysis: &DocumentAnalysis) -> String {
    let mut funcs = Vec::new();
    for func in &analysis.functions {
        if !func.calls_args && !func.calls_usage { continue; }
        let mut obj = serde_json::Map::new();
        obj.insert("name".to_string(), serde_json::Value::String(func.name.clone()));
        if let Some(ref title) = func.title {
            obj.insert("description".to_string(), serde_json::Value::String(title.clone()));
        }
        if !func.args_entries.is_empty() {
            let args: Vec<serde_json::Value> = func.args_entries.iter()
                .filter(|e| e.spec != "-")
                .filter_map(|e| {
                    let field = e.parsed.as_ref().ok()?;
                    let mut m = serde_json::Map::new();
                    m.insert("name".to_string(), serde_json::Value::String(field.name.clone()));
                    m.insert("type".to_string(), serde_json::Value::String(
                        format_type(field, e.is_array)
                    ));
                    m.insert("description".to_string(), serde_json::Value::String(e.description.clone()));
                    if field.required { m.insert("required".to_string(), serde_json::Value::Bool(true)); }
                    if let Some(ref s) = field.short { m.insert("short".to_string(), serde_json::Value::String(s.clone())); }
                    Some(serde_json::Value::Object(m))
                })
                .collect();
            obj.insert("args".to_string(), serde_json::Value::Array(args));
        }
        if !func.usage_entries.is_empty() {
            let cmds: Vec<serde_json::Value> = func.usage_entries.iter()
                .filter(|e| !e.is_group_separator)
                .map(|e| {
                    let mut m = serde_json::Map::new();
                    m.insert("name".to_string(), serde_json::Value::String(e.name.clone()));
                    m.insert("description".to_string(), serde_json::Value::String(e.description.clone()));
                    serde_json::Value::Object(m)
                })
                .collect();
            obj.insert("commands".to_string(), serde_json::Value::Array(cmds));
        }
        funcs.push(serde_json::Value::Object(obj));
    }
    serde_json::to_string_pretty(&funcs).unwrap_or_default()
}

/// Escape a string for safe YAML double-quoted output.
fn yaml_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Simple HTML escaping.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
