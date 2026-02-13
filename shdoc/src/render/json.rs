//! JSON renderer — structured output for tooling integration.
//!
//! Serializes the Document model directly as JSON.
//! Useful for custom rendering pipelines and IDE integration.

use crate::model::*;
use crate::render::Renderer;

pub struct JsonRenderer;

impl Renderer for JsonRenderer {
    fn render(&self, doc: &Document) -> String {
        let mut out = String::new();
        out.push_str("{\n");

        // File metadata
        out.push_str("  \"file\": {\n");
        write_opt_field(&mut out, "title", &doc.file.title, true);
        write_opt_field(&mut out, "brief", &doc.file.brief, true);
        write_opt_field(&mut out, "description", &doc.file.description, true);
        write_opt_field(&mut out, "tags", &doc.file.tags, false);
        out.push_str("  },\n");

        // Functions
        out.push_str("  \"functions\": [\n");
        for (i, func) in doc.functions.iter().enumerate() {
            out.push_str(&render_function_json(func));
            if i < doc.functions.len() - 1 {
                out.push_str(",\n");
            } else {
                out.push('\n');
            }
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        out
    }

    fn file_extension(&self) -> &str {
        "json"
    }
}

fn render_function_json(func: &FunctionDoc) -> String {
    let mut out = String::new();
    out.push_str("    {\n");

    out.push_str(&format!(
        "      \"name\": \"{}\",\n",
        json_escape(&func.name)
    ));

    if let Some(ref desc) = func.description {
        out.push_str(&format!(
            "      \"description\": \"{}\",\n",
            json_escape(desc)
        ));
    }

    // Section
    if let Some(ref section) = func.section {
        out.push_str(&format!(
            "      \"section\": \"{}\",\n",
            json_escape(&section.title)
        ));
    }

    // Example
    if let Some(ref example) = func.example {
        out.push_str(&format!(
            "      \"example\": \"{}\",\n",
            json_escape(example)
        ));
    }

    // Args
    if !func.args.is_empty() {
        out.push_str("      \"args\": [\n");
        for (i, arg) in func.args.iter().enumerate() {
            let comma = if i < func.args.len() - 1 { "," } else { "" };
            out.push_str(&format!(
                "        \"{}\"{}",
                json_escape(&arg.raw),
                comma
            ));
            out.push('\n');
        }
        out.push_str("      ],\n");
    }

    if func.noargs {
        out.push_str("      \"noargs\": true,\n");
    }

    // Exit codes
    if !func.exit_codes.is_empty() {
        write_string_array(&mut out, "exit_codes", &func.exit_codes);
    }

    // stdout
    if !func.stdout.is_empty() {
        write_string_array(&mut out, "stdout", &func.stdout);
    }

    // stderr
    if !func.stderr.is_empty() {
        write_string_array(&mut out, "stderr", &func.stderr);
    }

    // Implementations
    if !func.implementations.is_empty() {
        out.push_str("      \"implementations\": [\n");
        for (i, imp) in func.implementations.iter().enumerate() {
            let lang = match imp.lang {
                ImplLang::Bash => "bash",
                ImplLang::Rust => "rust",
            };
            let comma = if i < func.implementations.len() - 1 {
                ","
            } else {
                ""
            };
            out.push_str(&format!(
                "        {{ \"lang\": \"{}\", \"source\": \"{}\" }}{}",
                lang,
                json_escape(&imp.source_file),
                comma
            ));
            out.push('\n');
        }
        out.push_str("      ],\n");
    }

    // Remove trailing comma from last field
    let trimmed = out.trim_end().trim_end_matches(',').to_string();
    out = trimmed;
    out.push('\n');
    out.push_str("    }");
    out
}

fn write_opt_field(out: &mut String, name: &str, value: &Option<String>, trailing_comma: bool) {
    let comma = if trailing_comma { "," } else { "" };
    match value {
        Some(v) => {
            out.push_str(&format!(
                "    \"{}\": \"{}\"{}",
                name,
                json_escape(v),
                comma
            ));
            out.push('\n');
        }
        None => {
            out.push_str(&format!("    \"{}\": null{}", name, comma));
            out.push('\n');
        }
    }
}

fn write_string_array(out: &mut String, name: &str, items: &[String]) {
    out.push_str(&format!("      \"{}\": [", name));
    if items.len() == 1 {
        out.push_str(&format!("\"{}\"", json_escape(&items[0])));
    } else {
        out.push('\n');
        for (i, item) in items.iter().enumerate() {
            let comma = if i < items.len() - 1 { "," } else { "" };
            out.push_str(&format!("        \"{}\"{}",json_escape(item), comma));
            out.push('\n');
        }
        out.push_str("      ");
    }
    out.push_str("],\n");
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
