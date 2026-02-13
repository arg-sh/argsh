//! Cross-match merge: combine bash + rust documents by function name.
//!
//! When shdoc processes both .sh and .rs files, this module merges
//! the resulting Documents so each function shows all its implementations.

use crate::model::*;
use std::collections::HashMap;

/// Merge multiple documents into a single document per logical module.
///
/// Groups FunctionDocs by canonical name across all documents.
/// Bash docs take priority for descriptions (richer shdoc annotations).
pub fn merge(docs: Vec<(String, Document)>) -> Vec<(String, Document)> {
    // Group documents by module name (e.g., "to" from "to.sh" and "to.rs")
    let mut groups: HashMap<String, Vec<(String, Document)>> = HashMap::new();

    for (source_file, doc) in docs {
        // Use mirrors annotation for module name if available (e.g., "libraries/args.sh" → "args"),
        // otherwise derive from the source filename.
        let module_name = doc
            .file
            .mirrors
            .as_deref()
            .map(derive_module_name)
            .unwrap_or_else(|| derive_module_name(&source_file));
        groups
            .entry(module_name)
            .or_default()
            .push((source_file, doc));
    }

    let mut result = Vec::new();

    for (module_name, docs_in_group) in groups {
        if docs_in_group.len() == 1 {
            // No merging needed — single source
            let (source, doc) = docs_in_group.into_iter().next().unwrap();
            result.push((source, doc));
            continue;
        }

        // Merge multiple sources
        let merged = merge_group(docs_in_group);
        result.push((module_name, merged));
    }

    result
}

/// Check if a source file is a bash script.
fn is_bash_source(path: &str) -> bool {
    path.ends_with(".sh") || path.ends_with(".bash") || path.ends_with(".bats")
}

/// Check if a function has richer documentation (from bash shdoc annotations).
fn is_richer(func: &FunctionDoc) -> bool {
    func.example.is_some()
        || !func.args.is_empty()
        || !func.exit_codes.is_empty()
        || !func.stdout.is_empty()
        || !func.stderr.is_empty()
        || !func.options.is_empty()
        || !func.set_vars.is_empty()
}

/// Merge a group of documents that share the same module name.
fn merge_group(docs: Vec<(String, Document)>) -> Document {
    let mut file_doc = FileDoc::default();
    let mut func_map: HashMap<String, FunctionDoc> = HashMap::new();
    let mut func_order: Vec<String> = Vec::new();

    for (source_file, doc) in docs {
        let from_bash = is_bash_source(&source_file);

        // File-level: bash takes priority
        if from_bash {
            if doc.file.title.is_some() {
                file_doc.title = doc.file.title;
            }
            if doc.file.brief.is_some() {
                file_doc.brief = doc.file.brief;
            }
            if doc.file.description.is_some() {
                file_doc.description = doc.file.description;
            }
            if doc.file.tags.is_some() {
                file_doc.tags = doc.file.tags;
            }
        } else if file_doc.title.is_none() {
            // Rust fallback for file metadata
            file_doc.title = doc.file.title;
            if file_doc.description.is_none() {
                file_doc.description = doc.file.description;
            }
        }

        for func in doc.functions {
            let canonical = canonical_name(&func.name);

            if let Some(existing) = func_map.get_mut(&canonical) {
                // When the incoming function has richer docs (bash annotations),
                // replace the existing entry but keep all implementations merged.
                if is_richer(&func) && !is_richer(existing) {
                    let mut merged_impls = existing.implementations.clone();
                    merged_impls.extend(func.implementations.clone());
                    let _ = std::mem::replace(existing, func);
                    existing.implementations = merged_impls;
                } else {
                    // Just merge implementations and fill in missing description
                    existing.implementations.extend(func.implementations);
                    if existing.description.is_none() && func.description.is_some() {
                        existing.description = func.description;
                    }
                }
            } else {
                func_order.push(canonical.clone());
                func_map.insert(canonical, func);
            }
        }
    }

    // Preserve insertion order
    let functions: Vec<FunctionDoc> = func_order
        .into_iter()
        .filter_map(|name| func_map.remove(&name))
        .collect();

    Document {
        file: file_doc,
        functions,
    }
}

/// Derive a module name from a source file path.
/// "libraries/to.sh" → "to", "builtin/src/to.rs" → "to"
fn derive_module_name(path: &str) -> String {
    let filename = path.rsplit('/').next().unwrap_or(path);
    filename
        .strip_suffix(".sh")
        .or_else(|| filename.strip_suffix(".bash"))
        .or_else(|| filename.strip_suffix(".bats"))
        .or_else(|| filename.strip_suffix(".rs"))
        .unwrap_or(filename)
        .to_string()
}

/// Canonical function name for matching across implementations.
/// Strips leading colons (`:args` → `args`), keeps `::` separators.
fn canonical_name(name: &str) -> String {
    name.trim_start_matches(':').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_name_from_sh() {
        assert_eq!(derive_module_name("libraries/to.sh"), "to");
        assert_eq!(derive_module_name("to.sh"), "to");
    }

    #[test]
    fn module_name_from_rs() {
        assert_eq!(derive_module_name("builtin/src/to.rs"), "to");
    }

    #[test]
    fn canonical_strips_colon() {
        assert_eq!(canonical_name(":args"), "args");
        assert_eq!(canonical_name("to::int"), "to::int");
    }

    #[test]
    fn merge_single_doc() {
        let docs = vec![(
            "to.sh".to_string(),
            Document {
                file: FileDoc {
                    title: Some("to".to_string()),
                    ..Default::default()
                },
                functions: vec![FunctionDoc {
                    name: "to::int".to_string(),
                    description: Some("Validate int".to_string()),
                    implementations: vec![Implementation {
                        lang: ImplLang::Bash,
                        source_file: "to.sh".to_string(),
                    }],
                    ..Default::default()
                }],
            },
        )];

        let result = merge(docs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.functions[0].implementations.len(), 1);
    }

    #[test]
    fn merge_bash_and_rust() {
        let docs = vec![
            (
                "to.sh".to_string(),
                Document {
                    file: FileDoc {
                        title: Some("to".to_string()),
                        description: Some("Type converters".to_string()),
                        ..Default::default()
                    },
                    functions: vec![FunctionDoc {
                        name: "to::int".to_string(),
                        description: Some("Validate integer from bash".to_string()),
                        implementations: vec![Implementation {
                            lang: ImplLang::Bash,
                            source_file: "to.sh".to_string(),
                        }],
                        ..Default::default()
                    }],
                },
            ),
            (
                "to.rs".to_string(),
                Document {
                    file: FileDoc {
                        title: Some("to".to_string()),
                        ..Default::default()
                    },
                    functions: vec![FunctionDoc {
                        name: "to::int".to_string(),
                        description: Some("Validate integer from rust".to_string()),
                        implementations: vec![Implementation {
                            lang: ImplLang::Rust,
                            source_file: "to.rs".to_string(),
                        }],
                        ..Default::default()
                    }],
                },
            ),
        ];

        let result = merge(docs);
        assert_eq!(result.len(), 1);
        let funcs = &result[0].1.functions;
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].implementations.len(), 2);
        // Bash description takes priority
        assert_eq!(
            funcs[0].description.as_deref(),
            Some("Validate integer from bash")
        );
    }
}
