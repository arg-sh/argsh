//! Line-level stripping for bash minification.
//!
//! Removes entire lines that are unnecessary in minified output:
//! comments, shebangs, blank lines, `import` calls/definitions,
//! and `set -euo pipefail`. Also handles mid-line shebangs caused
//! by `cat` concatenation of files without trailing newlines.

use regex::Regex;
use std::sync::LazyLock;

static RE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*#.*").unwrap());
static RE_BLANK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*$").unwrap());
/// Matches simple import calls: `import string`, `import fmt`, etc.
/// Only matches when followed by a simple word — avoids stripping array
/// elements like `    import import::clear)`.
static RE_IMPORT_CALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*import\s+\w+\s*$").unwrap());
/// Matches complete single-line import function definitions: `import() { ... }`.
/// Multi-line definitions (e.g. ` import() { \n body \n }`) are kept intact
/// to avoid orphaning their function body.
static RE_IMPORT_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t]*import\(\)\s*\{.+\}\s*$").unwrap());
static RE_SET_PIPEFAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^set -euo pipefail").unwrap());
/// Matches a shebang `#!/` that appears mid-line (after a non-newline char).
/// This happens when files are concatenated without trailing newlines,
/// producing lines like `}#!/usr/bin/env bash`.
static RE_MIDLINE_SHEBANG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(.)#!/").unwrap());

/// Returns true if the line should be stripped (removed entirely).
///
/// Strips:
/// - Full-line comments (including shebangs — template provides its own)
/// - Blank/whitespace-only lines
/// - Lines matching `import ...` or `import(...)` at start
/// - `set -euo pipefail`
pub fn should_strip(line: &str) -> bool {
    RE_COMMENT.is_match(line)
        || RE_BLANK.is_match(line)
        || RE_IMPORT_CALL.is_match(line)
        || RE_IMPORT_DEF.is_match(line)
        || RE_SET_PIPEFAIL.is_match(line)
}

/// Strip lines from input, returning only lines that survive.
///
/// Pre-processes input to split mid-line shebangs caused by file
/// concatenation (e.g. `}#!/usr/bin/env bash` → `}` + `#!/usr/bin/env bash`).
pub fn strip_lines(input: &str) -> Vec<String> {
    // Split mid-line shebangs onto their own lines so they get stripped
    let normalized = RE_MIDLINE_SHEBANG.replace_all(input, "$1\n#!/");
    normalized
        .lines()
        .filter(|line| !should_strip(line))
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_shebang() {
        assert!(should_strip("#!/usr/bin/env bash"));
    }

    #[test]
    fn strips_comment() {
        assert!(should_strip("# this is a comment"));
        assert!(should_strip("  # indented comment"));
    }

    #[test]
    fn strips_blank() {
        assert!(should_strip(""));
        assert!(should_strip("   "));
        assert!(should_strip("\t\t"));
    }

    #[test]
    fn strips_import_call() {
        assert!(should_strip("import string"));
        assert!(should_strip("import fmt"));
        assert!(should_strip("  import is"));
    }

    #[test]
    fn strips_import_function() {
        assert!(should_strip("import() { something; }"));
    }

    #[test]
    fn keeps_import_in_array() {
        // Array element like `    import import::clear)` must not be stripped
        assert!(!should_strip("    import import::clear)"));
    }

    #[test]
    fn keeps_import_dynamic() {
        // Dynamic import like `import "${_lib}"` — kept (harmless at runtime)
        assert!(!should_strip(r#"    import "${_lib}""#));
    }

    #[test]
    fn strips_set_pipefail() {
        assert!(should_strip("set -euo pipefail"));
    }

    #[test]
    fn keeps_code() {
        assert!(!should_strip("echo hello"));
        assert!(!should_strip("local x=1"));
    }

    #[test]
    fn splits_midline_shebang() {
        let input = "}#!/usr/bin/env bash\necho hello";
        let result = strip_lines(input);
        assert_eq!(result, vec!["}", "echo hello"]);
    }

    #[test]
    fn handles_normal_concatenation() {
        let input = "echo a\n#!/usr/bin/env bash\necho b";
        let result = strip_lines(input);
        assert_eq!(result, vec!["echo a", "echo b"]);
    }
}
