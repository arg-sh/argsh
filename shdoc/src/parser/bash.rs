//! Bash @ annotation parser — line-by-line state machine.
//!
//! Mirrors the gawk shdoc script (`.bin/shdoc`, 927 lines) rule-by-rule
//! to produce byte-identical output for existing library files.

use crate::model::*;
use regex::Regex;
use std::sync::LazyLock;

// -- Regex patterns -----------------------------------------------------------

static RE_INTERNAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@internal").unwrap());

static RE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@(?:name|file)[[:space:]]+(.*)").unwrap());

static RE_BRIEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@brief[[:space:]]+(.*)").unwrap());

static RE_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@tags[[:space:]]+(.*)").unwrap());

static RE_DESCRIPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@description").unwrap());

static RE_SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@section[[:space:]]+(.*)").unwrap());

static RE_EXAMPLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@example").unwrap());

static RE_EXAMPLE_CONT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*# ").unwrap());

static RE_OPTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:blank:]]*#[[:blank:]]+@option[[:blank:]]+[^[:blank:]]").unwrap());

static RE_OPTION_EXTRACT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:blank:]]*#[[:blank:]]+@option[[:blank:]]+").unwrap());

// Complex regex for well-formed @option parsing (from shdoc line 428)
static RE_OPTION_VALID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(((-[[:alnum:]]([[:blank:]]*<[^>]+>)?",
        r"|--[[:alnum:]][[:alnum:]-]*((=|[[:blank:]]+)<[^>]+>)?)",
        r"([[:blank:]]*\|?[[:blank:]]+))+)",
        r"([^[:blank:]|<-].*)?$"
    ))
    .unwrap()
});

static RE_ARG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:blank:]]*#[[:blank:]]+@arg[[:blank:]]+[^[:blank:]]").unwrap());

static RE_ARG_EXTRACT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:blank:]]*#[[:blank:]]+@arg[[:blank:]]+").unwrap());

static RE_ARG_NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\$([0-9]+|@)\s").unwrap());

static RE_NOARGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:blank:]]+@noargs[[:blank:]]*$").unwrap());

static RE_SET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@set[[:space:]]+(.*)").unwrap());

static RE_EXITCODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@exitcode[[:space:]]+(.*)").unwrap());

static RE_SEE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:space:]]*#[[:space:]]+@see[[:space:]]+(.*)").unwrap());

// @stdin, @stdout, @stderr with capture for indentation and text
static RE_STD_IO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([[:blank:]]*#[[:blank:]]+)@(stdin|stdout|stderr)[[:blank:]]+(.*\S)[[:blank:]]*$")
        .unwrap()
});

// Function declaration with opening brace on same line
static RE_FUNC_DECL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[[:blank:]]*(function[[:blank:]]+)?([a-zA-Z0-9_:.:-]+)[[:blank:]]*(\([[:blank:]]*\))?[[:blank:]]*\{")
        .unwrap()
});

// Function declaration without opening brace (deferred)
static RE_FUNC_DECL_PARTIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[[:blank:]]*(function[[:blank:]]+)?([a-zA-Z0-9_:.:-]+)[[:blank:]]*(\([[:blank:]]*\))?[[:blank:]]*$")
        .unwrap()
});

static RE_LONE_BRACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:blank:]]*\{").unwrap());

static RE_EMPTY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[[:blank:]]*$").unwrap());

static RE_NON_COMMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^#]*$").unwrap());

// Description continuation exit condition (shdoc line 650):
// Exit when: line doesn't start with optional-whitespace-then-#,
// or has # @<not-d>, or is a non-comment, or is empty
static RE_DESC_EXIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[[:space:]]*# @[^d]").unwrap()
});

// -- Parser state -------------------------------------------------------------

#[derive(Default)]
struct ParserState {
    // Results
    file_doc: FileDoc,
    functions: Vec<FunctionDoc>,

    // Current docblock
    docblock: Docblock,

    // State flags
    in_description: bool,
    in_example: bool,
    is_internal: bool,
    current_section: Option<String>,
    section_description: Option<String>,
    file_description_set: bool,

    // Multi-line continuation
    multi_line_name: Option<String>,
    multi_line_indent_re: Option<Regex>,

