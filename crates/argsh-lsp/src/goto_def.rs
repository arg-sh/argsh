use tower_lsp::lsp_types::*;

use argsh_syntax::document::{analyze, DocumentAnalysis};

use crate::resolver::ResolvedImports;
use crate::util::extract_word_at;

/// Go-to-definition: resolve the symbol under the cursor to a location.
pub fn goto_definition(
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
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

    // 1. Check if cursor is inside a single-quoted usage entry — resolve target function
    if let Some(loc) = goto_usage_entry(analysis, imports, line, col, line_idx, uri) {
        return Some(loc);
    }

    // 2. Check if cursor is on a `:-func::name` mapping specifically
    if let Some(target) = extract_func_mapping_at(line, col) {
        return find_function_location(analysis, imports, &target, uri);
    }

    // 3. Check if cursor is on a type reference :~typename — resolve to to::typename()
    if let Some(loc) = goto_type_definition(analysis, imports, line, col, uri) {
        return Some(loc);
    }

    // 4. Check if cursor is on an import statement — resolve to the imported file
    if let Some(module) = extract_import_module(line, col) {
        // Strip @/~/^ prefix for matching against resolved files
        let clean = module.trim_start_matches(&['@', '~', '^'] as &[char]);
        for (mod_name, path) in &imports.resolved_files {
            // Match either the full module name or the clean version
            if *mod_name == module || *mod_name == clean || mod_name.ends_with(clean) {
                if let Ok(import_uri) = Url::from_file_path(path) {
                    return Some(Location {
                        uri: import_uri,
                        range: Range {
                            start: Position { line: 0, character: 0 },
                            end: Position { line: 0, character: 0 },
                        },
                    });
                }
            }
        }
    }

    // 5. Check if cursor is on a function call or name like `func::name`
    let word = extract_word_at(line, col);
    if !word.is_empty() {
        // Check if it matches a usage entry's explicit_func
        for func in &analysis.functions {
            for entry in &func.usage_entries {
                if let Some(ref target) = entry.explicit_func {
                    if target == &word {
                        return find_function_location(analysis, imports, target, uri);
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
                    if let Some(loc) = find_function_location(analysis, imports, target_name, uri) {
                        return Some(loc);
                    }
                    // Try with full caller prefix: caller::subcmd
                    let prefixed = format!("{}::{}", func.name, entry.name);
                    if let Some(loc) = find_function_location(analysis, imports, &prefixed, uri) {
                        return Some(loc);
                    }
                    // Try with last segment prefix: last_seg::subcmd
                    if let Some(pos) = func.name.rfind("::") {
                        let seg_prefixed = format!("{}::{}", &func.name[pos + 2..], entry.name);
                        if let Some(loc) = find_function_location(analysis, imports, &seg_prefixed, uri) {
                            return Some(loc);
                        }
                    }
                }
            }
        }

        // Direct function name match
        if let Some(loc) = find_function_location(analysis, imports, &word, uri) {
            return Some(loc);
        }
    }

    None
}

/// Find a function by name in the analysis (current file first, then imports).
fn find_function_location(
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
    name: &str,
    uri: &Url,
) -> Option<Location> {
    // First try in current file
    if let Some(f) = analysis.functions.iter().find(|f| f.name == name) {
        return Some(Location {
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
        });
    }

    // Then try imported files
    if imports.functions.iter().any(|f| f.name == name) {
        for (_, path) in &imports.resolved_files {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let imported_analysis = analyze(&content);
            if let Some(f) = imported_analysis.functions.iter().find(|f| f.name == name) {
                let import_uri = Url::from_file_path(path).ok()?;
                return Some(Location {
                    uri: import_uri,
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
                });
            }
        }
    }

    None
}

/// If cursor is inside a single-quoted usage entry, resolve to the target function.
fn goto_usage_entry(
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
    line: &str,
    col: usize,
    line_idx: usize,
    uri: &Url,
) -> Option<Location> {
    // Find enclosing single-quoted string
    let spec = extract_single_quoted_at(line, col)?;

    // Find which function this line belongs to
    let func = analysis.functions.iter().find(|f|
        line_idx >= f.line && line_idx <= f.end_line
    )?;

    // Find matching usage entry
    for entry in &func.usage_entries {
        if entry.is_group_separator { continue; }
        if !spec.contains(&entry.name) { continue; }

        // Resolve target function
        let candidates = if let Some(ref explicit) = entry.explicit_func {
            vec![explicit.clone()]
        } else {
            let mut c = vec![format!("{}::{}", func.name, entry.name)];
            // Last segment prefix: main::manifest → manifest::subcmd
            if let Some(pos) = func.name.rfind("::") {
                c.push(format!("{}::{}", &func.name[pos + 2..], entry.name));
            }
            c.push(entry.name.clone());
            c.push(format!("argsh::{}", entry.name));
            c
        };

        for candidate in &candidates {
            if let Some(loc) = find_function_location(analysis, imports, candidate, uri) {
                return Some(loc);
            }
        }
    }
    None
}

/// Extract the content of the single-quoted string containing the cursor.
fn extract_single_quoted_at(line: &str, col: usize) -> Option<String> {
    let bytes = line.as_bytes();
    let mut in_sq = false;
    let mut sq_start = 0;

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\'' {
            if in_sq {
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

/// If cursor is on `:~typename`, resolve to `to::typename()` function.
fn goto_type_definition(
    analysis: &DocumentAnalysis,
    imports: &ResolvedImports,
    line: &str,
    col: usize,
    uri: &Url,
) -> Option<Location> {
    // Find :~typename pattern around cursor
    let bytes = line.as_bytes();

    // Search backwards from cursor for :~
    let mut start = col;
    while start > 0 {
        if start >= 2 && bytes[start - 2] == b':' && bytes[start - 1] == b'~' {
            // Found :~ — extract the type name after it
            let type_start = start;
            let mut type_end = start;
            while type_end < bytes.len() {
                let ch = bytes[type_end] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    type_end += 1;
                } else {
                    break;
                }
            }
            if type_end > type_start && col >= start - 2 && col <= type_end {
                let type_name = &line[type_start..type_end];
                // Try to find to::typename function
                let func_name = format!("to::{}", type_name);
                return find_function_location(analysis, imports, &func_name, uri);
            }
            break;
        }
        start -= 1;
    }

    // Also try: cursor is directly on the type name after :~
    // Scan forward from cursor to find if we're inside :~<word>
    if col < bytes.len() {
        let mut scan = col;
        // Go backwards to find :~
        while scan > 1 {
            if bytes[scan - 1] == b'~' && scan >= 2 && bytes[scan - 2] == b':' {
                // We're after :~ — extract the type name
                let type_start = scan;
                let mut type_end = scan;
                while type_end < bytes.len() {
                    let ch = bytes[type_end] as char;
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        type_end += 1;
                    } else {
                        break;
                    }
                }
                if type_end > type_start {
                    let type_name = &line[type_start..type_end];
                    let func_name = format!("to::{}", type_name);
                    return find_function_location(analysis, imports, &func_name, uri);
                }
                break;
            }
            let ch = bytes[scan - 1] as char;
            if !ch.is_ascii_alphanumeric() && ch != '_' {
                break;
            }
            scan -= 1;
        }
    }

    None
}

/// Extract a `:-func::name` mapping if the cursor is on it.
fn extract_func_mapping_at(line: &str, col: usize) -> Option<String> {
    // Find all `:-` patterns in the line
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find(":-") {
        let abs_pos = search_start + pos;
        let func_start = abs_pos + 2;

        // Find the end of the function name (allow :: in names like argsh::docs)
        let mut func_end = func_start;
        let bytes = line.as_bytes();
        while func_end < bytes.len() {
            let ch = bytes[func_end] as char;
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' {
                func_end += 1;
            } else {
                break;
            }
        }

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

