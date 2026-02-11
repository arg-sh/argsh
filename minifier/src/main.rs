//! Bash script minifier with optional variable obfuscation and source bundling.
//!
//! Processes bash scripts through up to 5 phases:
//!
//! 1. **Bundle** (optional) — resolve and inline `import`/`source`/`.` statements
//! 2. **Strip** — remove comments, blanks, imports, shebangs, `set -euo pipefail`
//! 3. **Flatten** — remove indentation, trailing semicolons, end-of-line comments
//! 4. **Obfuscate** (optional) — rename local variables to short generated names
//! 5. **Join** — aggressively join newlines into minimal single-line output
//!
//! General-purpose tool — not specific to any framework.

mod bundle;
mod discover;
mod flatten;
mod join;
mod obfuscate;
mod quote;
mod strip;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "minifier", about = "Bash script minifier with optional obfuscation")]
struct Cli {
    /// Input file
    #[arg(short = 'i')]
    input: String,

    /// Output file
    #[arg(short = 'o')]
    output: String,

    /// Enable source bundling (resolve and inline imports)
    #[arg(short = 'B', long = "bundle")]
    bundle: bool,

    /// Search directory for resolving imports (repeatable). Requires -B.
    #[arg(short = 'S', long = "search-path")]
    search_paths: Vec<PathBuf>,

    /// Enable variable name obfuscation
    #[arg(short = 'O', long = "obfuscate")]
    obfuscate: bool,

    /// Exclude variables matching these patterns from obfuscation (repeatable). Requires -O.
    #[arg(short = 'V')]
    exclude_vars: Vec<String>,

    /// Ignore variables matching regex patterns (comma-separated, default: "usage,args"). Requires -O.
    #[arg(short = 'I', default_value = "usage,args")]
    ignore_vars: String,
}

/// Pipeline configuration for [`minify`].
struct MinifyConfig<'a> {
    do_bundle: bool,
    input_path: Option<&'a std::path::Path>,
    search_paths: &'a [PathBuf],
    do_obfuscate: bool,
    var_prefix: &'a str,
    ignore_vars: &'a str,
}

