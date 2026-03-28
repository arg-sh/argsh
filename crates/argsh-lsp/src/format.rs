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
fn is_array_open(line: &str, name: &str) -> bool {
    let trimmed = line.trim();
    let needle = format!("{}=(", name);
    trimmed.contains(&needle) && !trimmed.contains(')')
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

    // Find first single quote
    let sq_start = bytes.iter().position(|&b| b == b'\'')?;
    // Find closing single quote
    let sq_end = bytes[sq_start + 1..].iter().position(|&b| b == b'\'')? + sq_start + 1;

    let spec = &line[sq_start..=sq_end];

    // Find the description: starts with " after the spec
    let rest = &line[sq_end + 1..];
    let dq_start = rest.find('"')?;
    let desc = &rest[dq_start..];

    let desc = desc.trim_end();

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