    // Deferred function declaration
    function_declaration: Option<String>,

    // Current description accumulator
    description: String,
}

#[derive(Default)]
struct Docblock {
    example: Option<String>,
    args: Vec<(String, String)>, // (sort_key, raw_text)
    noargs: bool,
    options: Vec<OptionEntry>,
    options_bad: Vec<String>,
    set_vars: Vec<String>,
    exit_codes: Vec<String>,
    stdin: Vec<String>,
    stdout: Vec<String>,
    stderr: Vec<String>,
    see_also: Vec<String>,
    tags: Vec<String>,
}

impl Docblock {
    fn is_empty(&self) -> bool {
        self.example.is_none()
            && self.args.is_empty()
            && !self.noargs
            && self.options.is_empty()
            && self.options_bad.is_empty()
            && self.set_vars.is_empty()
            && self.exit_codes.is_empty()
            && self.stdin.is_empty()
            && self.stdout.is_empty()
            && self.stderr.is_empty()
            && self.see_also.is_empty()
    }
}

// -- Public API ---------------------------------------------------------------

/// Parse a bash source file with shdoc annotations into a Document.
pub fn parse(input: &str) -> Document {
    let mut state = ParserState::default();

    for line in input.lines() {
        process_line(&mut state, line);
    }

    // Finalize (matches gawk END block)
    Document {
        file: state.file_doc,
        functions: state.functions,
    }
}

// -- Line processing ----------------------------------------------------------

