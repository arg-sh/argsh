use tower_lsp::lsp_types::*;

/// Format an argsh script -- aligns args=() and usage=() array entries.
///
/// For each multi-line array, finds paired entries (spec + description),
/// calculates the maximum spec width, and aligns descriptions to that column.
///
/// Example output:
/// ```bash
/// local -a args=(
///   'files'             "Files to minify"
///   'template|t:~file'  "Path to template"
///   'out|o'             "Output file"
/// )
/// ```
pub fn format_document(content: &str) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if is_array_open(line, "args") || is_array_open(line, "usage") {
            let start = i + 1;
            let mut end = start;
            while end < lines.len() {
                if lines[end].trim().starts_with(')') || lines[end].trim() == ")" {
                    break;
                }
                end += 1;
            }

            if end > start && end < lines.len() {
                let entry_edits = format_array_entries(&lines, start, end);
                edits.extend(entry_edits);
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }

    edits
}

/// Check if a line opens an args=( or usage=( array (multi-line).
/// Handles cases like `local -a kind=() event=() args=(` where earlier
/// array inits have closing parens but args=( is still open.
fn is_array_open(line: &str, name: &str) -> bool {
    let trimmed = line.trim();
    let needle = format!("{}=(", name);
    if let Some(pos) = trimmed.find(&needle) {
        // Check there's no closing ')' after the args=( opening
        let after = &trimmed[pos + needle.len()..];
        !after.contains(')')
    } else {
        false
    }
}

/// Format array entries -- align specs and descriptions.
fn format_array_entries(lines: &[&str], start: usize, end: usize) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    // Parse each line into (line_idx, indent, spec, desc)
    let mut entries: Vec<(usize, usize, String, String)> = Vec::new();

    let mut line_idx = start;
    while line_idx < end {
        let line = lines[line_idx];
        let trimmed = line.trim();

        let indent = line.len() - line.trim_start().len();

        if let Some((spec_part, desc_part)) = split_entry(trimmed) {
            entries.push((line_idx, indent, spec_part, desc_part));
        }

        line_idx += 1;
    }

    if entries.is_empty() {
        return edits;
    }

    // Use the first entry's indent as the common indent
    let common_indent = entries[0].1;
    let indent_str = " ".repeat(common_indent);

    // Find the maximum spec width (including quotes)
    let max_spec_width = entries
        .iter()
        .map(|(_, _, spec, _)| spec.len())
        .max()
        .unwrap_or(0);

    // At least 2 spaces after the longest spec
    let desc_col = max_spec_width + 2;

    for (line_idx, _indent, spec, desc) in &entries {
        let padding = desc_col.saturating_sub(spec.len());
        let pad_str = " ".repeat(if padding < 1 { 1 } else { padding });

        let new_full = format!("{}{}{}{}", indent_str, spec, pad_str, desc);

        let current = lines[*line_idx];
        if current != new_full {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: *line_idx as u32,
                        character: 0,
                    },
                    end: Position {
                        line: *line_idx as u32,
                        character: current.len() as u32,
                    },
                },
                new_text: new_full,
            });
        }
    }

    edits
}

