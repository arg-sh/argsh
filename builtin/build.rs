use std::process::Command;

fn git_output(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() {
    let version = git_output(&["describe", "--tags", "--dirty", "--always"]);
    let commit = git_output(&["rev-parse", "--short", "HEAD"]);

    println!("cargo:rustc-env=ARGSH_SO_VERSION={}", version);
    println!("cargo:rustc-env=ARGSH_SO_COMMIT={}", commit);

    // Rerun when git state changes (branch switch, new commit, tag)
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/");
}
