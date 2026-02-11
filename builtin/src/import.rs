//! import builtin — module import with selective functions and aliasing.
//!
//! Mirrors: libraries/import.sh
//!
//! Syntax:
//!   import <module>                          # import all
//!   import <func...> <module>                # selective
//!   import "<func as alias>" <func> <module> # aliasing
//!   import --force <module>                  # bypass cache
//!   import --list                            # show loaded modules

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::shell;
use std::ffi::{c_char, c_int};

// ── import ───────────────────────────────────────────────────────

static IMPORT_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Import a bash module with optional selective function filtering.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "import_struct"]
pub static mut IMPORT_STRUCT: BashBuiltin = BashBuiltin {
    name: c"import".as_ptr(),
    function: import_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"import [--force] [--list] [func...] <module>".as_ptr(),
    long_doc: IMPORT_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "import_builtin_load"]
pub extern "C" fn import_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "import_builtin_unload"]
pub extern "C" fn import_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn import_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        import_main(&args)
    })
    .unwrap_or(1)
}

// ── import::clear ────────────────────────────────────────────────

static IMPORT_CLEAR_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Clear the import cache.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "import::clear_struct"]
pub static mut IMPORT_CLEAR_STRUCT: BashBuiltin = BashBuiltin {
    name: c"import::clear".as_ptr(),
    function: import_clear_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"import::clear".as_ptr(),
    long_doc: IMPORT_CLEAR_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "import::clear_builtin_load"]
pub extern "C" fn import_clear_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "import::clear_builtin_unload"]
pub extern "C" fn import_clear_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn import_clear_builtin_fn(_word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        shell::run_bash("declare -gA import_cache=()");
        0
    })
    .unwrap_or(1)
}

// ── Implementation ───────────────────────────────────────────────

struct FuncSpec {
    original: String,
    alias: Option<String>,
}

/// Parse specifiers: "func1", "func1 as myfunc"
fn parse_specifiers(specs: &[String]) -> Vec<FuncSpec> {
    specs.iter().map(|spec| {
        if let Some((orig, alias)) = spec.split_once(" as ") {
            FuncSpec {
                original: orig.trim().to_string(),
                alias: Some(alias.trim().to_string()),
            }
        } else {
            FuncSpec {
                original: spec.clone(),
                alias: None,
            }
        }
    }).collect()
}

/// Main entry point for import builtin.
pub fn import_main(args: &[String]) -> i32 {
    // Parse flags
    let mut force = false;
    let mut list_mode = false;
    let mut positionals: Vec<String> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--force" => force = true,
            "--list" => list_mode = true,
            _ => positionals.push(arg.clone()),
        }
    }

    // Ensure import_cache exists as associative array
    shell::run_bash("declare -gA import_cache 2>/dev/null || true");

    // --list: print cached module names
    if list_mode {
        let keys = shell::get_assoc_keys("import_cache");
        for key in &keys {
            println!("{}", key);
        }
        return 0;
    }

    if positionals.is_empty() {
        shell::write_stderr("import: missing module argument"); // coverage:off - exit(2) prevents coverage flush in forked subshell
        return 2; // coverage:off - exit(2) prevents coverage flush in forked subshell
    }

    // Last positional = module, rest = function specifiers
    let module = positionals.last().unwrap().clone();
    let specifiers = if positionals.len() > 1 {
        parse_specifiers(&positionals[..positionals.len() - 1])
    } else {
        Vec::new()
    };

    // Cache check — bypass when selective specifiers are present since a prior
    // full import cached the module but we may need different functions.
    if !force && specifiers.is_empty() && shell::assoc_get("import_cache", &module).is_some() {
        return 0;
    }

    // Resolve module path
    let resolved = match resolve_module_path(&module) {
        Some(path) => path,
        None => {
            shell::write_stderr(&format!("Library not found {}", module));
            return 1;
        }
    };

    // Import
    if specifiers.is_empty() {
        // Full import
        let ret = shell::source_bash_file(&resolved);
        if ret != 0 { // coverage:off - source failure for resolved path impossible to trigger
            return ret; // coverage:off
        } // coverage:off
    } else {
        // Selective import: snapshot → source → prune
        let before = shell::get_all_function_names();

        let ret = shell::source_bash_file(&resolved);
        if ret != 0 { // coverage:off - source failure for resolved path impossible to trigger
            return ret; // coverage:off
        } // coverage:off

        let after = shell::get_all_function_names();
        let new_funcs: std::collections::HashSet<String> =
            after.difference(&before).cloned().collect();

        // Validate all requested functions exist
        for spec in &specifiers {
            if !new_funcs.contains(&spec.original) {
                shell::write_stderr(&format!(
                    "import: function '{}' not found in module '{}'",
                    spec.original, module
                ));
                // Cleanup: remove ALL new functions
                for f in &new_funcs {
                    shell::remove_function(f);
                }
                return 1;
            }
        }

        // Build keep set and apply aliases
        let mut keep: std::collections::HashSet<String> = std::collections::HashSet::new();
        for spec in &specifiers {
            if let Some(ref alias) = spec.alias {
                shell::create_function_alias(&spec.original, alias);
                // Keep the original; the alias wrapper calls it.
                keep.insert(spec.original.clone());
            } else {
                keep.insert(spec.original.clone());
            }
        }

        // Remove unwanted new functions
        for f in &new_funcs {
            if !keep.contains(f) {
                shell::remove_function(f);
            }
        }
    }

    // Update cache
    shell::assoc_set("import_cache", &module, "1");

    0
}

/// Get ARGSH_SOURCE only if it's a real path (contains '/').
/// Bare names like "argsh" are used as identifiers, not file paths.
fn get_argsh_source_path() -> Option<String> {
    shell::get_scalar("ARGSH_SOURCE").filter(|s| s.contains('/'))
}

/// Resolve module path following import.sh semantics.
/// Prefixes: @ → PATH_BASE, ~ → ARGSH_SOURCE/BASH_SOURCE[-1],
/// plain → ARGSH_SOURCE/__ARGSH_LIB_DIR/BASH_SOURCE[0]
/// Extension fallback: "", ".sh", ".bash"
fn resolve_module_path(module: &str) -> Option<String> {
    let base_path = if let Some(rest) = module.strip_prefix('@') {
        let path_base = shell::get_scalar("PATH_BASE")?;
        format!("{}/{}", path_base, rest)
    } else if let Some(rest) = module.strip_prefix('~') {
        let src = get_argsh_source_path()
            .or_else(shell::get_bash_source_last)?;
        format!("{}/{}", path_dirname(&src), rest)
    } else {
        // For plain names: try ARGSH_SOURCE (file path → dirname), then
        // __ARGSH_LIB_DIR (already a directory), then BASH_SOURCE[0] (file path → dirname).
        if let Some(src) = get_argsh_source_path() {
            format!("{}/{}", path_dirname(&src), module)
        } else if let Some(lib_dir) = shell::get_scalar("__ARGSH_LIB_DIR") {
            // __ARGSH_LIB_DIR is already a directory, don't apply path_dirname
            format!("{}/{}", lib_dir, module)
        } else {
            let src = shell::get_bash_source_first()?;
            format!("{}/{}", path_dirname(&src), module)
        }
    };

    for ext in &["", ".sh", ".bash"] {
        let full = format!("{}{}", base_path, ext);
        if std::path::Path::new(&full).is_file() {
            return Some(full);
        }
    }

    None
}

/// Get directory component of a path (equivalent to ${var%/*}).
fn path_dirname(path: &str) -> &str {
    if let Some(pos) = path.rfind('/') {
        &path[..pos]
    } else {
        "."
    }
}
