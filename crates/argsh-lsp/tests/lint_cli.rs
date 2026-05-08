//! Integration tests for the `argsh-lint` CLI binary.
//!
//! Each test spawns the release binary as a subprocess and asserts on stdout,
//! stderr, and exit code. The binary is built by `cargo test` automatically
//! via the `env!("CARGO_BIN_EXE_argsh-lint")` environment variable.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_argsh-lint"))
}

/// Run argsh-lint with the given arguments and stdin content.
/// Returns (stdout, stderr, exit code).
fn run(args: &[&str], stdin_input: Option<&str>) -> (String, String, i32) {
    let mut cmd = Command::new(bin());
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if stdin_input.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    let mut child = cmd.spawn().expect("spawn argsh-lint");

    if let Some(input) = stdin_input {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }

    let output = child.wait_with_output().expect("wait argsh-lint");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

/// Owning handle for a temp file: keeps the `TempDir` alive (auto-cleans on
/// Drop) while exposing the path. Dereffing the handle yields the path so
/// callers can pass it to `run`, etc.
struct TmpFile {
    // Kept alive for RAII cleanup; not otherwise used.
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl TmpFile {
    fn as_str(&self) -> &str {
        self.path.to_str().unwrap()
    }
}

/// Write a temp file with the given content under a freshly-created temp dir.
/// The returned `TmpFile` owns the dir — cleanup happens when it is dropped.
fn tmpfile(name: &str, content: &str) -> TmpFile {
    let dir = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    TmpFile { _dir: dir, path }
}

// -----------------------------------------------------------------------------

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    let (stdout, _stderr, code) = run(&["--help"], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("argsh-lint"), "stdout: {}", stdout);
    assert!(stdout.contains("USAGE:"), "stdout: {}", stdout);
    assert!(stdout.contains("--format"), "stdout: {}", stdout);
}

#[test]
fn short_help_flag_also_works() {
    let (stdout, _stderr, code) = run(&["-h"], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("USAGE:"));
}

#[test]
fn version_flag_prints_version() {
    let (stdout, _stderr, code) = run(&["--version"], None);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("argsh-lint "), "stdout: {}", stdout);
}