fn process_line(s: &mut ParserState, line: &str) {
    // 1. @internal (shdoc line 608)
    if RE_INTERNAL.is_match(line) {
        s.is_internal = true;
        return;
    }

    // 2. @file / @name (shdoc line 615)
    if let Some(caps) = RE_FILE.captures(line) {
        s.file_doc.title = Some(caps[1].to_string());
        return;
    }

    // 3. @brief (shdoc line 623)
    if let Some(caps) = RE_BRIEF.captures(line) {
        s.file_doc.brief = Some(caps[1].to_string());
        return;
    }

    // 4. @tags (shdoc line 631) — file-level or function-level
    //    File-level: before any function has been processed
    //    Function-level: when building a docblock (has content or description)
    if let Some(caps) = RE_TAGS.captures(line) {
        let raw = caps[1].to_string();
        if s.functions.is_empty() && s.docblock.is_empty() && s.description.is_empty() {
            // File-level tags
            s.file_doc.tags = Some(raw);
        } else {
            // Function-level tags
            let parsed: Vec<String> = raw.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
            s.docblock.tags = parsed;
        }
        return;
    }

    // 5. @description start (shdoc line 639)
    if RE_DESCRIPTION.is_match(line) {
        s.in_description = true;
        s.in_example = false;

        handle_description(s);
        reset_docblock(s);

        // Extract text after @description on the same line
        let stripped = RE_DESCRIPTION.replace(line, "").to_string();
        let stripped = strip_comment_prefix(&stripped);
        if !stripped.is_empty() {
            s.description = stripped;
        }
        return;
    }

    // 6. In-description continuation (shdoc line 649)
    if s.in_description {
        if should_exit_description(line) {
            s.in_description = false;
            handle_description(s);
            // Fall through to process the current line
        } else {
            // Accumulate description
            let mut text = line.to_string();
            // Remove leading "# @description " or "# " or "#"
            text = strip_description_line(&text);
            concat_str(&mut s.description, &text);
            return;
        }
    }

    // 7. @section (shdoc line 667)
    if let Some(caps) = RE_SECTION.captures(line) {
        s.current_section = Some(caps[1].to_string());
        return;
    }

    // 8. @example start (shdoc line 675)
    if RE_EXAMPLE.is_match(line) {
        s.in_example = true;
        return;
    }

    // 9. In-example continuation (shdoc line 684)
    if s.in_example {
        if !RE_EXAMPLE_CONT.is_match(line) {
            s.in_example = false;
            // Fall through
        } else {
            let mut text = line.to_string();
            // Remove leading "# "
            if let Some(pos) = text.find('#') {
                text = text[pos + 1..].to_string();
            }
            if let Some(ref mut ex) = s.docblock.example {
                ex.push('\n');
                ex.push_str(&text);
            } else {
                s.docblock.example = Some(text);
            }
            return;
        }
    }

    // 10. @option (shdoc line 699)
    if RE_OPTION.is_match(line) {
        let text = RE_OPTION_EXTRACT.replace(line, "").trim().to_string();
        process_at_option(s, &text);
        return;
    }

    // 11. @arg (shdoc line 717)
    if RE_ARG.is_match(line) {
        let text = RE_ARG_EXTRACT.replace(line, "").trim().to_string();

        if let Some(caps) = RE_ARG_NUMBERED.captures(&text) {
            let arg_number = &caps[1];
            let sort_key = if arg_number == "@" {
                "@".to_string()
            } else {
                format!("{:03}", arg_number.parse::<u32>().unwrap_or(0))
            };
            s.docblock.args.push((sort_key, text));
        } else {
            // Badly formatted @arg, process as @option (shdoc line 752)
            process_at_option(s, &text);
        }
        return;
    }

    // 12. @noargs (shdoc line 759)
    if RE_NOARGS.is_match(line) {
        s.docblock.noargs = true;
        return;
    }

    // 13. @set (shdoc line 767)
    if let Some(caps) = RE_SET.captures(line) {
        s.docblock.set_vars.push(caps[1].to_string());
        return;
    }

    // 13. @exitcode (shdoc line 776)
    if let Some(caps) = RE_EXITCODE.captures(line) {
        s.docblock.exit_codes.push(caps[1].to_string());
        return;
    }

    // 13. @see (shdoc line 785)
    if let Some(caps) = RE_SEE.captures(line) {
        s.docblock.see_also.push(caps[1].to_string());
        return;
    }

    // 14. Multi-line continuation for stdin/stdout/stderr (shdoc line 797)
    if let Some(ref name) = s.multi_line_name.clone() {
        if let Some(ref re) = s.multi_line_indent_re {
            if re.is_match(line) {
                // Append to last entry
                let text = strip_comment_content(line);
                let list = match name.as_str() {
                    "stdin" => &mut s.docblock.stdin,
                    "stdout" => &mut s.docblock.stdout,
                    "stderr" => &mut s.docblock.stderr,
                    _ => return,
                };
                if let Some(last) = list.last_mut() {
                    last.push('\n');
                    last.push_str(&text);
                }
                return;
            }
        }
        s.multi_line_name = None;
        s.multi_line_indent_re = None;
    }

    // 15. @stdin/@stdout/@stderr (shdoc line 821)
    if let Some(caps) = RE_STD_IO.captures(line) {
        let indentation = &caps[1];
        let docblock_name = caps[2].to_string();
        let text = caps[3].to_string();

        let list = match docblock_name.as_str() {
            "stdin" => &mut s.docblock.stdin,
            "stdout" => &mut s.docblock.stdout,
            "stderr" => &mut s.docblock.stderr,
            _ => return,
        };
        list.push(text);

        // Set up multi-line continuation
        let indent_pattern = format!(
            r"^{}[[:blank:]]+[^[:blank:]].*$",
            regex::escape(indentation)
        );
        s.multi_line_name = Some(docblock_name);
        s.multi_line_indent_re = Regex::new(&indent_pattern).ok();
        return;
    }

    // 16. Function declaration with { (shdoc line 846)
    if RE_FUNC_DECL.is_match(line) {
        process_function(s, line);
        return;
    }

    // 17. Function declaration without { (shdoc line 852)
    if RE_FUNC_DECL_PARTIAL.is_match(line) {
        s.function_declaration = Some(line.to_string());
        return;
    }

    // 18. Lone { after stored declaration (shdoc line 861)
    if RE_LONE_BRACE.is_match(line) {
        if let Some(decl) = s.function_declaration.take() {
            process_function(s, &decl);
            return;
        }
    }

    // Empty line while waiting for { (shdoc line 870)
    if RE_EMPTY.is_match(line) && s.function_declaration.is_some() {
        return;
    }

    // Non-comment line → reset (shdoc line 877)
    if RE_NON_COMMENT.is_match(line) {
        s.function_declaration = None;
        handle_description(s);
        reset_docblock(s);
    }
}

