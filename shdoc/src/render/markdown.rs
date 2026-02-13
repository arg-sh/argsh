//! GitHub-flavored markdown renderer.
//!
//! Mirrors the `render_docblock()` function and style transforms from the gawk
//! shdoc to produce byte-identical output.

use crate::model::*;
use crate::render::Renderer;
use crate::toc;
use regex::Regex;
use std::sync::LazyLock;

pub struct MarkdownRenderer;

// Style transform regexes (from shdoc `styles[]` array, BEGIN block lines 5-54)
static RE_ARG_N: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\$[0-9]+)\s+(\S+)\s+").unwrap());

static RE_ARG_AT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\$@\s+(\S+)\s+").unwrap());

static RE_SET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\S+) (\S+)").unwrap());

static RE_EXITCODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([>!]?[0-9]{1,3}) (.*)").unwrap());

impl Renderer for MarkdownRenderer {
    fn render(&self, doc: &Document) -> String {
        let mut output = String::new();

        // File description (gawk END block, lines 906-917)
        if doc.file.title.is_some() {
            if let Some(ref desc) = doc.file.description {
                output.push_str(desc);
                output.push_str("\n\n");
            }
        }

        // Table of contents (gawk END block, lines 919-922)
        let visible: Vec<&FunctionDoc> = doc.functions.iter().filter(|f| !f.name.is_empty()).collect();
        if !visible.is_empty() {
            output.push_str("## Index\n\n");
            for func in &visible {
                output.push_str(&toc::render_toc_item(&func.name));
                output.push('\n');
            }
            output.push('\n');
        }

        // Function documentation (gawk line 924: print doc)
        for func in &visible {
            output.push_str(&render_function(func));
            output.push('\n');
        }

        output
    }

    fn file_extension(&self) -> &str {
        "mdx"
    }
}

