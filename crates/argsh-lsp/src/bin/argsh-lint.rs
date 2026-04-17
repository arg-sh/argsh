//! argsh-lint — standalone CLI linter for argsh scripts.
//!
//! Reuses the same analysis pipeline as the LSP server (`argsh-lsp`), but
//! emits shellcheck-style diagnostics for CI, editor-agnostic pipelines, and
//! the `argsh lint` subcommand. No LSP protocol involved.
//!
//! Flag syntax mirrors shellcheck where it makes sense (same long names, same
//! short letters) so users can transfer muscle memory.
//!
//! Exit codes:
//!   0  no diagnostics above the severity threshold
//!   1  at least one diagnostic emitted
//!   2  CLI usage error (unknown flag, file not found, read error, ...)

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use argsh_lsp::{diagnostics, resolver};
use argsh_syntax::document::analyze;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

const USAGE: &str = "\
argsh-lint — static analysis for argsh scripts

USAGE:
    argsh-lint [OPTIONS] [FILE...]

OPTIONS (shellcheck-compatible where applicable):
    -h,     --help              Print this help
    -V,     --version           Print version
    -f FMT, --format=FMT        Output format: gcc (default), tty, json, checkstyle, quiet
    -e LIST,--exclude=LIST      Comma-separated codes to skip (e.g. AG004,AG007)
    -i LIST,--include=LIST      Comma-separated codes to enable exclusively
    -S SEV, --severity=SEV      Minimum severity: error, warning, info, style
    -C WHEN,--color=WHEN        Colorize tty output: auto (default), always, never
            --no-resolve        Skip cross-file import resolution (faster, skips AG013)