#[test]
fn unknown_flag_prints_error_and_exits_two() {
    let (_stdout, stderr, code) = run(&["--nonexistent"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown flag"), "stderr: {}", stderr);
}

#[test]
fn clean_file_produces_no_output_and_exits_zero() {
    let path = tmpfile(
        "clean.sh",
        "#!/usr/bin/env argsh\n\
         \n\
         main() {\n\
           local flag\n\
           local -a args=(\n\
             'flag|f'  \"Flag\"\n\
           )\n\
           :args \"Test\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&[path.as_str()], None);
    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "expected no output, got: {}", stdout);
}

#[test]
fn file_with_diagnostics_exits_one_and_emits_gcc_format() {
    // 'missing|m' references a variable that isn't declared → AG004.
    let path = tmpfile(
        "bad.sh",
        "#!/usr/bin/env argsh\n\
         \n\
         main() {\n\
           local flag\n\
           local -a args=(\n\
             'flag|f'    \"Flag\"\n\
             'missing|m' \"Uses undeclared var\"\n\
           )\n\
           :args \"Test\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&[path.as_str()], None);
    assert_eq!(code, 1);
    // gcc-style: file:line:col: severity: message
    assert!(stdout.contains("bad.sh:"), "stdout: {}", stdout);
    assert!(stdout.contains(": warning:") || stdout.contains(": error:"), "stdout: {}", stdout);
    assert!(stdout.contains("AG004"), "stdout: {}", stdout);
}

#[test]
fn stdin_mode_uses_stdin_placeholder_filename() {
    let input = "#!/usr/bin/env argsh\n\
                 \n\
                 main() {\n\
                   local -a args=(\n\
                     'missing|m' \"undeclared\"\n\
                   )\n\
                   :args \"Test\" \"${@}\"\n\
                 }\n";
    let (stdout, _stderr, code) = run(&[], Some(input));
    assert_eq!(code, 1);
    assert!(stdout.starts_with("<stdin>:"), "stdout: {}", stdout);
    assert!(stdout.contains("AG004"), "stdout: {}", stdout);
}

#[test]
fn json_format_emits_valid_json_per_line() {
    let path = tmpfile(
        "bad.sh",
        "#!/usr/bin/env argsh\n\
         \n\
         main() {\n\
           local -a args=(\n\
             'missing|m' \"undeclared\"\n\
           )\n\
           :args \"Test\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&["--format", "json", path.as_str()], None);
    assert_eq!(code, 1);
    for line in stdout.lines() {
        // Each line must parse as JSON
        let parsed: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid JSON: {}: {}", e, line));
        assert!(parsed.get("file").is_some());
        assert!(parsed.get("line").is_some());
        assert!(parsed.get("column").is_some());
        assert!(parsed.get("severity").is_some());
        assert!(parsed.get("code").is_some());
        assert!(parsed.get("message").is_some());
    }
}

#[test]
fn unknown_format_prints_error_and_exits_two() {
    let (_stdout, stderr, code) = run(&["--format", "xml"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown format"), "stderr: {}", stderr);
}

#[test]
fn nonexistent_file_emits_error_and_exits_two() {
    let (_stdout, stderr, code) = run(&["/does/not/exist.sh"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("/does/not/exist.sh"), "stderr: {}", stderr);
}

#[test]
fn multiple_files_concatenate_diagnostics() {
    let bad = tmpfile(
        "bad1.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let clean = tmpfile(
        "clean.sh",
        "#!/usr/bin/env argsh\n\
         main() {\n\
           local flag\n\
           local -a args=(\n\
             'flag|f' \"ok\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &[bad.as_str(), clean.as_str()],
        None,
    );
    assert_eq!(code, 1);
    // Exactly one diagnostic, from bad1.sh
    let line_count = stdout.lines().filter(|l| !l.is_empty()).count();
    assert_eq!(line_count, 1, "stdout: {}", stdout);
    assert!(stdout.contains("bad1.sh"), "stdout: {}", stdout);
    assert!(!stdout.contains("clean.sh"), "stdout: {}", stdout);
}

#[test]
fn dash_dash_separator_allows_filename_starting_with_dash() {
    let path = tmpfile(
        "-weird-name.sh",
        "#!/usr/bin/env argsh\n\
         main() {\n\
           local flag\n\
           local -a args=(\n\
             'flag|f' \"ok\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (_stdout, _stderr, code) = run(&["--", path.as_str()], None);
    assert_eq!(code, 0);
}

#[test]
fn exclude_filters_out_specific_codes() {
    // Two diagnostics: AG004 + AG004. Excluding AG004 → zero diagnostics.
    let path = tmpfile(
        "multi.sh",
        "#!/usr/bin/env argsh\n\
         main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
             'other|o'   \"y\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["--exclude=AG004", path.as_str()],
        None,
    );
    assert_eq!(code, 0, "stdout: {}", stdout);
    assert!(!stdout.contains("AG004"), "stdout: {}", stdout);
}

#[test]
fn short_e_flag_matches_exclude_long_form() {
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["-e", "AG004", path.as_str()],
        None,
    );
    assert_eq!(code, 0, "stdout: {}", stdout);
    assert!(stdout.is_empty());
}

#[test]
fn include_restricts_to_whitelisted_codes() {
    // File has AG004; --include=AG007 means AG007 only → AG004 filtered out.
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["--include=AG007", path.as_str()],
        None,
    );
    assert_eq!(code, 0, "stdout: {}", stdout);
    assert!(stdout.is_empty());
}

#[test]
fn severity_filter_drops_below_threshold() {
    // AG004 is a warning; --severity=error drops everything below error.
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["--severity=error", path.as_str()],
        None,
    );
    assert_eq!(code, 0, "stdout: {}", stdout);
    assert!(stdout.is_empty());
}

#[test]
fn short_s_severity_flag_works() {
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["-S", "error", path.as_str()],
        None,
    );
    assert_eq!(code, 0, "stdout: {}", stdout);
    assert!(stdout.is_empty());
}

#[test]
fn severity_warning_keeps_warnings() {
    // AG004 is warning; --severity=warning keeps it.
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["--severity=warning", path.as_str()],
        None,
    );
    assert_eq!(code, 1);
    assert!(stdout.contains("AG004"));
}

#[test]
fn quiet_format_emits_nothing_but_sets_exit_code() {
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&["--format=quiet", path.as_str()], None);
    assert_eq!(code, 1, "stdout: {}", stdout);
    assert!(stdout.is_empty(), "stdout: {}", stdout);
}

#[test]
fn checkstyle_format_emits_xml_document() {
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&["--format=checkstyle", path.as_str()], None);
    assert_eq!(code, 1);
    assert!(stdout.starts_with("<?xml"), "stdout: {}", stdout);
    assert!(stdout.contains("<checkstyle"), "stdout: {}", stdout);
    assert!(stdout.contains("<file name="), "stdout: {}", stdout);
    assert!(stdout.contains("<error"), "stdout: {}", stdout);
    assert!(stdout.contains("source=\"argsh.AG004\""), "stdout: {}", stdout);
    assert!(stdout.contains("</checkstyle>"), "stdout: {}", stdout);
}

#[test]
fn color_never_suppresses_ansi_escapes_in_tty_format() {
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["--format=tty", "--color=never", path.as_str()],
        None,
    );
    assert_eq!(code, 1);
    // No ANSI escape sequences when color=never.
    assert!(!stdout.contains('\x1b'), "stdout contains ANSI escapes: {:?}", stdout);
}

