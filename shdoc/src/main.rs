//! shdoc — generate documentation from annotated bash (and Rust) source files.
//!
//! Replacement for the gawk-based shdoc. Supports two modes:
//!
//! - **stdin mode** (backward-compatible): `shdoc < file.sh`
//! - **file mode**: `shdoc -o docs/libraries -p _prefix.mdx libraries/*.sh builtin/src/*.rs`

mod model;
mod parser;
mod render;
mod toc;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "shdoc",
    about = "Generate documentation from annotated bash scripts and Rust source files"
)]
struct Cli {
    /// Input files (glob patterns supported). If omitted, reads from stdin.
    files: Vec<String>,

    /// Output directory (required when files are given)
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,

    /// Prefix template file or directory containing _prefix.mdx.
    /// Supports ${name} substitution.
    #[arg(short = 'p', long)]
    prefix: Option<String>,

    /// Output format: markdown (default), html, json
    #[arg(short = 'f', long, default_value = "markdown")]
    format: String,

    /// Disable YAML frontmatter from @tags
    #[arg(long)]
    no_frontmatter: bool,

    /// Include @internal functions in output
    #[arg(long)]
    show_internal: bool,

    /// Filter functions by tag. Prefix with ! to exclude.
    /// Can be specified multiple times. E.g. --filter '!internal'
    #[arg(long)]
    filter: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.files.is_empty() {
        // stdin mode — backward-compatible with gawk shdoc
        return stdin_mode(&cli);
    }

    file_mode(&cli)
}

/// stdin mode: read from stdin, parse as bash, write markdown to stdout.
fn stdin_mode(cli: &Cli) -> Result<()> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("failed to read stdin")?;

    let mut doc = parser::bash::parse(&input);
    filter_functions(&mut doc, cli.show_internal, &cli.filter);
    let renderer = render::create_renderer(&cli.format)?;
    print!("{}", renderer.render(&doc));
    Ok(())
}

/// file mode: process multiple files, optionally merge, write to output directory.
fn file_mode(cli: &Cli) -> Result<()> {
    let output_dir = cli
        .output
        .as_deref()
        .context("--output is required when files are given")?;

    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output directory: {}", output_dir.display()))?;

    // Resolve prefix template
    let prefix_template = resolve_prefix(cli.prefix.as_deref(), output_dir)?;

    // Expand globs and read all input files
    let input_files = expand_globs(&cli.files)?;

    // Parse all files and tag with implementation source
    let mut parsed: Vec<(String, model::Document)> = Vec::new();
    for path in &input_files {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        match parser::parse_file(path, &content) {
            Ok(mut doc) => {
                let source_file = path.to_string_lossy().to_string();
                let lang = match path.extension().and_then(|e| e.to_str()) {
                    Some("rs") => model::ImplLang::Rust,
                    _ => model::ImplLang::Bash,
                };
                // Tag each function with its implementation source
                for func in &mut doc.functions {
                    if func.implementations.is_empty() {
                        func.implementations.push(model::Implementation {
                            lang: lang.clone(),
                            source_file: source_file.clone(),
                        });
                    }
                }
                parsed.push((source_file, doc));
            }
            Err(e) => {
                eprintln!("warning: skipping {}: {}", path.display(), e);
            }
        }
    }

    // Merge documents that share a module name (e.g., to.sh + to.rs)
    let merged = parser::merge::merge(parsed);

    let renderer = render::create_renderer(&cli.format)?;
    let ext = renderer.file_extension();

    for (source, mut doc) in merged {
        filter_functions(&mut doc, cli.show_internal, &cli.filter);
        // Skip files with no documented functions (e.g., lib.rs, shared.rs)
        if doc.functions.is_empty() {
            continue;
        }

        let name = derive_output_name(&source);
        let out_path = output_dir.join(format!("{}.{}", name, ext));

        let mut output = String::new();

        // Frontmatter from @tags
        if !cli.no_frontmatter {
            if let Some(ref tags) = doc.file.tags {
                output.push_str(&format!("---\ntags: [{}]\n---\n\n", tags));
            }
        }

        // Prefix with ${name} substitution
        if let Some(ref tpl) = prefix_template {
            output.push_str(&tpl.replace("${name}", &name));
            output.push('\n');
        }

        // Rendered documentation
        output.push_str(&renderer.render(&doc));

        fs::write(&out_path, &output)
            .with_context(|| format!("failed to write {}", out_path.display()))?;
    }

    Ok(())
}