/// Core minification pipeline — extracted for testability.
fn minify(source: &str, config: &MinifyConfig) -> Result<String> {
    // Phase 1: Bundle (optional)
    let source = if config.do_bundle {
        let input_path = config
            .input_path
            .unwrap_or_else(|| std::path::Path::new(".")); // coverage:off - input_path always Some when do_bundle is true
        let bundle_config = bundle::BundleConfig {
            search_paths: config.search_paths.to_vec(),
        };
        bundle::bundle(source, input_path, &bundle_config)?
    } else {
        source.to_string()
    };

    // Phase 2-5: Strip → Flatten → Obfuscate → Join
    let lines = strip::strip_lines(&source);
    let lines = flatten::flatten_lines(&lines);
    let lines = if config.do_obfuscate {
        let ignore_patterns = discover::parse_ignore_patterns(config.ignore_vars)?;
        let sorted_vars = discover::discover_variables(&source, &ignore_patterns);
        let rename_map = obfuscate::build_rename_map(&sorted_vars, config.var_prefix);
        let obfuscator = obfuscate::Obfuscator::new(&sorted_vars, &rename_map);
        obfuscator.obfuscate_lines(&lines)
    } else {
        lines
    };
    let combined = lines.join("\n");
    Ok(join::join_newlines(&combined))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let source = fs::read_to_string(&cli.input)
        .with_context(|| format!("Failed to read {}", cli.input))?;

    let input_path = PathBuf::from(&cli.input);
    // Merge -V patterns into the -I ignore list
    let ignore_vars = if cli.exclude_vars.is_empty() {
        cli.ignore_vars.clone()
    } else {
        let mut parts = vec![cli.ignore_vars.clone()];
        parts.extend(cli.exclude_vars);
        parts.join(",")
    };
    let config = MinifyConfig {
        do_bundle: cli.bundle,
        input_path: Some(&input_path),
        search_paths: &cli.search_paths,
        do_obfuscate: cli.obfuscate,
        var_prefix: "a",
        ignore_vars: &ignore_vars,
    };
    let result = minify(&source, &config)?;

    fs::write(&cli.output, &result)
        .with_context(|| format!("Failed to write {}", cli.output))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg<'a>(do_obfuscate: bool, var_prefix: &'a str, ignore_vars: &'a str) -> MinifyConfig<'a> {
        MinifyConfig {
            do_bundle: false,
            input_path: None,
            search_paths: &[],
            do_obfuscate,
            var_prefix,
            ignore_vars,
        }
    }

    #[test]
    fn pipeline_minify_only() {
        let input = "#!/usr/bin/env bash\n# comment\nset -euo pipefail\n\necho hello\n  echo world\n";
        let result = minify(input, &cfg(false, "a", "usage,args")).unwrap();
        assert!(result.contains("echo hello"));
        assert!(result.contains("echo world"));
        assert!(!result.contains("#!/"));
        assert!(!result.contains("# comment"));
    }

    #[test]
    fn pipeline_obfuscate() {
        let input = "local foo=1\necho $foo\n";
        let result = minify(input, &cfg(true, "a", "usage,args")).unwrap();
        assert!(!result.contains("foo"), "Got: {result}");
        assert!(result.contains("a0"), "Got: {result}");
    }

    #[test]
    fn pipeline_obfuscate_custom_prefix() {
        let input = "local bar=1\necho $bar\n";
        let result = minify(input, &cfg(true, "x", "usage,args")).unwrap();
        assert!(result.contains("x0"), "Got: {result}");
    }

    #[test]
    fn pipeline_ignore_vars() {
        let input = "local foo=1\nlocal bar=2\necho $foo $bar\n";
        let result = minify(input, &cfg(true, "a", "foo")).unwrap();
        assert!(result.contains("foo"), "foo should be kept, got: {result}");
    }

    #[test]
    fn pipeline_underscore_prefix_not_corrupted() {
        // End-to-end: `_path` must not be partially renamed when `path` is a discovered variable
        let input = "local path=\"${1}\"\nlocal _force=0 _path=\"\"\n_path=\"${1}\"\necho \"${_path}\"\necho \"${path}\"\n";
        let result = minify(input, &cfg(true, "a", "usage,args")).unwrap();
        // `_path` must appear unchanged; `path` must be renamed to `a0`
        assert!(
            !result.contains("_a0"),
            "_path was partially renamed: {result}"
        );
        assert!(
            result.contains("${_path}"),
            "${{_path}} must stay intact: {result}"
        );
    }

    #[test]
    fn pipeline_long_underscore_prefix_not_corrupted() {
        // End-to-end: `_argsh_builtin_path` must not be renamed when `path` is discovered.
        // This mirrors the real argsh code where `path` (from binary.sh) is discovered,
        // and `_argsh_builtin_path` (from main.sh) must stay intact.
        let input = "local path=\"${1}\"\nlocal _argsh_builtin_path=\"\"\n_argsh_builtin_path=\"${1}\"\necho \"${_argsh_builtin_path}\"\necho \"${path}\"\n";
        let result = minify(input, &cfg(true, "a", "usage,args")).unwrap();
        assert!(
            result.contains("_argsh_builtin_path"),
            "_argsh_builtin_path was corrupted: {result}"
        );
        assert!(
            !result.contains("_argsh_builtin_a0"),
            "_argsh_builtin_path partially renamed: {result}"
        );
    }

    #[test]
    fn pipeline_midline_array_write() {
        // End-to-end: `prev[j]=` mid-line must be renamed (shellcheck lint regression)
        let input = "local -a prev\nlocal j=0\nfor (( j=0; j <= n; j++ )); do prev[j]=\"${j}\"; done\necho \"${prev[@]}\"\n";
        let result = minify(input, &cfg(true, "a", "usage,args")).unwrap();
        assert!(
            !regex::Regex::new(r"\bprev\b").unwrap().is_match(&result),
            "prev not fully renamed (mid-line array write missed): {result}"
        );
    }

    #[test]
    fn pipeline_substring_offset_renamed() {
        // End-to-end: bare variable in `${var:i-1:1}` must be renamed
        let input = "local i=0\nlocal str=\"hello\"\necho \"${str:i-1:1}\"\necho \"${str:0:i}\"\n";
        let result = minify(input, &cfg(true, "a", "usage,args")).unwrap();
        // `i` must not appear as bare variable in substring offsets
        assert!(
            !regex::Regex::new(r"\$\{[^}]+:[^}]*\bi\b").unwrap().is_match(&result),
            "i not renamed in substring offset: {result}"
        );
    }

    #[test]
    fn pipeline_bare_array_subscript_renamed() {
        // End-to-end: bare array subscript `prev[j]=` and `(( prev[j] ))` must rename `j`
        let input = "local -a prev\nlocal j=0\nfor (( j=0; j <= n; j++ )); do prev[j]=\"${j}\"; done\necho $(( prev[j-1] + 1 ))\n";
        let result = minify(input, &cfg(true, "a", "usage,args")).unwrap();
        // `j` must not appear as bare subscript in array brackets
        assert!(
            !regex::Regex::new(r"\w\[j[\]\-+]").unwrap().is_match(&result),
            "j not renamed in bare array subscript: {result}"
        );
    }

    #[test]
    fn pipeline_read_ra_flag_not_corrupted() {
        // End-to-end: `read -ra` combined flag must not be corrupted by obfuscation
        let input = "local a flags\nIFS='|' read -ra flags <<< \"$a\"\n";
        let result = minify(input, &cfg(true, "a", "usage,args")).unwrap();
        // The `-ra` flag must remain intact — no `read -ra<digit>` corruption
        assert!(
            !regex::Regex::new(r"read -ra\d").unwrap().is_match(&result),
            "read -ra flag corrupted: {result}"
        );
    }
}
