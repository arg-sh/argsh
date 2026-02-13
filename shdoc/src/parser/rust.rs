//! Rust doc comment parser.
//!
//! Extracts documentation from `.rs` files:
//! - `//!` module-level docs → FileDoc.description
//! - `//! Mirrors: libraries/to.sh` → cross-reference
//! - `///` before functions → FunctionDoc.description
//! - `#[export_name = "to::int_struct"]` → builtin name
//! - `short_doc` and `long_doc` statics → descriptions

use crate::model::*;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

static RE_MODULE_DOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^//!\s?(.*)").unwrap());

static RE_DOC_COMMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^///\s?(.*)").unwrap());

static RE_EXPORT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"#\[export_name\s*=\s*"([^"]+)_struct"\]"#).unwrap());

static RE_SHORT_DOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"short_doc:\s*c"([^"]+)""#).unwrap());

#[allow(dead_code)]
static RE_LONG_DOC_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"SyncPtr\(c"([^"]+)""#).unwrap());

static RE_PUB_FN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^pub\s+(extern\s+"C"\s+)?fn\s+(\w+)"#).unwrap());

static RE_MIRRORS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Mirrors:\s*(.+)").unwrap());

/// Parse a Rust source file into a Document.
pub fn parse(input: &str, path: &Path) -> Document {
    let mut file_doc = FileDoc::default();
    let mut functions: Vec<FunctionDoc> = Vec::new();
    let source_file = path.to_string_lossy().to_string();

    // Phase 1: collect module docs
    let mut module_desc = String::new();
    let mut mirrors: Option<String> = None;

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    // Collect module-level //! docs
    while i < lines.len() {
        if let Some(caps) = RE_MODULE_DOC.captures(lines[i]) {
            let text = caps[1].to_string();
            if let Some(caps) = RE_MIRRORS.captures(&text) {
                // Take only the path part (strip parenthetical notes like "(:usage function)")
                let raw = caps[1].trim();
                mirrors = Some(
                    raw.split_whitespace()
                        .next()
                        .unwrap_or(raw)
                        .to_string(),
                );
            } else if !module_desc.is_empty() || !text.is_empty() {
                if !module_desc.is_empty() {
                    module_desc.push('\n');
                }
                module_desc.push_str(&text);
            }
            i += 1;
        } else {
            break;
        }
    }

    if !module_desc.is_empty() {
        let trimmed = module_desc.trim().to_string();
        if !trimmed.is_empty() {
            file_doc.description = Some(trimmed);
        }
    }
    // Use filename stem as title
    file_doc.title = path.file_stem().map(|s| s.to_string_lossy().to_string());
    file_doc.mirrors = mirrors;

    // Phase 2: scan for export_name + doc comments + pub fn
    let mut current_doc: Vec<String> = Vec::new();
    let mut current_export_name: Option<String> = None;
    let mut _current_short_doc: Option<String> = None;

    while i < lines.len() {
        let line = lines[i].trim();

        // Collect /// doc comments
        if let Some(caps) = RE_DOC_COMMENT.captures(line) {
            current_doc.push(caps[1].to_string());
            i += 1;
            continue;
        }

        // Detect #[export_name = "name_struct"]
        if let Some(caps) = RE_EXPORT_NAME.captures(line) {
            current_export_name = Some(caps[1].to_string());
            i += 1;
            continue;
        }

        // Detect short_doc
        if let Some(caps) = RE_SHORT_DOC.captures(line) {
            _current_short_doc = Some(caps[1].to_string());
            i += 1;
            continue;
        }

        // Detect pub fn — only emit if it has an #[export_name] (actual builtin)
        if RE_PUB_FN.is_match(line) {
            if let Some(export_name) = current_export_name.take() {
                let description = if !current_doc.is_empty() {
                    Some(current_doc.join("\n").trim().to_string())
                } else {
                    None
                };

                functions.push(FunctionDoc {
                    name: export_name,
                    description,
                    implementations: vec![Implementation {
                        lang: ImplLang::Rust,
                        source_file: source_file.clone(),
                    }],
                    ..Default::default()
                });
            }
            current_doc.clear();
            current_export_name = None;
            _current_short_doc = None;
            i += 1;
            continue;
        }

        // Detect BashBuiltin struct (indicates a builtin registration)
        if line.contains("BashBuiltin") && line.contains('{') && current_export_name.is_some() {
            // Scan ahead for short_doc and long_doc within this struct
            let builtin_name = current_export_name.take().unwrap();
            let mut desc = current_doc.join("\n").trim().to_string();

            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim().starts_with("};") {
                let bline = lines[j].trim();
                if let Some(caps) = RE_SHORT_DOC.captures(bline) {
                    if desc.is_empty() {
                        desc = caps[1].to_string();
                    }
                }
                j += 1;
            }

            if !builtin_name.is_empty() {
                functions.push(FunctionDoc {
                    name: builtin_name,
                    description: if desc.is_empty() { None } else { Some(desc) },
                    implementations: vec![Implementation {
                        lang: ImplLang::Rust,
                        source_file: source_file.clone(),
                    }],
                    ..Default::default()
                });
            }

            current_doc.clear();
            _current_short_doc = None;
            i = j + 1;
            continue;
        }

        // Non-doc line: clear accumulator if not blank
        if !line.is_empty() && !line.starts_with("//") && !line.starts_with('#') {
            current_doc.clear();
            current_export_name = None;
            _current_short_doc = None;
        }
        i += 1;
    }

    Document {
        file: file_doc,
        functions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_module_docs() {
        let input = "//! to::* builtins\n//!\n//! Mirrors: libraries/to.sh\n\nuse foo;\n";
        let doc = parse(input, Path::new("to.rs"));
        assert_eq!(doc.file.description.as_deref(), Some("to::* builtins"));
        assert_eq!(doc.file.mirrors.as_deref(), Some("libraries/to.sh"));
    }

    #[test]
    fn parse_export_name() {
        let input = r#"
#[export_name = "to::int_struct"]
pub static mut TO_INT_STRUCT: BashBuiltin = BashBuiltin {
    name: c"to::int".as_ptr(),
    short_doc: c"to::int <value>".as_ptr(),
};
"#;
        let doc = parse(input, Path::new("to.rs"));
        assert_eq!(doc.functions.len(), 1);
        assert_eq!(doc.functions[0].name, "to::int");
    }

    #[test]
    fn parse_doc_comment_on_exported_fn() {
        let input = "/// Check if variable is an array.\n#[export_name = \"is::array_struct\"]\npub fn is_array() {}\n";
        let doc = parse(input, Path::new("is.rs"));
        assert_eq!(doc.functions.len(), 1);
        assert_eq!(doc.functions[0].name, "is::array");
        assert_eq!(
            doc.functions[0].description.as_deref(),
            Some("Check if variable is an array.")
        );
    }

    #[test]
    fn parse_internal_fn_skipped() {
        let input = "/// Internal helper function.\npub fn helper() {}\n";
        let doc = parse(input, Path::new("shared.rs"));
        assert_eq!(doc.functions.len(), 0);
    }
}
