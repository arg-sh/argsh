//! HTML renderer — standalone HTML page with semantic markup.

use crate::model::*;
use crate::render::Renderer;

pub struct HtmlRenderer;

impl Renderer for HtmlRenderer {
    fn render(&self, doc: &Document) -> String {
        let mut out = String::new();

        out.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        out.push_str("<meta charset=\"utf-8\">\n");
        if let Some(ref title) = doc.file.title {
            out.push_str(&format!("<title>{}</title>\n", html_escape(title)));
        }
        out.push_str("<style>\n");
        out.push_str("body { font-family: system-ui, sans-serif; max-width: 48em; margin: 2em auto; padding: 0 1em; }\n");
        out.push_str("code { background: #f4f4f4; padding: 0.15em 0.3em; border-radius: 3px; }\n");
        out.push_str("pre { background: #f4f4f4; padding: 1em; border-radius: 5px; overflow-x: auto; }\n");
        out.push_str("dt { font-weight: bold; margin-top: 0.5em; }\n");
        out.push_str("dd { margin-left: 1.5em; }\n");
        out.push_str(".tag { display: inline-block; font-size: 0.75em; padding: 0.1em 0.4em; border-radius: 3px; margin-left: 0.5em; }\n");
        out.push_str(".tag-bash { background: #4eaa25; color: white; }\n");
        out.push_str(".tag-rust { background: #dea584; color: #1a1a1a; }\n");
        out.push_str("</style>\n");
        out.push_str("</head>\n<body>\n");

        // File description
        if let Some(ref desc) = doc.file.description {
            out.push_str(&format!("<p>{}</p>\n", html_escape(desc)));
        }

        // Index
        if !doc.functions.is_empty() {
            out.push_str("<h2>Index</h2>\n<ul>\n");
            for func in &doc.functions {
                let anchor = func.name.to_lowercase().replace(|c: char| !c.is_alphanumeric() && c != '-', "");
                out.push_str(&format!(
                    "  <li><a href=\"#{}\">{}</a></li>\n",
                    html_escape(&anchor),
                    html_escape(&func.name)
                ));
            }
            out.push_str("</ul>\n");
        }

        // Functions
        for func in &doc.functions {
            out.push_str(&render_function_html(func));
        }

        out.push_str("</body>\n</html>\n");
        out
    }

    fn file_extension(&self) -> &str {
        "html"
    }
}

fn render_function_html(func: &FunctionDoc) -> String {
    let mut out = String::new();
    let anchor = func.name.to_lowercase().replace(|c: char| !c.is_alphanumeric() && c != '-', "");

    // Section
    if let Some(ref section) = func.section {
        out.push_str(&format!("<h2>{}</h2>\n", html_escape(&section.title)));
        if let Some(ref desc) = section.description {
            out.push_str(&format!("<p>{}</p>\n", html_escape(desc)));
        }
    }

    // Function heading with implementation tags
    out.push_str(&format!(
        "<h3 id=\"{}\">{}", html_escape(&anchor), html_escape(&func.name)
    ));
    for imp in &func.implementations {
        let (class, label) = match imp.lang {
            ImplLang::Bash => ("tag-bash", "bash"),
            ImplLang::Rust => ("tag-rust", "rust"),
        };
        out.push_str(&format!(" <span class=\"tag {}\">{}</span>", class, label));
    }
    out.push_str("</h3>\n");

    // Description
    if let Some(ref desc) = func.description {
        out.push_str(&format!("<p>{}</p>\n", html_escape(desc)));
    }

    // Example
    if let Some(ref example) = func.example {
        out.push_str("<h4>Example</h4>\n");
        out.push_str(&format!("<pre><code class=\"language-bash\">{}</code></pre>\n", html_escape(example)));
    }

    // Arguments
    if !func.args.is_empty() {
        out.push_str("<h4>Arguments</h4>\n<dl>\n");
        for arg in &func.args {
            out.push_str(&format!("  <dt><code>{}</code></dt>\n", html_escape(&arg.raw)));
        }
        out.push_str("</dl>\n");
    }

    if func.noargs {
        out.push_str("<p><em>Function has no arguments.</em></p>\n");
    }

    // Exit codes
    if !func.exit_codes.is_empty() {
        out.push_str("<h4>Exit codes</h4>\n<dl>\n");
        for code in &func.exit_codes {
            out.push_str(&format!("  <dt>{}</dt>\n", html_escape(code)));
        }
        out.push_str("</dl>\n");
    }

    // stdout
    if !func.stdout.is_empty() {
        out.push_str("<h4>Output on stdout</h4>\n<ul>\n");
        for item in &func.stdout {
            out.push_str(&format!("  <li>{}</li>\n", html_escape(item)));
        }
        out.push_str("</ul>\n");
    }

    // stderr
    if !func.stderr.is_empty() {
        out.push_str("<h4>Output on stderr</h4>\n<ul>\n");
        for item in &func.stderr {
            out.push_str(&format!("  <li>{}</li>\n", html_escape(item)));
        }
        out.push_str("</ul>\n");
    }

    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