// -- Helper functions ---------------------------------------------------------

/// Process a function declaration and add to results.
fn process_function(s: &mut ParserState, line: &str) {
    // Skip if docblock is empty and description is empty (shdoc line 100-108)
    if s.docblock.is_empty() && s.description.is_empty() {
        return;
    }
    // Skip if we're inside an example block
    if s.in_example {
        return;
    }

    let internal = s.is_internal;
    s.is_internal = false;

    // Extract function name (shdoc line 119-124)
    let func_name = extract_func_name(line);

    let section = if let Some(title) = s.current_section.take() {
        let desc = s.section_description.take();
        Some(SectionInfo {
            title,
            description: desc,
        })
    } else {
        None
    };

    // Sort args by sort_key
    s.docblock.args.sort_by(|a, b| a.0.cmp(&b.0));

    let func_doc = FunctionDoc {
        name: func_name,
        description: if s.description.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut s.description))
        },
        section,
        example: s.docblock.example.take(),
        is_internal: internal,
        args: s
            .docblock
            .args
            .drain(..)
            .map(|(sort_key, raw)| ArgEntry { sort_key, raw })
            .collect(),
        noargs: s.docblock.noargs,
        options: std::mem::take(&mut s.docblock.options),
        options_bad: std::mem::take(&mut s.docblock.options_bad),
        set_vars: std::mem::take(&mut s.docblock.set_vars),
        exit_codes: std::mem::take(&mut s.docblock.exit_codes),
        stdin: std::mem::take(&mut s.docblock.stdin),
        stdout: std::mem::take(&mut s.docblock.stdout),
        stderr: std::mem::take(&mut s.docblock.stderr),
        see_also: std::mem::take(&mut s.docblock.see_also),
        tags: std::mem::take(&mut s.docblock.tags),
        implementations: Vec::new(),
    };

    s.functions.push(func_doc);

    reset_docblock(s);
}

/// Extract function name from a declaration line.
fn extract_func_name(line: &str) -> String {
    // Try with brace first
    if let Some(caps) = RE_FUNC_DECL.captures(line) {
        return caps[2].to_string();
    }
    // Try without brace
    if let Some(caps) = RE_FUNC_DECL_PARTIAL.captures(line) {
        return caps[2].to_string();
    }
    // Fallback: use the whole line trimmed
    line.trim().to_string()
}

/// Handle accumulated description (shdoc line 239).
/// Cascade: section_description → file_description → discard.
fn handle_description(s: &mut ParserState) {
    // Trim leading/trailing whitespace and newlines
    let desc = s.description.trim().to_string();
    if desc.is_empty() {
        return;
    }

    if s.current_section.is_some() && s.section_description.is_none() {
        s.section_description = Some(desc);
        // Don't clear description — matches gawk behavior where description
        // is preserved until reset() is called
        return;
    }

    if !s.file_description_set {
        s.file_doc.description = Some(desc);
        s.file_description_set = true;
        // Don't clear description — it stays for potential function use
    }

    // Already set — description stays for the next function (matches gawk behavior)
}

/// Reset the docblock (shdoc line 232).
fn reset_docblock(s: &mut ParserState) {
    s.docblock = Docblock::default();
    s.description.clear();
}

/// Process @option text (shdoc line 425).
fn process_at_option(s: &mut ParserState, text: &str) {
    if let Some(caps) = RE_OPTION_VALID.captures(text) {
        let term = caps[1].trim().to_string();
        // Trim spaces around pipes
        let term = term
            .split('|')
            .map(|p| p.trim())
            .collect::<Vec<_>>()
            .join(" | ");
        let definition = caps
            .get(8)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        s.docblock.options.push(OptionEntry { term, definition });
    } else {
        s.docblock.options_bad.push(text.to_string());
    }
}

/// Check if we should exit description mode (shdoc line 650).
fn should_exit_description(line: &str) -> bool {
    // Exit on: non-comment line, empty line, or # @<not-d>
    if RE_EMPTY.is_match(line) {
        return true;
    }
    if !line.trim_start().starts_with('#') {
        return true;
    }
    if RE_DESC_EXIT.is_match(line) {
        return true;
    }
    false
}