/// Render a single function's documentation block.
///
/// Matches `render_docblock()` (shdoc lines 454-596) exactly.
fn render_function(func: &FunctionDoc) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Section header (if present)
    if let Some(ref section) = func.section {
        lines.push(format!("## {}\n", section.title));
        if let Some(ref desc) = section.description {
            lines.push(desc.clone());
            lines.push(String::new());
        }
    }

    // Function heading
    lines.push(format!("### {}\n", func.name));

    // Badges: implementation languages, @internal, @tags
    let badges = render_badges(func);
    if !badges.is_empty() {
        lines.push(badges);
        lines.push(String::new());
    }

    // Description
    if let Some(ref desc) = func.description {
        lines.push(desc.clone());
        lines.push(String::new());
    }

    // Example (shdoc lines 482-488)
    if let Some(ref example) = func.example {
        lines.push("#### Example\n".to_string());
        lines.push("```bash".to_string());
        lines.push(unindent(example));
        lines.push("```".to_string());
        lines.push(String::new());
    }

    // Options (shdoc lines 490-518)
    if !func.options.is_empty() || !func.options_bad.is_empty() {
        lines.push("#### Options\n".to_string());

        for opt in &func.options {
            // Render as definition list (dt/dd)
            let term = format!("**{}**", opt.term);
            // Escape < and > in term
            let term = term.replace('<', "\\<").replace('>', "\\>");
            lines.push(format!("* {}\n", term));
            if !opt.definition.is_empty() {
                lines.push(format!("  {}\n", opt.definition));
            }
        }

        for bad in &func.options_bad {
            lines.push(format!("* {}", bad));
        }
        lines.push(String::new());
    }

    // Arguments (shdoc lines 521-538)
    if !func.args.is_empty() {
        lines.push("#### Arguments\n".to_string());
        for arg in &func.args {
            let rendered = render_arg(&arg.raw);
            lines.push(format!("* {}", rendered));
        }
        lines.push(String::new());
    }

    // Noargs (shdoc lines 540-545)
    if func.noargs {
        lines.push("_Function has no arguments._".to_string());
        lines.push(String::new());
    }

    // Variables set (shdoc lines 547-558)
    if !func.set_vars.is_empty() {
        lines.push("#### Variables set\n".to_string());
        for var in &func.set_vars {
            let rendered = render_set(var);
            lines.push(format!("* {}", rendered));
        }
        lines.push(String::new());
    }

    // Exit codes (shdoc lines 560-569)
    if !func.exit_codes.is_empty() {
        lines.push("#### Exit codes\n".to_string());
        for code in &func.exit_codes {
            let rendered = render_exitcode(code);
            lines.push(format!("* {}", rendered));
        }
        lines.push(String::new());
    }

    // stdin (shdoc lines 571-573)
    if !func.stdin.is_empty() {
        render_docblock_list(&mut lines, &func.stdin, "Input on stdin");
    }

    // stdout (shdoc lines 575-577)
    if !func.stdout.is_empty() {
        render_docblock_list(&mut lines, &func.stdout, "Output on stdout");
    }

    // stderr (shdoc lines 579-581)
    if !func.stderr.is_empty() {
        render_docblock_list(&mut lines, &func.stderr, "Output on stderr");
    }

    // See also (shdoc lines 583-592)
    if !func.see_also.is_empty() {
        lines.push("#### See also\n".to_string());
        for see in &func.see_also {
            let link = toc::render_toc_link(see);
            lines.push(format!("* {}", link));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Render a docblock list section (stdin/stdout/stderr).
fn render_docblock_list(lines: &mut Vec<String>, items: &[String], title: &str) {
    lines.push(format!("#### {}\n", title));
    for item in items {
        // Indent additional lines for markdown list
        let indented = item.replace('\n', "\n  ");
        lines.push(format!("* {}", indented));
    }
    lines.push(String::new());
}

// -- Style transforms ---------------------------------------------------------

/// Render @arg entry with argN and arg@ styles.
///
/// `$1 string desc` → `**$1** (string): desc`
/// `$@ type desc` → `**...** (type): desc`
fn render_arg(text: &str) -> String {
    // Try $@ first
    if let Some(caps) = RE_ARG_AT.captures(text) {
        let type_name = &caps[1];
        let rest = &text[caps[0].len()..];
        return format!("**...** ({}): {}", type_name, rest);
    }
    // Try $N
    if let Some(caps) = RE_ARG_N.captures(text) {
        let arg_name = &caps[1];
        let type_name = &caps[2];
        let rest = &text[caps[0].len()..];
        return format!("**{}** ({}): {}", arg_name, type_name, rest);
    }
    // Fallback: return as-is
    text.to_string()
}

/// Render @set entry.
///
/// `var type rest` → `**var** (type): rest`
fn render_set(text: &str) -> String {
    if let Some(caps) = RE_SET.captures(text) {
        let var = &caps[1];
        let type_name = &caps[2];
        let rest = text[caps[0].len()..].trim_start();
        return format!("**{}** ({}): {}", var, type_name, rest);
    }
    text.to_string()
}

/// Render @exitcode entry.
///
/// `0 desc` → `**0**: desc`
fn render_exitcode(text: &str) -> String {
    if let Some(caps) = RE_EXITCODE.captures(text) {
        let code = &caps[1];
        let desc = &caps[2];
        return format!("**{}**: {}", code, desc);
    }
    text.to_string()
}

/// Remove common leading indentation from a multi-line string.
///
/// Mirrors the gawk `unindent()` function (shdoc lines 192-230).
fn unindent(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();

    // Find first non-empty line
    let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);

    if start >= lines.len() {
        return text.to_string();
    }

    // Find minimum indentation across non-empty lines from start
    let min_indent = lines[start..]
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);

    // Strip minimum indentation and rejoin
    lines[start..]
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                &l[min_indent..]
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render badges for a function: implementation languages, @internal, @tags.
///
/// Only shows implementation badges when there are mixed sources (not all bash-only).
/// Output: `> `**`bash`** **`rust`** *`internal`* `` `tag1` `` `` `tag2` ``
fn render_badges(func: &FunctionDoc) -> String {
    let mut badges: Vec<String> = Vec::new();

    // Implementation language badges (only when implementations are populated)
    if !func.implementations.is_empty() {
        let has_bash = func.implementations.iter().any(|i| i.lang == ImplLang::Bash);
        let has_rust = func.implementations.iter().any(|i| i.lang == ImplLang::Rust);
        if has_bash && has_rust {
            badges.push("`bash`".to_string());
            badges.push("`rust`".to_string());
        } else if has_rust {
            badges.push("`rust`".to_string());
        }
        // bash-only is the default — no badge needed
    }

    // @internal badge
    if func.is_internal {
        badges.push("*`internal`*".to_string());
    }

    // @tags badges
    for tag in &func.tags {
        badges.push(format!("`{}`", tag));
    }

    if badges.is_empty() {
        return String::new();
    }

    format!("> {}", badges.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_numbered() {
        assert_eq!(
            render_arg("$1 string The value"),
            "**$1** (string): The value"
        );
    }

    #[test]
    fn arg_at() {
        assert_eq!(
            render_arg("$@ string Additional args"),
            "**...** (string): Additional args"
        );
    }

    #[test]
    fn exitcode_simple() {
        assert_eq!(
            render_exitcode("0 If successful"),
            "**0**: If successful"
        );
    }

    #[test]
    fn exitcode_modifier() {
        assert_eq!(
            render_exitcode(">0 Generic error"),
            "**>0**: Generic error"
        );
    }

    #[test]
    fn set_variable() {
        assert_eq!(
            render_set("usage array [get] The usage array"),
            "**usage** (array): [get] The usage array"
        );
    }

    #[test]
    fn unindent_basic() {
        assert_eq!(unindent("  a\n  b\n  c"), "a\nb\nc");
    }

    #[test]
    fn unindent_mixed() {
        assert_eq!(unindent("  a\n    b\n  c"), "a\n  b\nc");
    }

    #[test]
    fn unindent_empty_first() {
        assert_eq!(unindent("\n  a\n  b"), "a\nb");
    }
}
