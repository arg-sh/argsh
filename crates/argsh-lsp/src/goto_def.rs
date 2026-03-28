use tower_lsp::lsp_types::*;

use argsh_syntax::document::DocumentAnalysis;

/// Go-to-definition: resolve the symbol under the cursor to a location.
pub fn goto_definition(
    analysis: &DocumentAnalysis,
    position: Position,
    content: &str,
    uri: &Url,
) -> Option<Location> {
    let line_idx = position.line as usize;
    let col = position.character as usize;
    let lines: Vec<&str> = content.lines().collect();

    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];

    // 1. Check if cursor is on a usage entry `:-func::name` mapping
    if let Some(target) = extract_func_mapping_at(line, col) {
        return find_function_location(analysis, &target, uri);
    }

    // 2. Check if cursor is on an import statement
    if let Some(module) = extract_import_module(line, col) {
        // We cannot resolve import paths without filesystem context,
        // but we can check if the module corresponds to a function in the file.
        let _ = module;
    }

    // 3. Check if cursor is on a function call or name like `func::name`
    let word = extract_word_at(line, col);
    if !word.is_empty() {
        // Check if it matches a usage entry's explicit_func
        for func in &analysis.functions {
            for entry in &func.usage_entries {
                if let Some(ref target) = entry.explicit_func {
                    if target == &word {
                        return find_function_location(analysis, target, uri);
                    }
                }
                // Check if the word matches a usage entry name and resolve
                // to the target function (either explicit or implicit)
                if entry.name == word || entry.aliases.contains(&word.to_string()) {
                    let target_name = entry
                        .explicit_func
                        .as_deref()
                        .unwrap_or_else(|| {
                            // Implicit: parent::subcmd pattern not easily detectable here
                            &word
                        });
                    if let Some(loc) = find_function_location(analysis, target_name, uri) {
                        return Some(loc);
                    }
                    // Try with parent prefix
                    let prefixed = format!("{}::{}", func.name, entry.name);
                    if let Some(loc) = find_function_location(analysis, &prefixed, uri) {
                        return Some(loc);
                    }
                }
            }
        }

        // Direct function name match
        if let Some(loc) = find_function_location(analysis, &word, uri) {
            return Some(loc);
        }
    }

    None
}

/// Find a function by name in the analysis and return its location.
fn find_function_location(
    analysis: &DocumentAnalysis,
    name: &str,
    uri: &Url,
) -> Option<Location> {
    analysis
        .functions
        .iter()
        .find(|f| f.name == name)
        .map(|f| Location {
            uri: uri.clone(),
            range: Range {
                start: Position {
                    line: f.line as u32,
                    character: 0,
                },
                end: Position {
                    line: f.line as u32,
                    character: f.name.len() as u32,
                },
            },
        })
}

/// Extract a `:-func::name` mapping if the cursor is on it.
fn extract_func_mapping_at(line: &str, col: usize) -> Option<String> {
    // Find all `:-` patterns in the line
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find(":-") {
        let abs_pos = search_start + pos;
        let func_start = abs_pos + 2;

        // Find the end of the function name
        let func_end = line[func_start..]
            .find(|c: char| c == '\'' || c == '"' || c == '@' || c == ' ' || c == ':')
            .map(|p| func_start + p)
            .unwrap_or(line.len());

        // Check if cursor is within this range
        if col >= abs_pos && col <= func_end {
            let func_name = &line[func_start..func_end];
            if !func_name.is_empty() {
                return Some(func_name.to_string());
            }
        }

        search_start = func_start;
    }
    None
}

/// Extract the import module name if cursor is on it.
fn extract_import_module(line: &str, _col: usize) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("import ") {
        let rest = trimmed.strip_prefix("import ")?.trim();
        // Last word is the module name
        let module = rest.split_whitespace().last()?;
        return Some(module.to_string());
    }
    None
}

/// Extract the word (identifier with :: allowed) at the given column.
fn extract_word_at(line: &str, col: usize) -> String {
    if col > line.len() {
        return String::new();
    }

    let bytes = line.as_bytes();

    // Find start of word
    let mut start = col;
    while start > 0 {
        let prev = start - 1;
        let ch = bytes[prev] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' {
            start = prev;
        } else {
            break;
        }
    }

    // Find end of word
    let mut end = col;
    while end < bytes.len() {
        let ch = bytes[end] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' {
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