/// Strip comment prefix for description lines.
/// Matches gawk: `sub(/^[[:space:]]*#[[:space:]]*/, "")`
fn strip_description_line(line: &str) -> String {
    let s = line.trim_start();
    if let Some(rest) = s.strip_prefix("# @description") {
        return rest.trim_start().to_string();
    }
    if let Some(rest) = s.strip_prefix('#') {
        return rest.trim_start().to_string();
    }
    s.to_string()
}

/// Strip leading "# " from comment content for multi-line items.
fn strip_comment_content(line: &str) -> String {
    let s = line.trim_start();
    if let Some(rest) = s.strip_prefix("# ") {
        rest.trim_end().to_string()
    } else if let Some(rest) = s.strip_prefix('#') {
        rest.trim().to_string()
    } else {
        s.trim_end().to_string()
    }
}

/// Strip comment prefix for any comment line.
fn strip_comment_prefix(line: &str) -> String {
    let s = line.trim_start();
    if let Some(rest) = s.strip_prefix("# ") {
        return rest.to_string();
    }
    if s == "#" {
        return String::new();
    }
    s.to_string()
}

/// Concatenate strings with newline separator (matches gawk `concat()`).
fn concat_str(dest: &mut String, text: &str) {
    if dest.is_empty() {
        *dest = text.to_string();
    } else {
        dest.push('\n');
        dest.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_function() {
        let input = r#"# @file test
# @description Test file
# @description
#   A simple function
# @arg $1 string The value
# @exitcode 0 Success
func() {
  echo "$1"
}
"#;
        let doc = parse(input);
        assert_eq!(doc.file.title.as_deref(), Some("test"));
        assert_eq!(doc.file.description.as_deref(), Some("Test file"));
        assert_eq!(doc.functions.len(), 1);
        assert_eq!(doc.functions[0].name, "func");
        assert_eq!(
            doc.functions[0].description.as_deref(),
            Some("A simple function")
        );
        assert_eq!(doc.functions[0].args.len(), 1);
        assert_eq!(doc.functions[0].exit_codes.len(), 1);
    }

    #[test]
    fn parse_namespaced_function() {
        let input = r#"# @description Check array
# @arg $1 string The name
is::array() {
  true
}
"#;
        let doc = parse(input);
        assert_eq!(doc.functions[0].name, "is::array");
    }

    #[test]
    fn parse_internal_preserved() {
        let input = r#"# @internal
# @description Internal function
_helper() {
  true
}
# @description Public function
public() {
  true
}
"#;
        let doc = parse(input);
        assert_eq!(doc.functions.len(), 2);
        assert_eq!(doc.functions[0].name, "_helper");
        assert!(doc.functions[0].is_internal);
        assert_eq!(doc.functions[1].name, "public");
        assert!(!doc.functions[1].is_internal);
    }

    #[test]
    fn parse_example() {
        let input = r#"# @description Trim
# @example
#   string::trim "  hello  "
func() { true; }
"#;
        let doc = parse(input);
        assert!(doc.functions[0].example.is_some());
    }

    #[test]
    fn parse_noargs() {
        let input = r#"# @description No args function
# @noargs
func() { true; }
"#;
        let doc = parse(input);
        assert!(doc.functions[0].noargs);
    }

    #[test]
    fn parse_deferred_brace() {
        let input = r#"# @description Deferred
func()
{
  true
}
"#;
        let doc = parse(input);
        assert_eq!(doc.functions.len(), 1);
        assert_eq!(doc.functions[0].name, "func");
    }

    #[test]
    fn parse_tags() {
        let input = "# @file test\n# @tags core, builtin\n";
        let doc = parse(input);
        assert_eq!(doc.file.tags.as_deref(), Some("core, builtin"));
    }

    #[test]
    fn parse_section() {
        let input = r#"# @file test
# @section Install
# @description Install helpers
# @description Install the thing
# @arg $1 string Path
install() { true; }
"#;
        let doc = parse(input);
        assert_eq!(doc.functions.len(), 1);
        assert!(doc.functions[0].section.is_some());
        let sec = doc.functions[0].section.as_ref().unwrap();
        assert_eq!(sec.title, "Install");
    }
}