/// Split a line into (spec_with_quotes, description_with_quotes).
/// Handles: `'spec' "description"` and `'-' "group heading"`
fn split_entry(line: &str) -> Option<(String, String)> {
    let bytes = line.as_bytes();

    // Find first single quote (start of spec)
    let sq_start = bytes.iter().position(|&b| b == b'\'')?;
    // Find closing single quote
    let sq_end = bytes[sq_start + 1..].iter().position(|&b| b == b'\'')? + sq_start + 1;

    let spec = &line[sq_start..=sq_end];

    // Find the description after the spec — can be "double" or 'single' quoted
    let rest = &line[sq_end + 1..];
    let trimmed_rest = rest.trim_start();

    let desc = if trimmed_rest.starts_with('"') {
        // Double-quoted description
        trimmed_rest.trim_end()
    } else if trimmed_rest.starts_with('\'') {
        // Single-quoted description
        trimmed_rest.trim_end()
    } else {
        return None;
    };

    Some((spec.to_string(), desc.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_entry() {
        let (spec, desc) = split_entry("    'port|p:~int' \"Port number\"").unwrap();
        assert_eq!(spec, "'port|p:~int'");
        assert_eq!(desc, "\"Port number\"");
    }

    #[test]
    fn test_split_entry_single_quoted_description() {
        let (spec, desc) = split_entry("    'up|u@destructive' 'Start cluster'").unwrap();
        assert_eq!(spec, "'up|u@destructive'");
        assert_eq!(desc, "'Start cluster'");
    }

    #[test]
    fn test_split_entry_group_separator() {
        let (spec, desc) = split_entry("    '-' \"Options\"").unwrap();
        assert_eq!(spec, "'-'");
        assert_eq!(desc, "\"Options\"");
    }

    #[test]
    fn test_format_aligns_to_longest() {
        let input = "#!/usr/bin/env bash\nf() {\n  local -a args=(\n    'p|port:~int' \"Port\"\n    'v|verbose:+' \"Verbose output\"\n    'c' \"Config\"\n  )\n}\n";
        let edits = format_document(input);
        assert!(!edits.is_empty(), "Should produce alignment edits");

        // Apply edits to verify alignment
        let lines: Vec<&str> = input.lines().collect();
        let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        for edit in &edits {
            let line = edit.range.start.line as usize;
            result_lines[line] = edit.new_text.clone();
        }

        // All descriptions should start at the same column
        let desc_cols: Vec<usize> = result_lines[3..6]
            .iter()
            .filter_map(|l| l.find('"'))
            .collect();
        assert!(
            desc_cols.windows(2).all(|w| w[0] == w[1]),
            "Descriptions should be aligned: {:?}\n{}",
            desc_cols,
            result_lines[3..6].join("\n")
        );
    }

    #[test]
    fn test_format_no_edits_when_already_aligned() {
        // Single entry already in canonical form (2-space gap after spec)
        let input = "#!/usr/bin/env bash\nf() {\n  local -a args=(\n    'verbose|v:+'  \"Verbose\"\n  )\n}\n";
        let edits = format_document(input);
        assert!(
            edits.is_empty(),
            "Single aligned entry should produce no edits, got {} edits",
            edits.len()
        );
    }

    #[test]
    fn test_format_preserves_single_line_array() {
        // Single-line array like args=('x' "Y") should NOT be touched
        let input = "#!/usr/bin/env bash\nf() {\n  local -a args=('x' \"Y\")\n}\n";
        let edits = format_document(input);
        assert!(
            edits.is_empty(),
            "Single-line array should not be formatted"
        );
    }

    #[test]
    fn test_format_usage_array() {
        let input = "#!/usr/bin/env bash\nm() {\n  local -a usage=(\n    'serve|s' \"Start server\"\n    'build' \"Build project\"\n  )\n}\n";
        let edits = format_document(input);
        // 'serve|s' is longer than 'build', so 'build' line needs padding
        assert!(!edits.is_empty(), "Should produce alignment edits for usage array");
    }
}

    #[test]
    fn test_format_single_quoted_descriptions() {
        let input = "#!/usr/bin/env bash\nmain() {\n  local -a usage=(\n    'up|u@destructive' 'Start cluster'\n    'down@destructive' 'Stop cluster'\n    'provision|p@destructive' 'Provision a cluster (full lifecycle)'\n  )\n}\n";
        let edits = format_document(input);
        assert!(!edits.is_empty(), "Should produce alignment edits for single-quoted descriptions");

        let lines: Vec<&str> = input.lines().collect();
        let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        for edit in &edits {
            let line = edit.range.start.line as usize;
            if line < result_lines.len() {
                result_lines[line] = edit.new_text.clone();
            }
        }

        // All descriptions should start at the same column
        let desc_cols: Vec<usize> = result_lines[3..6].iter()
            .filter_map(|l| {
                // Find the second single quote pair (description start)
                let first_end = l.find('\'').and_then(|s| l[s+1..].find('\'').map(|e| s + 1 + e))?;
                l[first_end+1..].find('\'').map(|p| first_end + 1 + p)
            })
            .collect();
        assert!(desc_cols.windows(2).all(|w| w[0] == w[1]),
            "Single-quoted descriptions should be aligned: {:?}\n{}",
            desc_cols, result_lines[3..6].join("\n"));
    }

    #[test]
    fn test_format_mixed_quote_styles() {
        // Mix of single and double quoted descriptions
        let input = "#!/usr/bin/env bash\nm() {\n  local -a usage=(\n    'serve' \"Start server\"\n    'build' 'Build project'\n  )\n}\n";
        let edits = format_document(input);
        // Should handle both without crashing
        let lines: Vec<&str> = input.lines().collect();
        let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        for edit in &edits {
            let line = edit.range.start.line as usize;
            if line < result_lines.len() {
                result_lines[line] = edit.new_text.clone();
            }
        }
        // Both entries should still have their content
        assert!(result_lines[3].contains("serve"), "serve entry preserved");
        assert!(result_lines[4].contains("build"), "build entry preserved");
    }

    #[test]
    fn test_format_args_after_empty_array_inits() {
        // local -a kind=() event=() args=( — formatter should recognize args=( as open
        let input = "#!/usr/bin/env bash\nf() {\n  local -a kind=() event=() args=(\n    'handler:!' \"Handler function\"\n    'kind|k:!'  \"Resource kind\"\n  )\n}\n";
        let edits = format_document(input);
        let lines: Vec<&str> = input.lines().collect();
        let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        for edit in &edits {
            let line = edit.range.start.line as usize;
            if line < result_lines.len() {
                result_lines[line] = edit.new_text.clone();
            }
        }
        assert!(result_lines[3].contains("handler"), "handler entry found");
        assert!(result_lines[4].contains("kind"), "kind entry found");
        // Descriptions should be aligned
        let col3 = result_lines[3].find('"').unwrap();
        let col4 = result_lines[4].find('"').unwrap();
        assert_eq!(col3, col4, "descriptions should be aligned at same column");
    }