/// Resolve the prefix template from the -p flag, following the same logic
/// as the bash wrapper in docker-entrypoint.sh.
fn resolve_prefix(prefix_arg: Option<&str>, output_dir: &Path) -> Result<Option<String>> {
    match prefix_arg {
        Some(p) => {
            let path = Path::new(p);
            if path.is_file() {
                Ok(Some(fs::read_to_string(path).with_context(|| {
                    format!("failed to read prefix file: {}", path.display())
                })?))
            } else if path.is_dir() {
                let candidate = path.join("_prefix.mdx");
                if candidate.is_file() {
                    Ok(Some(fs::read_to_string(&candidate)?))
                } else {
                    Ok(None)
                }
            } else {
                // Try _prefix.mdx in the same directory
                let candidate = Path::new(p).with_file_name("_prefix.mdx");
                if candidate.is_file() {
                    Ok(Some(fs::read_to_string(&candidate)?))
                } else {
                    anyhow::bail!("prefix not found: {}", p);
                }
            }
        }
        None => {
            // Fallback: check output directory for _prefix.mdx
            let candidate = output_dir.join("_prefix.mdx");
            if candidate.is_file() {
                Ok(Some(fs::read_to_string(&candidate)?))
            } else {
                Ok(None)
            }
        }
    }
}

/// File extensions recognized as source files.
const SUPPORTED_EXTENSIONS: &[&str] = &["sh", "bash", "bats", "rs"];

/// Expand glob patterns into a list of real file paths.
/// Also handles bare directory paths by scanning for supported file types.
fn expand_globs(patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for pattern in patterns {
        let path = Path::new(pattern);
        if path.is_file() {
            files.push(path.to_path_buf());
            continue;
        }
        // If it's a directory, scan for supported extensions (non-recursive)
        if path.is_dir() {
            let entries = fs::read_dir(path)
                .with_context(|| format!("failed to read directory: {}", path.display()))?;
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if SUPPORTED_EXTENSIONS.contains(&ext) {
                            files.push(p);
                        }
                    }
                }
            }
            continue;
        }
        // Try as glob
        let matches: Vec<_> = glob::glob(pattern)
            .with_context(|| format!("invalid glob pattern: {}", pattern))?
            .filter_map(|r| r.ok())
            .filter(|p| p.is_file())
            .collect();
        if matches.is_empty() {
            eprintln!("warning: no files matched: {}", pattern);
        }
        files.extend(matches);
    }
    // Sort for deterministic output
    files.sort();
    files.dedup();
    Ok(files)
}

/// Derive the output file name (without extension) from a source path.
/// "libraries/to.sh" → "to", "builtin/src/to.rs" → "to"
fn derive_output_name(source: &str) -> String {
    let filename = source.rsplit('/').next().unwrap_or(source);
    filename
        .strip_suffix(".sh")
        .or_else(|| filename.strip_suffix(".bash"))
        .or_else(|| filename.strip_suffix(".bats"))
        .or_else(|| filename.strip_suffix(".rs"))
        .unwrap_or(filename)
        .to_string()
}

/// Filter functions based on --show-internal and --filter flags.
///
/// By default, @internal functions are excluded. Use --show-internal to include them.
/// --filter supports inclusion (e.g. "core") and exclusion (e.g. "!deprecated") by tag.
/// The special tag "internal" maps to the @internal annotation.
fn filter_functions(doc: &mut model::Document, show_internal: bool, filters: &[String]) {
    doc.functions.retain(|func| {
        // Internal filter (default: exclude)
        if func.is_internal && !show_internal {
            // Unless explicitly included via --filter internal
            if !filters.iter().any(|f| f == "internal") {
                return false;
            }
        }

        // Tag-based filters
        for filter in filters {
            if filter == "internal" {
                // Handled above (inclusion)
                continue;
            }
            if let Some(excluded) = filter.strip_prefix('!') {
                if excluded == "internal" {
                    if func.is_internal {
                        return false;
                    }
                } else if func.tags.iter().any(|t| t == excluded) {
                    return false;
                }
            } else {
                // Inclusion filter: function must have this tag
                let matches = func.tags.iter().any(|t| t == filter.as_str())
                    || (filter == "internal" && func.is_internal);
                if !matches {
                    return false;
                }
            }
        }

        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_name_from_sh() {
        assert_eq!(derive_output_name("libraries/to.sh"), "to");
        assert_eq!(derive_output_name("to.sh"), "to");
    }

    #[test]
    fn output_name_from_rs() {
        assert_eq!(derive_output_name("builtin/src/to.rs"), "to");
    }

    #[test]
    fn output_name_no_extension() {
        assert_eq!(derive_output_name("Makefile"), "Makefile");
    }
}