#[test]
fn color_always_emits_ansi_escapes_in_tty_format() {
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(
        &["--format=tty", "--color=always", path.as_str()],
        None,
    );
    assert_eq!(code, 1);
    assert!(stdout.contains('\x1b'), "stdout: {}", stdout);
}

#[test]
fn short_f_flag_maps_to_format() {
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&["-f", "json", path.as_str()], None);
    assert_eq!(code, 1);
    // First line should parse as JSON.
    let first = stdout.lines().next().unwrap_or("");
    serde_json::from_str::<serde_json::Value>(first).expect("valid JSON");
}

#[test]
fn invalid_severity_exits_two() {
    let (_stdout, stderr, code) = run(&["--severity=bogus"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown severity"), "stderr: {}", stderr);
}

#[test]
fn invalid_color_exits_two() {
    let (_stdout, stderr, code) = run(&["--color=plaid"], None);
    assert_eq!(code, 2);
    assert!(stderr.contains("unknown color"), "stderr: {}", stderr);
}

#[test]
fn no_resolve_flag_still_runs_diagnostics() {
    // With --no-resolve, AG013 (unresolved import) is not emitted because
    // resolution is skipped, but other diagnostics still work.
    let path = tmpfile(
        "bad.sh",
        "main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&["--no-resolve", path.as_str()], None);
    assert_eq!(code, 1);
    assert!(stdout.contains("AG004"), "stdout: {}", stdout);
    // Must not contain AG013 since resolution was skipped.
    assert!(!stdout.contains("AG013"), "stdout: {}", stdout);
}

#[test]
fn suppression_comment_silences_diagnostic() {
    // `# argsh disable-file=AG004` must suppress the AG004 diagnostic
    // (regression: suppression support works end-to-end in the CLI too,
    // not just in the LSP server).
    let path = tmpfile(
        "suppressed.sh",
        "#!/usr/bin/env argsh\n\
         # argsh disable-file=AG004\n\
         main() {\n\
           local -a args=(\n\
             'missing|m' \"x\"\n\
           )\n\
           :args \"T\" \"${@}\"\n\
         }\n",
    );
    let (stdout, _stderr, code) = run(&[path.as_str()], None);
    assert_eq!(code, 0, "stdout: {}", stdout);
    assert!(stdout.is_empty());
}
