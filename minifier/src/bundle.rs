//! Source-file bundling for bash scripts.
//!
//! Resolves `import`, `source`, and `.` statements, recursively inlining
//! referenced files to produce a single self-contained bash script.
//!
//! Runs **before** the strip phase so that `import` lines are visible for
//! resolution before strip removes them.
//!
//! ## Dedup rules
//!
//! | Context | Behavior |
//! |---------|----------|
//! | Top-level (brace depth == 0) | Dedup: skip if already inlined |
//! | Inside function body (brace depth > 0) | Always inline |
//! | `# minifier force source` annotation | Always inline |

use crate::quote::QuoteTracker;
use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Maximum recursion depth to prevent infinite loops.
const MAX_DEPTH: usize = 64;

/// Matches `import <target>` (not dynamic, not array elements).
static RE_IMPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*import\s+([^\s;#]+)\s*$").unwrap());

/// Matches `source <path>` with optional quotes.
static RE_SOURCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^[ \t]*source\s+["']?([^"'\s;#]+)["']?"#).unwrap());

/// Matches `. <path>` (dot-source) with optional quotes.
static RE_DOT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^[ \t]*\.\s+["']?([^"'\s;#]+)["']?"#).unwrap());

/// Configuration for the bundle phase.
pub struct BundleConfig {
    /// Directories to search for imports (in order).
    pub search_paths: Vec<PathBuf>,
}

/// Bundle a bash script by resolving and inlining imports.
///
/// Returns the bundled source with all resolvable imports inlined.
pub fn bundle(source: &str, input_path: &Path, config: &BundleConfig) -> Result<String> {
    let current_dir = input_path
        .parent()
        .unwrap_or_else(|| Path::new(".")) // coverage:off - parent() always Some for valid paths
        .to_path_buf();
    let mut seen = HashSet::new();
    let lines = bundle_recursive(source, &current_dir, config, &mut seen, 0)?;
    Ok(lines.join("\n"))
}

/// Strip `@` or `~` prefix from an import target.
fn strip_import_prefix(target: &str) -> &str {
    target
        .strip_prefix('@')
        .or_else(|| target.strip_prefix('~'))
        .unwrap_or(target)
}

/// Resolve an import target to a file path.
///
/// Resolution order:
/// 1. Relative to `current_dir`
/// 2. Each search path in order
///
/// For each candidate directory, tries: as-is, `.sh`, `.bash`.
fn resolve_path(target: &str, current_dir: &Path, config: &BundleConfig) -> Option<PathBuf> {
    let stripped = strip_import_prefix(target);
    // Reject path traversal and absolute paths
    if stripped.contains("..") || stripped.starts_with('/') {
        return None;
    }
    let extensions = ["", ".sh", ".bash"];

    // Try current directory first, then each search path
    let dirs = std::iter::once(current_dir.to_path_buf())
        .chain(config.search_paths.iter().cloned());

    for dir in dirs {
        for ext in &extensions {
            let candidate = dir.join(format!("{stripped}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Count the net brace depth change for a line, excluding `${...}` expansions.
///
/// Only counts `{` and `}` that are outside quotes and not part of
/// parameter expansions like `${var}`, `${var:-default}`, etc.
/// Uses a separate counter for `${` depth so matching `}` don't affect
/// the block brace count.
fn brace_depth_delta(line: &str) -> i32 {
    let mut delta: i32 = 0;
    let mut param_depth: i32 = 0; // tracks nested ${...} expansions
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = line.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { '\0' };
        // Count consecutive preceding backslashes. Odd = escaped, even = not.
        // Inside single quotes backslash is literal, so skip escape check.
        let is_escaped = if !in_single {
            let mut backslashes = 0;
            let mut j = i;
            while j > 0 && chars[j - 1] == '\\' {
                backslashes += 1;
                j -= 1;
            }
            backslashes % 2 == 1
        } else {
            false
        };

        match ch {
            '\'' if !in_double && (in_single || !is_escaped) => {
                in_single = !in_single;
            }
            '"' if !in_single && !is_escaped => {
                in_double = !in_double;
            }
            '{' if !in_single => {
                if prev == '$' {
                    // `${` — parameter expansion, track separately
                    param_depth += 1;
                } else if param_depth == 0 && !in_double {
                    // Block brace (function body, if/while blocks)
                    delta += 1;
                }
            }
            '}' if !in_single => {
                if param_depth > 0 {
                    // Closes a `${...}` expansion
                    param_depth -= 1;
                } else if !in_double {
                    // Closes a block brace
                    delta -= 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    delta
}

/// Extract the import target from a line, if it matches an import pattern.
///
/// Returns `None` if the line doesn't match or if the target contains `$`
/// (dynamic path — can't resolve statically).
fn extract_target(line: &str) -> Option<String> {
    for re in [&*RE_IMPORT, &*RE_SOURCE, &*RE_DOT] {
        if let Some(caps) = re.captures(line) {
            let target = caps.get(1).unwrap().as_str();
            // Can't resolve dynamic paths
            if target.contains('$') {
                return None;
            }
            return Some(target.to_string());
        }
    }
    None
}

/// Recursively bundle a source string, inlining resolved imports.
fn bundle_recursive(
    source: &str,
    current_dir: &Path,
    config: &BundleConfig,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<Vec<String>> {
    if depth > MAX_DEPTH {
        anyhow::bail!("bundle: maximum recursion depth ({MAX_DEPTH}) exceeded");
    }

    let mut output = Vec::new();
    let mut brace_depth: i32 = 0;
    let mut force_next = false;
    let mut open_quote_char: Option<char> = None;

    for line in source.lines() {
        // If we're inside a multi-line quoted string, emit as-is
        if let Some(qchar) = open_quote_char {
            output.push(line.to_string());
            // Close when the line contains the opening quote character
            if line.contains(qchar) {
                open_quote_char = None;
            }
            continue;
        }

        // Check for force annotation
        let trimmed = line.trim();
        if trimmed == "# minifier force source" {
            force_next = true;
            // Don't emit the annotation itself
            continue;
        }

        // Try to extract an import target
        if let Some(target) = extract_target(line) {
            if let Some(resolved) = resolve_path(&target, current_dir, config) {
                let canonical = resolved
                    .canonicalize()
                    .unwrap_or_else(|_| resolved.clone()); // coverage:off - canonicalize fallback for filesystem errors

                let is_top_level = brace_depth == 0;
                let should_dedup = is_top_level && !force_next && seen.contains(&canonical);

                if should_dedup {
                    // Already inlined at top level — skip
                    force_next = false;
                    continue;
                }

                seen.insert(canonical.clone());
                let content = std::fs::read_to_string(&resolved)?;
                let child_dir = resolved
                    .parent()
                    .unwrap_or_else(|| Path::new(".")) // coverage:off - parent() always Some for resolved paths
                    .to_path_buf();
                let inlined =
                    bundle_recursive(&content, &child_dir, config, seen, depth + 1)?;
                output.extend(inlined);
                force_next = false;
                continue;
            }
            // Not found — emit line as-is (strip phase will handle `import` lines)
        }

        output.push(line.to_string());
        force_next = false;

        // Update brace depth
        brace_depth += brace_depth_delta(line);
        if brace_depth < 0 {
            brace_depth = 0; // safety clamp
        }

        // Track open quotes for multi-line strings
        let (sq, dq) = QuoteTracker::line_has_open_quote(line);
        if sq {
            open_quote_char = Some('\'');
        } else if dq {
            open_quote_char = Some('"');
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_config(search_paths: Vec<PathBuf>) -> BundleConfig {
        BundleConfig { search_paths }
    }

    #[test]
    fn simple_import() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("foo.sh"), "echo foo\n").unwrap();
        fs::write(dir.path().join("main.sh"), "import foo\necho main\n").unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("echo foo"), "Got: {result}");
        assert!(result.contains("echo main"), "Got: {result}");
    }

    #[test]
    fn source_inline() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("lib.sh"), "echo lib\n").unwrap();
        fs::write(
            dir.path().join("main.sh"),
            "source ./lib.sh\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("echo lib"), "Got: {result}");
        assert!(result.contains("echo main"), "Got: {result}");
    }

    #[test]
    fn dot_source_inline() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("helper.sh"), "echo helper\n").unwrap();
        fs::write(
            dir.path().join("main.sh"),
            ". ./helper.sh\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("echo helper"), "Got: {result}");
    }

    #[test]
    fn extension_probing() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("utils.sh"), "echo utils\n").unwrap();
        fs::write(dir.path().join("main.sh"), "import utils\necho main\n").unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("echo utils"), "Got: {result}");
    }

    #[test]
    fn search_path_resolution() {
        let dir = TempDir::new().unwrap();
        let libs = dir.path().join("libs");
        fs::create_dir(&libs).unwrap();
        fs::write(libs.join("mylib.sh"), "echo mylib\n").unwrap();
        fs::write(dir.path().join("main.sh"), "import mylib\necho main\n").unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(
            &source,
            &dir.path().join("main.sh"),
            &make_config(vec![libs]),
        )
        .unwrap();
        assert!(result.contains("echo mylib"), "Got: {result}");
    }

    #[test]
    fn unresolvable_left_as_is() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("main.sh"),
            "import nonexistent\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("import nonexistent"), "Got: {result}");
        assert!(result.contains("echo main"), "Got: {result}");
    }

    #[test]
    fn variable_in_path_left_as_is() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("main.sh"),
            "source \"${CONFIG}\"\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("${CONFIG}"), "Got: {result}");
    }

    #[test]
    fn top_level_dedup() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("shared.sh"), "echo shared\n").unwrap();
        fs::write(
            dir.path().join("main.sh"),
            "import shared\nimport shared\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        // "echo shared" should appear exactly once
        let count = result.matches("echo shared").count();
        assert_eq!(count, 1, "Expected 1 occurrence, got {count} in: {result}");
    }

    #[test]
    fn scoped_no_dedup() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("lib.sh"), "echo lib\n").unwrap();
        fs::write(
            dir.path().join("main.sh"),
            "import lib\nfoo() {\nimport lib\n}\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        // "echo lib" should appear twice — once top-level, once inside function
        let count = result.matches("echo lib").count();
        assert_eq!(count, 2, "Expected 2 occurrences, got {count} in: {result}");
    }

    #[test]
    fn force_annotation_overrides_dedup() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("lib.sh"), "echo lib\n").unwrap();
        fs::write(
            dir.path().join("main.sh"),
            "import lib\n# minifier force source\nimport lib\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        let count = result.matches("echo lib").count();
        assert_eq!(count, 2, "Expected 2 occurrences, got {count} in: {result}");
    }

    #[test]
    fn recursive_bundling() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("c.sh"), "echo c\n").unwrap();
        fs::write(dir.path().join("b.sh"), "import c\necho b\n").unwrap();
        fs::write(dir.path().join("a.sh"), "import b\necho a\n").unwrap();

        let source = fs::read_to_string(dir.path().join("a.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("a.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("echo c"), "Got: {result}");
        assert!(result.contains("echo b"), "Got: {result}");
        assert!(result.contains("echo a"), "Got: {result}");
    }

    #[test]
    fn circular_import_deduped() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.sh"), "import b\necho a\n").unwrap();
        fs::write(dir.path().join("b.sh"), "import a\necho b\n").unwrap();

        let source = fs::read_to_string(dir.path().join("a.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("a.sh"), &make_config(vec![])).unwrap();
        // Both should appear, but no infinite loop
        assert!(result.contains("echo a"), "Got: {result}");
        assert!(result.contains("echo b"), "Got: {result}");
    }

    #[test]
    fn at_prefix_stripped() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("core.sh"), "echo core\n").unwrap();
        fs::write(dir.path().join("main.sh"), "import @core\necho main\n").unwrap();

        let source = fs::read_to_string(dir.path().join("main.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(result.contains("echo core"), "Got: {result}");
    }

    #[test]
    fn multiline_quote_not_broken_by_import() {
        // A multi-line quoted string spanning 3+ lines must not have
        // middle lines misinterpreted as import statements.
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("lib.sh"), "echo lib\n").unwrap();
        let source = "echo \"line1\nimport lib\nline3\"\necho after\n";
        let result = bundle(source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        // The `import lib` is inside quotes — should NOT be inlined
        assert!(result.contains("import lib"), "import inside quotes should stay, got: {result}");
        assert!(result.contains("echo after"), "Got: {result}");
        // `echo lib` should NOT appear — the import was inside a string
        assert!(!result.contains("echo lib"), "import inside quotes should not be resolved, got: {result}");
    }

    #[test]
    fn brace_depth_delta_function() {
        assert_eq!(brace_depth_delta("foo() {"), 1);
        assert_eq!(brace_depth_delta("}"), -1);
        assert_eq!(brace_depth_delta("echo ${var}"), 0);
        assert_eq!(brace_depth_delta("foo() { echo ${x}; }"), 0);
        assert_eq!(brace_depth_delta("if true; then {"), 1);
    }

    #[test]
    fn brace_depth_quoted() {
        // Braces inside quotes don't count
        assert_eq!(brace_depth_delta("echo '{'"), 0);
        assert_eq!(brace_depth_delta("echo \"{\""), 0);
    }

    #[test]
    fn brace_depth_escaped_quote_with_brace() {
        // An escaped double quote `\"` before a `{` exercises the backslash
        // counting loop (lines 118-121) in brace_depth_delta.
        // `echo \"{ ` — the \" is an escaped quote (not a real string delimiter),
        // so the `{` is a real block brace outside quotes.
        assert_eq!(brace_depth_delta(r#"echo \"{  "#), 1);
        // Double backslash before quote: `\\"` — even backslashes, so the `"`
        // is a real opening quote, and `{` is inside double-quotes (ignored).
        assert_eq!(brace_depth_delta(r#"echo \\"{ "#), 0);
    }

    #[test]
    fn max_depth_exceeded() {
        // Trigger the MAX_DEPTH bail (line 186) by creating a chain > 64 deep.
        let dir = TempDir::new().unwrap();
        // Create files: d0.sh imports d1, d1 imports d2, ..., d65 imports d66
        for i in 0..=65 {
            let content = format!("import d{}\necho d{i}\n", i + 1);
            fs::write(dir.path().join(format!("d{i}.sh")), content).unwrap();
        }
        fs::write(dir.path().join("d66.sh"), "echo leaf\n").unwrap();

        let source = fs::read_to_string(dir.path().join("d0.sh")).unwrap();
        let result = bundle(&source, &dir.path().join("d0.sh"), &make_config(vec![]));
        assert!(result.is_err(), "Should fail with max depth exceeded");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("maximum recursion depth"),
            "Error should mention depth, got: {err}"
        );
    }

    #[test]
    fn negative_brace_depth_clamped() {
        // A source with more `}` than `{` triggers the brace_depth < 0 clamp (line 250).
        // After the clamp, imports at apparent top-level should still dedup.
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("lib.sh"), "echo lib\n").unwrap();
        // `}` without matching `{` pushes brace depth negative, then it clamps to 0.
        let source = "}\nimport lib\nimport lib\necho main\n";
        let result = bundle(source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        // After clamping to 0, the second import should be deduped
        let count = result.matches("echo lib").count();
        assert_eq!(count, 1, "Expected dedup after clamp, got {count} in: {result}");
    }

    #[test]
    fn open_single_quote_multiline_in_bundle() {
        // A multi-line single-quoted string should not have middle lines
        // parsed as imports. Tests the open_quote_char = Some('\'') path (line 256).
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("lib.sh"), "echo lib\n").unwrap();
        let source = "echo 'start\nimport lib\nend'\necho after\n";
        let result = bundle(source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        // The import inside single quotes should NOT be inlined
        assert!(
            result.contains("import lib"),
            "import inside single quotes should stay, got: {result}"
        );
        assert!(!result.contains("echo lib"), "should not resolve import inside quotes, got: {result}");
    }

    #[test]
    fn path_traversal_rejected() {
        let dir = TempDir::new().unwrap();
        // Create a file outside the expected scope
        fs::write(dir.path().join("secret.sh"), "echo secret\n").unwrap();
        let subdir = dir.path().join("sub");
        fs::create_dir(&subdir).unwrap();
        fs::write(
            subdir.join("main.sh"),
            "import ../secret\necho main\n",
        )
        .unwrap();

        let source = fs::read_to_string(subdir.join("main.sh")).unwrap();
        let result = bundle(&source, &subdir.join("main.sh"), &make_config(vec![])).unwrap();
        // The traversal import should NOT be resolved
        assert!(!result.contains("echo secret"), "Path traversal should be rejected, got: {result}");
        assert!(result.contains("import ../secret"), "Unresolved import should remain, got: {result}");
    }

    #[test]
    fn absolute_path_rejected() {
        let dir = TempDir::new().unwrap();
        let source = "import /etc/passwd\necho main\n";
        let result = bundle(source, &dir.path().join("main.sh"), &make_config(vec![])).unwrap();
        assert!(!result.contains("root:"), "Absolute path should be rejected, got: {result}");
    }

    #[test]
    fn extract_target_patterns() {
        assert_eq!(extract_target("import foo"), Some("foo".into()));
        assert_eq!(extract_target("  import bar"), Some("bar".into()));
        assert_eq!(extract_target("source ./lib.sh"), Some("./lib.sh".into()));
        assert_eq!(
            extract_target("source \"./lib.sh\""),
            Some("./lib.sh".into())
        );
        assert_eq!(extract_target(". ./helper.sh"), Some("./helper.sh".into()));
        assert_eq!(extract_target("source \"${X}\""), None);
        assert_eq!(extract_target("echo hello"), None);
    }
}
