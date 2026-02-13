use predicates::prelude::*;
use std::io::Write;
use std::process::Command;
use tempfile::{NamedTempFile, TempDir};

fn cmd() -> assert_cmd::Command {
    assert_cmd::Command::from(Command::new(env!("CARGO_BIN_EXE_shdoc")))
}

fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
}

// -- stdin mode (backward-compatible) --

#[test]
fn stdin_mode_produces_markdown() {
    let input = std::fs::read_to_string(fixture_path("bash.sh")).unwrap();
    let expected = std::fs::read_to_string(fixture_path("bash.expected.md")).unwrap();

    let assert = cmd().write_stdin(input).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn stdin_mode_docker() {
    let input = std::fs::read_to_string(fixture_path("docker.sh")).unwrap();
    let expected = std::fs::read_to_string(fixture_path("docker.expected.md")).unwrap();

    let assert = cmd().write_stdin(input).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn stdin_mode_complex_library() {
    let input = std::fs::read_to_string(fixture_path("string.sh")).unwrap();
    let expected = std::fs::read_to_string(fixture_path("string.expected.md")).unwrap();

    let assert = cmd().write_stdin(input).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn stdin_mode_to_library() {
    let input = std::fs::read_to_string(fixture_path("to.sh")).unwrap();
    let expected = std::fs::read_to_string(fixture_path("to.expected.md")).unwrap();

    let assert = cmd().write_stdin(input).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(output, expected);
}

// -- file mode --

#[test]
fn file_mode_creates_output() {
    let dir = TempDir::new().unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .arg(fixture_path("bash.sh"))
        .assert()
        .success();

    let output = std::fs::read_to_string(dir.path().join("bash.mdx")).unwrap();
    let expected = std::fs::read_to_string(fixture_path("bash.expected.md")).unwrap();
    assert_eq!(output, expected);
}

#[test]
fn file_mode_multiple_files() {
    let dir = TempDir::new().unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .arg(fixture_path("bash.sh"))
        .arg(fixture_path("docker.sh"))
        .assert()
        .success();

    assert!(dir.path().join("bash.mdx").exists());
    assert!(dir.path().join("docker.mdx").exists());
}

#[test]
fn file_mode_requires_output() {
    cmd()
        .arg(fixture_path("bash.sh"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("--output is required"));
}

// -- prefix template --

#[test]
fn file_mode_with_prefix() {
    let dir = TempDir::new().unwrap();
    let mut prefix_file = NamedTempFile::new().unwrap();
    prefix_file
        .write_all(b"import Link from \"@docusaurus/Link\";\n\n<Link to=\"https://example.com/${name}\">\nSource\n</Link>\n")
        .unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .args(["-p", prefix_file.path().to_str().unwrap()])
        .arg(fixture_path("bash.sh"))
        .assert()
        .success();

    let output = std::fs::read_to_string(dir.path().join("bash.mdx")).unwrap();
    // Prefix should be at the top with ${name} substituted
    assert!(
        output.starts_with("import Link"),
        "Should start with prefix, got: {}",
        &output[..80.min(output.len())]
    );
    assert!(
        output.contains("https://example.com/bash"),
        "${{name}} should be substituted"
    );
}

// -- frontmatter --

#[test]
fn file_mode_tags_frontmatter() {
    let dir = TempDir::new().unwrap();
    let mut input = NamedTempFile::with_suffix(".sh").unwrap();
    input
        .write_all(b"# @file test\n# @tags core, builtin\n# @description A test lib\n# @description Func docs\nfoo() { true; }\n")
        .unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .arg(input.path().to_str().unwrap())
        .assert()
        .success();

    // Find the output file
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "Should create output file");

    let output = std::fs::read_to_string(entries[0].path()).unwrap();
    assert!(
        output.starts_with("---\ntags: [core, builtin]\n---\n"),
        "Should have YAML frontmatter, got: {}",
        &output[..80.min(output.len())]
    );
}

#[test]
fn file_mode_no_frontmatter_flag() {
    let dir = TempDir::new().unwrap();
    let mut input = NamedTempFile::with_suffix(".sh").unwrap();
    input
        .write_all(b"# @file test\n# @tags core\n# @description Func docs\nfoo() { true; }\n")
        .unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .arg("--no-frontmatter")
        .arg(input.path().to_str().unwrap())
        .assert()
        .success();

    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let output = std::fs::read_to_string(entries[0].path()).unwrap();
    assert!(
        !output.contains("---\ntags:"),
        "Should NOT have frontmatter with --no-frontmatter"
    );
}

// -- output formats --

#[test]
fn file_mode_html_format() {
    let dir = TempDir::new().unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .args(["-f", "html"])
        .arg(fixture_path("bash.sh"))
        .assert()
        .success();

    let output_path = dir.path().join("bash.html");
    assert!(output_path.exists(), "Should create .html file");
    let output = std::fs::read_to_string(output_path).unwrap();
    assert!(output.contains("<!DOCTYPE html>"));
    assert!(output.contains("bash::version"));
}

#[test]
fn file_mode_json_format() {
    let dir = TempDir::new().unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .args(["-f", "json"])
        .arg(fixture_path("bash.sh"))
        .assert()
        .success();

    let output_path = dir.path().join("bash.json");
    assert!(output_path.exists(), "Should create .json file");
    let output = std::fs::read_to_string(output_path).unwrap();
    assert!(output.contains("\"functions\""));
    assert!(output.contains("bash::version"));
}

#[test]
fn invalid_format_fails() {
    let dir = TempDir::new().unwrap();

    cmd()
        .args(["-o", dir.path().to_str().unwrap()])
        .args(["-f", "xml"])
        .arg(fixture_path("bash.sh"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown format"));
}

// -- stdin with different formats --

#[test]
fn stdin_html_format() {
    let input = "# @file test\n# @description A func\nfoo() { true; }\n";

    let assert = cmd()
        .args(["-f", "html"])
        .write_stdin(input)
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("<!DOCTYPE html>"));
}

#[test]
fn stdin_json_format() {
    let input = "# @file test\n# @description A func\nfoo() { true; }\n";

    let assert = cmd()
        .args(["-f", "json"])
        .write_stdin(input)
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("\"functions\""));
}
