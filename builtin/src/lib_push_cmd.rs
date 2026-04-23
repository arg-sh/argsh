//! lib::push builtin — push a library to an OCI registry.
//!
//! Called by argsh::lib::publish. Requires credentials in ~/.docker/config.json.

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::oci::OciClient;
use crate::shell;
use std::ffi::{c_char, c_int};

// -- Builtin registration ---------------------------------------------------

static LIB_PUSH_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Push a library to an OCI registry.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "lib::push_struct"]
pub static mut LIB_PUSH_STRUCT: BashBuiltin = BashBuiltin {
    name: c"lib::push".as_ptr(),
    function: lib_push_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"lib::push <registry> <name> <tag> <source_dir>".as_ptr(),
    long_doc: LIB_PUSH_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "lib::push_builtin_load"]
pub extern "C" fn lib_push_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "lib::push_builtin_unload"]
pub extern "C" fn lib_push_builtin_unload(_name: *const c_char) {}

extern "C" fn lib_push_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        lib_push_main(&args)
    })
    .unwrap_or(1)
}

// -- Implementation ---------------------------------------------------------

/// Media type for a file based on its extension.
fn media_type_for(filename: &str) -> &'static str {
    if filename.ends_with(".sh") || filename.ends_with(".bash") {
        "application/vnd.argsh.lib.v1+bash"
    } else if filename.ends_with(".yml") || filename.ends_with(".yaml") {
        "application/vnd.argsh.plugin.v1+yaml"
    } else if filename.ends_with(".so") {
        "application/vnd.argsh.builtin.v1+so"
    } else if filename.ends_with(".bats") {
        "application/vnd.argsh.test.v1+bats"
    } else {
        // Extensionless files (like the jaml executable) are treated as bash
        "application/vnd.argsh.lib.v1+bash"
    }
}

/// Push a library directory to an OCI registry.
/// Args: <registry> <name> <tag> <source_dir>
fn lib_push_main(args: &[String]) -> i32 {
    if args.len() < 4 {
        shell::write_stderr("lib::push: usage: lib::push <registry> <name> <tag> <source_dir>");
        return 2;
    }

    let registry = &args[0];
    let name = &args[1];
    let tag = &args[2];
    let source_dir = &args[3];

    let dir = std::path::Path::new(source_dir);
    if !dir.is_dir() {
        shell::write_stderr(&format!("lib::push: not a directory: {}", source_dir));
        return 1;
    }

    // Create OCI client
    let mut client = match OciClient::new(registry, name, tag) {
        Ok(c) => c,
        Err(e) => {
            shell::write_stderr(&format!("lib::push: failed to connect to {}: {}", registry, e));
            return 1;
        }
    };

    // Collect files from source directory
    let entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => {
            let mut v: Vec<_> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .collect();
            v.sort_by_key(|e| e.file_name());
            v
        }
        Err(e) => {
            shell::write_stderr(&format!("lib::push: failed to read {}: {}", source_dir, e));
            return 1;
        }
    };

    if entries.is_empty() {
        shell::write_stderr("lib::push: no files to push in source directory");
        return 1;
    }

    // Push each file as a blob and build layer descriptors
    let mut layers: Vec<serde_json::Value> = Vec::new();
    for entry in &entries {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                shell::write_stderr(&format!("lib::push: failed to read {}: {}", path.display(), e));
                return 1;
            }
        };

        let size = data.len() as u64;
        let digest = match client.push_blob(&data) {
            Ok(d) => d,
            Err(e) => {
                shell::write_stderr(&format!("lib::push: failed to push blob {}: {}", filename, e));
                return 1;
            }
        };

        let media_type = media_type_for(&filename);
        let mut annotations = serde_json::json!({
            "org.opencontainers.image.title": filename
        });
        // Add platform annotation for platform-specific .so files
        if filename.ends_with(".so") && filename.contains("-linux-") {
            let platform = if filename.contains("-musl-") {
                if filename.contains("-amd64.so") { "linux-musl/amd64" }
                else if filename.contains("-arm64.so") { "linux-musl/arm64" }
                else { "linux/unknown" }
            } else if filename.contains("-amd64.so") { "linux/amd64" }
            else if filename.contains("-arm64.so") { "linux/arm64" }
            else { "linux/unknown" };
            annotations.as_object_mut().unwrap()
                .insert("org.argsh.platform".to_string(), serde_json::Value::String(platform.to_string()));
        }
        layers.push(serde_json::json!({
            "mediaType": media_type,
            "digest": digest,
            "size": size,
            "annotations": annotations
        }));
    }

    // Push empty config blob (OCI spec requirement)
    let config_data = b"{}";
    let config_digest = match client.push_blob(config_data) {
        Ok(d) => d,
        Err(e) => {
            shell::write_stderr(&format!("lib::push: failed to push config: {}", e));
            return 1;
        }
    };

    // Build manifest
    let manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": config_digest,
            "size": config_data.len()
        },
        "layers": layers
    });

    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();

    // Push manifest
    let manifest_digest = match client.push_manifest(&manifest_bytes) {
        Ok(d) => d,
        Err(e) => {
            shell::write_stderr(&format!("lib::push: failed to push manifest: {}", e));
            return 1;
        }
    };

    // Set result variables for bash
    shell::set_scalar("__LIB_PUSH_DIGEST", &manifest_digest);
    shell::set_scalar("__LIB_PUSH_REF", &format!("{}/{}:{}", registry, name, tag));
    0
}
