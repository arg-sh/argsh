//! lib::pull builtin — pull a library from an OCI registry.
//!
//! Called by argsh::lib::add when builtins are loaded.
//! Falls back to curl/GitHub releases when builtins aren't available.

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::oci::OciClient;
use crate::shell;
use std::ffi::{c_char, c_int};

// -- Builtin registration ---------------------------------------------------

static LIB_PULL_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Pull a library from an OCI registry.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "lib::pull_struct"]
pub static mut LIB_PULL_STRUCT: BashBuiltin = BashBuiltin {
    name: c"lib::pull".as_ptr(),
    function: lib_pull_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"lib::pull <registry> <name> <tag> <dest>".as_ptr(),
    long_doc: LIB_PULL_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "lib::pull_builtin_load"]
pub extern "C" fn lib_pull_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "lib::pull_builtin_unload"]
pub extern "C" fn lib_pull_builtin_unload(_name: *const c_char) {}

extern "C" fn lib_pull_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        lib_pull_main(&args)
    })
    .unwrap_or(1)
}

// -- Implementation ---------------------------------------------------------

/// Pull a library from an OCI registry to a local directory.
/// Args: <registry> <name> <tag> <dest>
/// Example: lib::pull ghcr.io arg-sh/libs/data 0.1.0 /path/to/.argsh/libs/data
fn lib_pull_main(args: &[String]) -> i32 {
    if args.len() < 4 {
        shell::write_stderr("lib::pull: usage: lib::pull <registry> <name> <tag> <dest>");
        return 2;
    }

    let registry = &args[0];
    let name = &args[1];
    let tag = &args[2];
    let dest = &args[3];

    // Create OCI client
    let mut client = match OciClient::new(registry, name, tag) {
        Ok(c) => c,
        Err(e) => {
            shell::write_stderr(&format!("lib::pull: failed to connect to {}: {}", registry, e));
            return 1;
        }
    };

    // Get manifest
    let manifest = match client.get_manifest() {
        Ok(m) => m,
        Err(e) => {
            shell::write_stderr(&format!("lib::pull: failed to get manifest: {}", e));
            return 1;
        }
    };

    // Create destination directory
    if let Err(e) = std::fs::create_dir_all(dest) {
        shell::write_stderr(&format!("lib::pull: failed to create {}: {}", dest, e));
        return 1;
    }

    // Detect current platform for .so filtering
    let current_arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => "",
    };
    let is_musl = std::path::Path::new("/lib/ld-musl-x86_64.so.1").exists()
        || std::path::Path::new("/lib/ld-musl-aarch64.so.1").exists();
    let platform_suffix = if is_musl {
        format!("-linux-musl-{}.so", current_arch)
    } else {
        format!("-linux-{}.so", current_arch)
    };

    // Download each layer
    let mut matching_so: Option<(String, Vec<u8>)> = None;

    for layer in &manifest.layers {
        let filename = layer.annotations.as_ref()
            .and_then(|a| a.get("org.opencontainers.image.title"))
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                layer.digest.split(':').next_back().unwrap_or("blob")
            })
            .to_string();

        // Track .so files for platform filtering
        let is_so = filename.ends_with(".so");
        let is_platform_so = is_so && filename.contains("-linux-");

        // Skip non-matching platform .so files
        if is_platform_so && !filename.ends_with(&platform_suffix) {
            continue;
        }

        let data = match client.get_blob(&layer.digest) {
            Ok(d) => d,
            Err(e) => {
                shell::write_stderr(&format!("lib::pull: failed to get blob {}: {}", layer.digest, e));
                return 1;
            }
        };

        if is_platform_so {
            // Rename platform-specific .so to canonical name (strip platform suffix)
            let canonical = filename.replace(&platform_suffix, ".so");
            let canonical = canonical.replace(['/', '\\'], "_");
            if canonical.contains("..") || canonical.starts_with('.') {
                shell::write_stderr(&format!("lib::pull: refusing unsafe .so filename: {}", canonical));
                return 1;
            }
            matching_so = Some((canonical, data));
        } else {
            // Sanitize filename: reject path traversal
            let filename = filename.replace(['/', '\\'], "_");
            if filename.contains("..") || filename.starts_with('.') {
                shell::write_stderr(&format!("lib::pull: refusing unsafe filename: {}", filename));
                return 1;
            }

            let path = std::path::Path::new(dest).join(&filename);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&path, &data) {
                shell::write_stderr(&format!("lib::pull: failed to write {}: {}", path.display(), e));
                return 1;
            }
        }
    }

    // Write the matching .so with canonical name
    if let Some((canonical, data)) = matching_so {
        let path = std::path::Path::new(dest).join(&canonical);
        if let Err(e) = std::fs::write(&path, &data) {
            shell::write_stderr(&format!("lib::pull: failed to write {}: {}", path.display(), e));
            return 1;
        }
    }

    // Expose digest to bash caller
    let digest = client.resolve_digest().unwrap_or_default();
    shell::set_scalar("__LIB_PULL_DIGEST", &digest);

    // Set result variable for bash
    shell::set_scalar("__LIB_PULL_DEST", dest);
    0
}
