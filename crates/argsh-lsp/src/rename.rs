use std::collections::HashMap;
use tower_lsp::lsp_types::*;

use argsh_syntax::document::DocumentAnalysis;

pub fn prepare_rename(
    analysis: &DocumentAnalysis,
    position: Position,
    content: &str,
) -> Option<PrepareRenameResponse> {
    let line_idx = position.line as usize;
    let col = position.character as usize;
    let lines: Vec<&str> = content.lines().collect();
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];

    // Extract word at cursor
    let word = extract_word_at(line, col);
    if word.is_empty() {
        return None;
    }

    // Check if it's a function name
    if analysis.functions.iter().any(|f| f.name == word) {
        return Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: word_range(line, col, &word, line_idx),
            placeholder: word,
        });
    }
    None
}

pub fn rename(
    analysis: &DocumentAnalysis,
    position: Position,
    new_name: &str,
    content: &str,
    uri: &Url,
) -> Option<WorkspaceEdit> {
    let line_idx = position.line as usize;
    let col = position.character as usize;
    let lines: Vec<&str> = content.lines().collect();
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];

    let old_name = extract_word_at(line, col);
    if old_name.is_empty() {
        return None;
    }

    // Check it's a function
    if !analysis.functions.iter().any(|f| f.name == old_name) {
        return None;
    }

    let mut edits: Vec<TextEdit> = Vec::new();

    // 1. Rename the function declaration
    for func in &analysis.functions {
        if func.name == old_name {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: func.line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: func.line as u32,
                        character: lines[func.line].len() as u32,
                    },
                },
                new_text: replace_word(lines[func.line], &old_name, new_name),
            });
        }
    }

    // 2. Update usage entries that reference this function
    for func in &analysis.functions {
        for entry in &func.usage_entries {
            // Check explicit mapping (e.g. :-old_name)
            if let Some(ref target) = entry.explicit_func {
                if target == &old_name && entry.line < lines.len() {
                    let old_line = lines[entry.line];
                    if !edits.iter().any(|e| e.range.start.line == entry.line as u32) {
                        edits.push(TextEdit {
                            range: Range {
                                start: Position {
                                    line: entry.line as u32,
                                    character: 0,
                                },
                                end: Position {
                                    line: entry.line as u32,
                                    character: old_line.len() as u32,
                                },
                            },
                            new_text: replace_word(old_line, &old_name, new_name),
                        });
                    }
                }
            }
        }
    }

    // 3. Update any other references in the file (function calls, etc.)
    for (i, line_str) in lines.iter().enumerate() {
        if line_str.contains(&old_name) && !edits.iter().any(|e| e.range.start.line == i as u32) {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: i as u32,
                        character: 0,
                    },
                    end: Position {
                        line: i as u32,
                        character: line_str.len() as u32,
                    },
                },
                new_text: replace_word(line_str, &old_name, new_name),
            });
        }
    }

    if edits.is_empty() {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

/// Replace occurrences of `old` with `new` only at word boundaries.
/// Word characters are ASCII alphanumeric, `_`, and `:`.
fn replace_word(line: &str, old: &str, new: &str) -> String {
    let mut result = String::new();
    let mut remaining = line;
    while let Some(pos) = remaining.find(old) {
        // Check word boundary before match
        let before_ok = pos == 0 || {
            let ch = remaining.as_bytes()[pos - 1] as char;
            !ch.is_ascii_alphanumeric() && ch != '_' && ch != ':' && ch != '-'
        };
        // Check word boundary after match
        let end = pos + old.len();
        let after_ok = end >= remaining.len() || {
            let ch = remaining.as_bytes()[end] as char;
            !ch.is_ascii_alphanumeric() && ch != '_' && ch != ':' && ch != '-'
        };

        if before_ok && after_ok {
            result.push_str(&remaining[..pos]);
            result.push_str(new);
        } else {
            result.push_str(&remaining[..end]);
        }
        remaining = &remaining[end..];
    }
    result.push_str(remaining);
    result
}

fn extract_word_at(line: &str, col: usize) -> String {
    let bytes = line.as_bytes();
    let len = bytes.len();
    if col >= len {
        return String::new();
    }
    let mut start = col;
    while start > 0 {
        let ch = bytes[start - 1] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = col;
    while end < len {
        let ch = bytes[end] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' {
            end += 1;
        } else {
            break;
        }
    }
    line[start..end].to_string()
}

fn word_range(line: &str, col: usize, word: &str, line_idx: usize) -> Range {
    // Find the word boundary that contains col (include - for hyphenated names)
    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 {
        let ch = bytes[start - 1] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '-' {
            start -= 1;
        } else {
            break;
        }
    }
    Range {
        start: Position {
            line: line_idx as u32,
            character: start as u32,
        },
        end: Position {
            line: line_idx as u32,
            character: (start + word.len()) as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argsh_syntax::document::analyze;

    #[test]
    fn test_prepare_rename_on_function() {
        let src = "#!/usr/bin/env argsh\nmain() {\n  echo hi\n}\n";
        let analysis = analyze(src);
        let result = prepare_rename(&analysis, Position { line: 1, character: 1 }, src);
        assert!(result.is_some());
    }

    #[test]
    fn test_prepare_rename_not_function() {
        let src = "#!/usr/bin/env argsh\necho hello\n";
        let analysis = analyze(src);
        let result = prepare_rename(&analysis, Position { line: 1, character: 1 }, src);
        assert!(result.is_none());
    }

    #[test]
    fn test_rename_function() {
        let src = "#!/usr/bin/env argsh\nmain() {\n  echo hi\n}\n";
        let analysis = analyze(src);
        let uri = Url::parse("file:///test.sh").unwrap();
        let result = rename(&analysis, Position { line: 1, character: 1 }, "app", src, &uri);
        assert!(result.is_some());
        let ws = result.unwrap();
        let changes = ws.changes.unwrap();
        let edits = changes.get(&uri).unwrap();
        // Should rename the declaration line
        assert!(edits.iter().any(|e| e.new_text.contains("app()")));
    }

    #[test]
    fn test_extract_word_at() {
        assert_eq!(extract_word_at("main() {", 1), "main");
        assert_eq!(extract_word_at("foo::bar() {", 5), "foo::bar");
        assert_eq!(extract_word_at("", 0), "");
    }
}
