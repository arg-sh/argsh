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
    // Prefer env vars (set by Docker build args or CI) over git
    let version = std::env::var("ARGSH_SO_VERSION").ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| git_output(&["describe", "--tags", "--dirty", "--always"]));
    let commit = std::env::var("ARGSH_SO_COMMIT").ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| git_output(&["rev-parse", "--short", "HEAD"]));

    println!("cargo:rustc-env=ARGSH_SO_VERSION={}", version);
    println!("cargo:rustc-env=ARGSH_SO_COMMIT={}", commit);

    // Rerun when git state changes (local builds)
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/");
    println!("cargo:rerun-if-changed=../.git/packed-refs");
    // Rerun when env vars change (Docker/CI builds)
    println!("cargo:rerun-if-env-changed=ARGSH_SO_VERSION");
    println!("cargo:rerun-if-env-changed=ARGSH_SO_COMMIT");
}
