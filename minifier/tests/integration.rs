use predicates::prelude::*;
use std::io::Write;
use std::process::Command;
use tempfile::{NamedTempFile, TempDir};

fn cmd() -> assert_cmd::Command {
    assert_cmd::Command::from(Command::new(env!("CARGO_BIN_EXE_minifier")))
}

fn minify(input: &str) -> String {
    let mut infile = NamedTempFile::new().unwrap();
    infile.write_all(input.as_bytes()).unwrap();
    let outfile = NamedTempFile::new().unwrap();

    cmd()
        .args(["-i", infile.path().to_str().unwrap()])
        .args(["-o", outfile.path().to_str().unwrap()])
        .assert()
        .success();

    std::fs::read_to_string(outfile.path()).unwrap()
}

fn minify_obfuscated(input: &str) -> String {
    let mut infile = NamedTempFile::new().unwrap();
    infile.write_all(input.as_bytes()).unwrap();
    let outfile = NamedTempFile::new().unwrap();

    cmd()
        .args(["-i", infile.path().to_str().unwrap()])
        .args(["-o", outfile.path().to_str().unwrap()])
        .arg("-O")
        .assert()
        .success();

    std::fs::read_to_string(outfile.path()).unwrap()
}

#[test]
fn cli_minify_only() {
    let input = "#!/usr/bin/env bash\n# comment\nset -euo pipefail\n\necho hello\n  echo world\n";
    let result = minify(input);
    assert!(result.contains("echo hello"));
    assert!(result.contains("echo world"));
    assert!(!result.contains("#!/"));
    assert!(!result.contains("# comment"));
    assert!(!result.contains("set -euo pipefail"));
}

#[test]
fn cli_obfuscate() {
    let input = "local foo=1\necho $foo\n";
    let result = minify_obfuscated(input);
    assert!(!result.contains("foo"), "Got: {result}");
    assert!(result.contains("a0"), "Got: {result}");
}

#[test]
fn cli_custom_prefix() {
    let mut infile = NamedTempFile::new().unwrap();
    infile.write_all(b"local foo=1\necho $foo\n").unwrap();
    let outfile = NamedTempFile::new().unwrap();

    cmd()
        .args(["-i", infile.path().to_str().unwrap()])
        .args(["-o", outfile.path().to_str().unwrap()])
        .arg("-O")
        .args(["-V", "x"])
        .assert()
        .success();

    let result = std::fs::read_to_string(outfile.path()).unwrap();
    assert!(result.contains("x0"), "Got: {result}");
}

#[test]
fn cli_ignore_vars() {
    let mut infile = NamedTempFile::new().unwrap();
    infile.write_all(b"local foo=1\nlocal bar=2\necho $foo $bar\n").unwrap();
    let outfile = NamedTempFile::new().unwrap();

    cmd()
        .args(["-i", infile.path().to_str().unwrap()])
        .args(["-o", outfile.path().to_str().unwrap()])
        .arg("-O")
        .args(["-I", "foo"])
        .assert()
        .success();

    let result = std::fs::read_to_string(outfile.path()).unwrap();
    // foo should be kept (ignored from obfuscation)
    assert!(result.contains("foo"), "foo should be kept, got: {result}");
}

#[test]
fn cli_missing_input() {
    cmd()
        .args(["-i", "/tmp/nonexistent_minifier_test_xyz.sh"])
        .args(["-o", "/tmp/out.sh"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to read"));
}

#[test]
fn cli_heredoc_preserved() {
    let input = "cat <<EOF\nhello world\nEOF\necho done\n";
    let result = minify(input);
    assert!(result.contains("hello world\n"), "Got: {result}");
    assert!(result.contains("EOF"), "Got: {result}");
}

#[test]
fn cli_case_statement() {
    let input = r#"case "$1" in
  start)
    echo starting
    ;;
  stop)
    echo stopping
    ;;
esac
"#;
    let result = minify(input);
    assert!(result.contains("esac"), "Got: {result}");
    assert!(result.contains("start)"), "Got: {result}");
}

#[test]
fn cli_then_space() {
    let input = "if true; then\n  echo yes\nfi\n";
    let result = minify(input);
    assert!(!result.contains("then;"), "Got: {result}");
    assert!(result.contains("then "), "Got: {result}");
}

#[test]
fn cli_full_pipeline_syntax_check() {
    // A more complex bash script that exercises many features
    let input = r#"#!/usr/bin/env bash
# Full pipeline test
set -euo pipefail

import fmt

local name="world"
local -a items=(one two three)

greet() {
  local msg="Hello $name"
  echo "$msg"
}

for item in "${items[@]}"; do
  echo "$item"
done

if [[ -n "$name" ]]; then
  greet
else
  echo "no name"
fi

case "$1" in
  start)
    echo starting
    ;;
  *)
    echo "unknown"
    ;;
esac
"#;
    let result = minify(input);
    // Should be valid bash (no then; or do;)
    assert!(!result.contains("then;"), "Got: {result}");
    assert!(!result.contains("do;"), "Got: {result}");
}

// --- Bundle integration tests ---

#[test]
fn cli_bundle_inlines_imports() {
    let dir = TempDir::new().unwrap();
    let libs = dir.path().join("libs");
    std::fs::create_dir(&libs).unwrap();
    std::fs::write(libs.join("greet.sh"), "echo hello from greet\n").unwrap();
    let main_path = dir.path().join("main.sh");
    std::fs::write(&main_path, "import greet\necho main\n").unwrap();
    let outfile = NamedTempFile::new().unwrap();

    cmd()
        .args(["-i", main_path.to_str().unwrap()])
        .args(["-o", outfile.path().to_str().unwrap()])
        .arg("-B")
        .args(["-S", libs.to_str().unwrap()])
        .assert()
        .success();

    let result = std::fs::read_to_string(outfile.path()).unwrap();
    assert!(
        result.contains("echo hello from greet"),
        "Import should be inlined, got: {result}"
    );
    assert!(result.contains("echo main"), "Got: {result}");
}

#[test]
fn cli_bundle_with_obfuscate() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("lib.sh"), "local libvar=42\necho $libvar\n").unwrap();
    let main_path = dir.path().join("main.sh");
    std::fs::write(&main_path, "import lib\nlocal foo=1\necho $foo\n").unwrap();
    let outfile = NamedTempFile::new().unwrap();

    cmd()
        .args(["-i", main_path.to_str().unwrap()])
        .args(["-o", outfile.path().to_str().unwrap()])
        .arg("-B")
        .arg("-O")
        .assert()
        .success();

    let result = std::fs::read_to_string(outfile.path()).unwrap();
    // Both variables should be obfuscated
    assert!(!result.contains("libvar"), "Got: {result}");
    assert!(!result.contains("foo"), "Got: {result}");
}

#[test]
fn cli_no_bundle_flag_leaves_imports() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("lib.sh"), "echo lib\n").unwrap();
    let main_path = dir.path().join("main.sh");
    std::fs::write(&main_path, "import lib\necho main\n").unwrap();
    let outfile = NamedTempFile::new().unwrap();

    cmd()
        .args(["-i", main_path.to_str().unwrap()])
        .args(["-o", outfile.path().to_str().unwrap()])
        .assert()
        .success();

    let result = std::fs::read_to_string(outfile.path()).unwrap();
    // Without -B, imports are NOT inlined (strip phase removes import lines)
    assert!(!result.contains("echo lib"), "Got: {result}");
    assert!(result.contains("echo main"), "Got: {result}");
}