Reads files listed on the command line. With no files, reads from stdin
(filename shown as \"<stdin>\" in output).

SUPPRESSION:
    In-source comments work like shellcheck's `# shellcheck disable=`:
      # argsh disable=AG001,AG004      — next line only
      # argsh disable-file=AG007       — entire file
      # argsh disable-file              — all codes, entire file

EXAMPLES:
    argsh-lint script.sh
    argsh-lint --exclude=AG013 --format=json *.sh
    argsh-lint --severity=error ci/*.sh
    cat script.sh | argsh-lint --format=tty";

#[derive(Copy, Clone, PartialEq, Eq)]
enum Format {
    Gcc,
    Tty,
    Json,
    Checkstyle,
    Quiet,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Color {
    Auto,
    Always,
    Never,
}

struct Cli {
    files: Vec<PathBuf>,
    resolve: bool,
    format: Format,
    color: Color,
    exclude: Vec<String>,
    include: Vec<String>,
    min_severity: DiagnosticSeverity,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            resolve: true,
            format: Format::Gcc,
            color: Color::Auto,
            exclude: Vec::new(),
            include: Vec::new(),
            // DiagnosticSeverity::HINT (4) is the weakest; keep everything by default.
            min_severity: DiagnosticSeverity::HINT,
        }
    }
}

fn parse_format(v: &str) -> Result<Format, String> {
    match v {
        "gcc" => Ok(Format::Gcc),
        "tty" => Ok(Format::Tty),
        "json" | "json1" => Ok(Format::Json),
        "checkstyle" => Ok(Format::Checkstyle),
        "quiet" => Ok(Format::Quiet),
        other => Err(format!(
            "unknown format: {} (valid: gcc, tty, json, checkstyle, quiet)",
            other
        )),
    }
}

fn parse_color(v: &str) -> Result<Color, String> {
    match v {
        "auto" => Ok(Color::Auto),
        "always" | "yes" => Ok(Color::Always),
        "never" | "no" => Ok(Color::Never),
        other => Err(format!(
            "unknown color setting: {} (valid: auto, always, never)",
            other
        )),
    }
}

fn parse_severity(v: &str) -> Result<DiagnosticSeverity, String> {
    match v {
        "error" => Ok(DiagnosticSeverity::ERROR),
        "warning" => Ok(DiagnosticSeverity::WARNING),
        "info" | "information" => Ok(DiagnosticSeverity::INFORMATION),
        "style" | "hint" => Ok(DiagnosticSeverity::HINT),
        other => Err(format!(
            "unknown severity: {} (valid: error, warning, info, style)",
            other
        )),
    }
}

fn parse_csv(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Take the value after a `-X <value>` short flag or return the inline value for
/// `--long=value`. Produces an error with the flag name if the value is missing.
fn take_value<I: Iterator<Item = String>>(
    iter: &mut I,
    flag: &str,
    inline: Option<&str>,
) -> Result<String, String> {
    if let Some(v) = inline {
        return Ok(v.to_string());
    }
    iter.next()
        .ok_or_else(|| format!("{} requires a value", flag))
}

fn parse_args(args: Vec<String>) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut iter = args.into_iter().skip(1); // skip argv[0]

    while let Some(arg) = iter.next() {
        // Split --long=value form upfront so we can pass the inline value into take_value.
        let (key, inline) = if let Some(eq) = arg.find('=') {
            if arg.starts_with("--") {
                (arg[..eq].to_string(), Some(arg[eq + 1..].to_string()))
            } else {
                (arg.clone(), None)
            }
        } else {
            (arg.clone(), None)
        };
        let inline_ref = inline.as_deref();

        match key.as_str() {
            "-h" | "--help" => {
                println!("{}", USAGE);
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("argsh-lint {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--no-resolve" => cli.resolve = false,
            "-f" | "--format" => {
                let v = take_value(&mut iter, &key, inline_ref)?;
                cli.format = parse_format(&v)?;
            }
            "-C" | "--color" => {
                // shellcheck allows `-C` without argument (= auto); support same.
                if key == "-C" && inline_ref.is_none() {
                    // Peek next: if it looks like a value, consume; else default to auto.
                    cli.color = Color::Auto;
                } else {
                    let v = take_value(&mut iter, &key, inline_ref)?;
                    cli.color = parse_color(&v)?;
                }
            }
            "-e" | "--exclude" => {
                let v = take_value(&mut iter, &key, inline_ref)?;
                cli.exclude.extend(parse_csv(&v));
            }
            "-i" | "--include" => {
                let v = take_value(&mut iter, &key, inline_ref)?;
                cli.include.extend(parse_csv(&v));
            }
            "-S" | "--severity" => {
                let v = take_value(&mut iter, &key, inline_ref)?;
                cli.min_severity = parse_severity(&v)?;
            }
            "--" => {
                // Remaining args are files (allows filenames that start with `-`).
                for f in iter.by_ref() {
                    cli.files.push(PathBuf::from(f));
                }
            }
            f if f.starts_with('-') && f != "-" => {
                return Err(format!("unknown flag: {}", f));
            }
            f => cli.files.push(PathBuf::from(f)),
        }
    }
    Ok(cli)
}

fn severity_rank(sev: Option<DiagnosticSeverity>) -> u8 {
    // LSP numbers (1=error, 4=hint) — we invert so higher = more severe.
    match sev {
        Some(DiagnosticSeverity::ERROR) => 4,
        Some(DiagnosticSeverity::WARNING) => 3,
        Some(DiagnosticSeverity::INFORMATION) => 2,
        Some(DiagnosticSeverity::HINT) => 1,
        _ => 3, // default to warning if unset
    }
}

fn severity_str(sev: Option<DiagnosticSeverity>) -> &'static str {
    match sev {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        Some(DiagnosticSeverity::HINT) => "style",
        _ => "warning",
    }
}

fn severity_checkstyle(sev: Option<DiagnosticSeverity>) -> &'static str {
    match sev {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        Some(DiagnosticSeverity::HINT) => "info",
        _ => "warning",
    }
}

fn code_str(diag: &Diagnostic) -> &str {
    match diag.code.as_ref() {
        Some(NumberOrString::String(s)) => s.as_str(),
        _ => "",
    }
}

/// ANSI color code for a severity — only used by `tty` format when color is enabled.
fn severity_ansi(sev: Option<DiagnosticSeverity>) -> &'static str {
    match sev {
        Some(DiagnosticSeverity::ERROR) => "\x1b[31m",       // red
        Some(DiagnosticSeverity::WARNING) => "\x1b[33m",     // yellow
        Some(DiagnosticSeverity::INFORMATION) => "\x1b[36m", // cyan
        Some(DiagnosticSeverity::HINT) => "\x1b[35m",        // magenta
        _ => "\x1b[33m",
    }
}

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";

fn should_use_color(color: Color) -> bool {
    match color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => std::io::stdout().is_terminal(),
    }
}

/// `file:line:col: severity: message` — gcc / shellcheck-compatible one-liner.
fn format_gcc(filename: &str, diag: &Diagnostic) -> String {
    let line = diag.range.start.line + 1;
    let col = diag.range.start.character + 1;
    let sev = severity_str(diag.severity);
    format!("{}:{}:{}: {}: {}", filename, line, col, sev, diag.message)
}

/// Colorized tty format — like shellcheck's default tty output.
fn format_tty(filename: &str, diag: &Diagnostic, use_color: bool) -> String {
    let line = diag.range.start.line + 1;
    let col = diag.range.start.character + 1;
    let sev = severity_str(diag.severity);
    if use_color {
        format!(
            "{}{}{}:{}:{}: {}{}{}{}: {}",
            ANSI_BOLD,
            filename,
            ANSI_RESET,
            line,
            col,
            severity_ansi(diag.severity),
            sev,
            ANSI_RESET,
            "",
            diag.message,
        )
    } else {
        format_gcc(filename, diag)
    }
}

fn format_json(filename: &str, diag: &Diagnostic) -> String {
    let line = diag.range.start.line + 1;
    let col_start = diag.range.start.character + 1;
    let line_end = diag.range.end.line + 1;
    let col_end = diag.range.end.character + 1;
    let sev = severity_str(diag.severity);
    let code = code_str(diag);
    format!(
        "{{\"file\":{},\"line\":{},\"column\":{},\"endLine\":{},\"endColumn\":{},\"severity\":{},\"code\":{},\"message\":{}}}",
        json_str(filename),
        line,
        col_start,
        line_end,
        col_end,
        json_str(sev),
        json_str(code),
        json_str(&diag.message),
    )
}

/// Checkstyle XML entry. Caller is responsible for the wrapping `<checkstyle>` /
/// per-file `<file>` elements (see `print_checkstyle`).
fn format_checkstyle_error(diag: &Diagnostic) -> String {
    let line = diag.range.start.line + 1;
    let col = diag.range.start.character + 1;
    let sev = severity_checkstyle(diag.severity);
    let code = code_str(diag);
    format!(
        "    <error line=\"{}\" column=\"{}\" severity=\"{}\" message={} source=\"argsh.{}\"/>",
        line,
        col,
        sev,
        xml_attr(&diag.message),
        code,
    )
}

/// XML attribute escaper (quotes + &, <, >).
fn xml_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {
                out.push(' ');
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Minimal JSON string escaper (handles quote, backslash, control chars).
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Filter diagnostics by include/exclude lists and minimum severity.
fn filter_diag(diag: &Diagnostic, cli: &Cli) -> bool {
    let code = code_str(diag);
    if !cli.include.is_empty() && !cli.include.iter().any(|c| c == code) {
        return false;
    }
    if cli.exclude.iter().any(|c| c == code) {
        return false;
    }
    if severity_rank(diag.severity) < severity_rank(Some(cli.min_severity)) {
        return false;
    }
    true
}

fn analyze_file(path: Option<&PathBuf>, content: &str, resolve: bool) -> Vec<Diagnostic> {
    let analysis = analyze(content);
    let imports = if resolve {
        if let Some(p) = path {
            resolver::resolve_imports(&analysis, p, resolver::DEFAULT_MAX_DEPTH)
        } else {
            resolver::ResolvedImports::default()
        }
    } else {
        resolver::ResolvedImports::default()
    };
    diagnostics::generate_diagnostics(&analysis, &imports, content)
}

fn print_checkstyle(all: &[(String, Vec<Diagnostic>)]) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(out, "<checkstyle version=\"4.3\">");
    for (file, diags) in all {
        if diags.is_empty() {
            continue;
        }
        let _ = writeln!(out, "  <file name={}>", xml_attr(file));
        for d in diags {
            let _ = writeln!(out, "{}", format_checkstyle_error(d));
        }
        let _ = writeln!(out, "  </file>");
    }
    let _ = writeln!(out, "</checkstyle>");
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let cli = match parse_args(argv) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("argsh-lint: {}", msg);
            eprintln!("Run `argsh-lint --help` for usage.");
            return ExitCode::from(2);
        }
    };

    let use_color = should_use_color(cli.color);

    // Collect per-file diagnostics (needed for checkstyle which wraps
    // everything in XML; other formats stream line-by-line).
    let mut all: Vec<(String, Vec<Diagnostic>)> = Vec::new();
    let mut total_emitted = 0usize;
    let mut had_io_error = false;

    let sources: Vec<(String, Option<PathBuf>, String)> = if cli.files.is_empty() {
        // stdin mode
        let mut content = String::new();
        use std::io::Read;
        if let Err(e) = std::io::stdin().read_to_string(&mut content) {
            eprintln!("argsh-lint: reading stdin: {}", e);
            return ExitCode::from(2);
        }
        vec![("<stdin>".to_string(), None, content)]
    } else {
        let mut out = Vec::new();
        for path in &cli.files {
            match std::fs::read_to_string(path) {
                Ok(c) => out.push((path.to_string_lossy().to_string(), Some(path.clone()), c)),
                Err(e) => {
                    eprintln!("argsh-lint: {}: {}", path.display(), e);
                    had_io_error = true;
                }
            }
        }
        out
    };

    for (filename, path, content) in &sources {
        let raw = analyze_file(path.as_ref(), content, cli.resolve);
        let filtered: Vec<Diagnostic> = raw.into_iter().filter(|d| filter_diag(d, &cli)).collect();

        if cli.format != Format::Checkstyle && cli.format != Format::Quiet {
            for d in &filtered {
                let line = match cli.format {
                    Format::Gcc => format_gcc(filename, d),
                    Format::Tty => format_tty(filename, d, use_color),
                    Format::Json => format_json(filename, d),
                    Format::Checkstyle | Format::Quiet => unreachable!(),
                };
                println!("{}", line);
            }
        }

        total_emitted += filtered.len();
        all.push((filename.clone(), filtered));
    }

    if cli.format == Format::Checkstyle {
        print_checkstyle(&all);
    }

    if had_io_error {
        return ExitCode::from(2);
    }
    if total_emitted > 0 {
        return ExitCode::from(1);
    }
    ExitCode::from(0)
}
