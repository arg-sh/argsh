use std::collections::HashSet;
use std::path::{Path, PathBuf};

use argsh_syntax::document::{analyze, DocumentAnalysis, FunctionInfo};

/// Default maximum depth for recursive import resolution.
pub const DEFAULT_MAX_DEPTH: usize = 2;

/// Resolved imports: collected function definitions from imported files.
pub struct ResolvedImports {
    /// All functions found in imported files.
    pub functions: Vec<FunctionInfo>,
    /// All resolved file paths (for go-to-definition across files).
    pub resolved_files: Vec<(String, PathBuf)>, // (module_name, path)
}

impl Default for ResolvedImports {
    fn default() -> Self {
        Self {
            functions: Vec::new(),
            resolved_files: Vec::new(),
        }
    }
}

/// Resolve imports from a document analysis, starting from the file at `base_path`.
/// Follows `import` and `source` statements up to `max_depth` levels.
/// Circular imports are handled via canonicalized path tracking.
pub fn resolve_imports(analysis: &DocumentAnalysis, base_path: &Path, max_depth: usize) -> ResolvedImports {
    let mut result = ResolvedImports::default();
    let mut visited = HashSet::new();
    // Add the current file to visited to prevent self-referencing
    if let Ok(canonical) = base_path.canonicalize() {
        visited.insert(canonical);
    }
    let base_dir = base_path.parent().unwrap_or(Path::new("."));

    resolve_recursive(analysis, base_dir, &mut result, &mut visited, 0, max_depth);
    result
}

fn resolve_recursive(
    analysis: &DocumentAnalysis,
    base_dir: &Path,
    result: &mut ResolvedImports,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
    max_depth: usize,
) {
    if depth >= max_depth {
        return;
    }

    // Process imports
    for imp in &analysis.imports {
        let module = &imp.module;

        // Handle import prefixes (mirrors import.sh):
        // @foo → relative to PATH_BASE (project root)
        // ^foo → relative to PATH_SCRIPTS
        // ~foo → relative to the script itself
        // foo  → relative to ARGSH_SOURCE directory (base_dir)
        let (clean_module, search_dir) = if module.starts_with('@') {
            // @ prefix: prefer PATH_BASE env var (matches runtime), fall back to project root
            let stripped = &module[1..];
            let project_root = std::env::var("PATH_BASE")
                .ok()
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
                .unwrap_or_else(|| find_project_root(base_dir).unwrap_or_else(|| base_dir.to_path_buf()));
            (stripped.to_string(), project_root)
        } else if module.starts_with('^') {
            // ^ prefix: relative to PATH_SCRIPTS — try to find .scripts/ directory
            let stripped = &module[1..];
            let project_root = find_project_root(base_dir).unwrap_or_else(|| base_dir.to_path_buf());
            let scripts_dir = find_scripts_dir(&project_root).unwrap_or(base_dir.to_path_buf());
            (stripped.to_string(), scripts_dir)
        } else if module.starts_with('~') {
            (module[1..].to_string(), base_dir.to_path_buf())
        } else {
            (module.clone(), base_dir.to_path_buf())
        };

        let candidates = resolve_module_path(&clean_module, &search_dir);

        // First-match-wins: mirrors import::source which returns on the first
        // existing file. Without this, both `foo` and `foo.sh` could be imported.
        let resolved = candidates.iter().find(|p| {
            if let Ok(c) = p.canonicalize() {
                !visited.contains(&c)
            } else {
                false
            }
        });

        if let Some(path) = resolved {
            let canonical = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };

            visited.insert(canonical.clone());

            // Read and analyze the file
            let content = match std::fs::read_to_string(&canonical) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let imported_analysis = analyze(&content);

            // Collect functions
            for func in &imported_analysis.functions {
                result.functions.push(func.clone());
            }

            result
                .resolved_files
                .push((module.clone(), canonical.clone()));

            // Recurse into the imported file's imports
            let import_dir = canonical.parent().unwrap_or(base_dir);
            resolve_recursive(&imported_analysis, import_dir, result, visited, depth + 1, max_depth);
        }
    }

    // Process `source argsh` statements
    if analysis.has_source_argsh {
        let argsh_candidates = find_argsh_lib_dir(base_dir);
        for lib_dir in argsh_candidates {
            for filename in &["main.sh", "args.sh"] {
                let file_path = lib_dir.join(filename);
                if !file_path.exists() {
                    continue;
                }
                let canonical = match file_path.canonicalize() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if visited.contains(&canonical) {
                    continue;
                }
                visited.insert(canonical.clone());

                let content = match std::fs::read_to_string(&canonical) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let imported = analyze(&content);
                for func in &imported.functions {
                    result.functions.push(func.clone());
                }

                let label = format!("argsh:{}", filename.trim_end_matches(".sh"));
                result.resolved_files.push((label, canonical));
            }
        }
    }
}

/// Resolve a module name to candidate file paths.
/// Find the project root by walking up looking for `.bin/argsh`, `.envrc`, or `.git`.
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..10 {
        if dir.join(".bin/argsh").exists()
            || dir.join(".envrc").exists()
            || dir.join(".git").exists()
        {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Find the PATH_SCRIPTS directory from a project root.
/// Looks for common script directory names.
fn find_scripts_dir(project_root: &Path) -> Option<PathBuf> {
    // Prefer explicit PATH_SCRIPTS env var (matches runtime behavior)
    if let Ok(path) = std::env::var("PATH_SCRIPTS") {
        let p = PathBuf::from(path);
        if p.is_dir() {
            return Some(p);
        }
    }
    // Heuristic fallback: common script directory names
    let candidates = [".scripts", "scripts", "bin"];
    for name in &candidates {
        let dir = project_root.join(name);
        if dir.is_dir() {
            return Some(dir);
        }
    }
    None
}

/// argsh import convention — mirrors `import::source` from libraries/import.sh:
/// Tries `{module}`, `{module}.sh`, `{module}.bash` in each search directory.
fn resolve_module_path(module: &str, base_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let extensions = ["", ".sh", ".bash"];

    // Direct: base_dir/module{ext}
    for ext in &extensions {
        candidates.push(base_dir.join(format!("{}{}", module, ext)));
    }

    // With libraries/ prefix
    for ext in &extensions {
        candidates.push(
            base_dir
                .join("libraries")
                .join(format!("{}{}", module, ext)),
        );
    }

    // Walk up to find project root with a libraries/ directory
    let mut dir = base_dir.to_path_buf();
    for _ in 0..5 {
        let lib_dir = dir.join("libraries");
        if lib_dir.is_dir() {
            for ext in &extensions {
                candidates.push(lib_dir.join(format!("{}{}", module, ext)));
            }
        }
        if !dir.pop() {
            break;
        }
    }

    candidates
}

/// Find the argsh library directory from a base path.
fn find_argsh_lib_dir(base_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Check ARGSH_SOURCE env var
    if let Ok(source) = std::env::var("ARGSH_SOURCE") {
        let source_path = Path::new(&source);
        if let Some(parent) = source_path.parent() {
            if parent.join("main.sh").exists() {
                candidates.push(parent.to_path_buf());
            }
        }
    }

    // Walk up from base_dir looking for libraries/ directory
    let mut dir = base_dir.to_path_buf();
    for _ in 0..5 {
        let lib_dir = dir.join("libraries");
        if lib_dir.join("main.sh").exists() {
            candidates.push(lib_dir);
        }
        if !dir.pop() {
            break;
        }
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_resolve_module_path_finds_sh_file() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("mylib.sh");
        fs::write(&lib, "mylib_func() { echo hi; }").unwrap();

        let candidates = resolve_module_path("mylib", dir.path());
        assert!(candidates.iter().any(|p| p == &lib),
            "Should find mylib.sh, candidates: {:?}", candidates);
    }

    #[test]
    fn test_resolve_module_path_finds_in_libraries_dir() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("libraries");
        fs::create_dir_all(&lib_dir).unwrap();
        let lib = lib_dir.join("string.sh");
        fs::write(&lib, "string::trim() { echo; }").unwrap();

        let candidates = resolve_module_path("string", dir.path());
        assert!(candidates.iter().any(|p| p == &lib),
            "Should find libraries/string.sh, candidates: {:?}", candidates);
    }

    #[test]
    fn test_resolve_imports_finds_functions() {
        let dir = tempfile::tempdir().unwrap();

        // Create a library file
        let lib = dir.path().join("helpers.sh");
        fs::write(&lib, "helper_func() {\n  :args \"Help\" \"${@}\"\n}\n").unwrap();

        // Create a main script that imports it
        let main_content = "#!/usr/bin/env bash\nimport helpers\nmain() { echo; }\n";
        let main_sh = dir.path().join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);

        assert!(!imports.functions.is_empty(),
            "Should find functions from imported file");
        assert!(imports.functions.iter().any(|f| f.name == "helper_func"),
            "Should find helper_func, got: {:?}", imports.functions.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    #[test]
    fn test_resolve_imports_handles_circular() {
        let dir = tempfile::tempdir().unwrap();

        // a.sh imports b.sh, b.sh imports a.sh
        let a = dir.path().join("a.sh");
        let b = dir.path().join("b.sh");
        fs::write(&a, "import b\nfunc_a() { echo a; }\n").unwrap();
        fs::write(&b, "import a\nfunc_b() { echo b; }\n").unwrap();

        let analysis = analyze(&fs::read_to_string(&a).unwrap());
        let imports = resolve_imports(&analysis, &a, 3);

        // Should not hang or crash
        assert!(imports.functions.iter().any(|f| f.name == "func_b"),
            "Should find func_b from b.sh");
        // Should NOT find func_a again (it's the current file, visited)
    }

    #[test]
    fn test_resolve_imports_respects_max_depth() {
        let dir = tempfile::tempdir().unwrap();

        // Chain: main -> a -> b -> c
        let a = dir.path().join("a.sh");
        let b = dir.path().join("b.sh");
        let c = dir.path().join("c.sh");
        fs::write(&a, "import b\nfunc_a() { echo; }\n").unwrap();
        fs::write(&b, "import c\nfunc_b() { echo; }\n").unwrap();
        fs::write(&c, "func_c() { echo; }\n").unwrap();

        let main_content = "import a\nmain() { echo; }\n";
        let main_sh = dir.path().join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        // Depth 1: only finds a.sh
        let analysis = analyze(main_content);
        let imports_d1 = resolve_imports(&analysis, &main_sh, 1);
        assert!(imports_d1.functions.iter().any(|f| f.name == "func_a"));
        assert!(!imports_d1.functions.iter().any(|f| f.name == "func_b"),
            "Depth 1 should not reach b.sh");

        // Depth 2: finds a.sh and b.sh
        let imports_d2 = resolve_imports(&analysis, &main_sh, 2);
        assert!(imports_d2.functions.iter().any(|f| f.name == "func_a"));
        assert!(imports_d2.functions.iter().any(|f| f.name == "func_b"));
        assert!(!imports_d2.functions.iter().any(|f| f.name == "func_c"),
            "Depth 2 should not reach c.sh");

        // Depth 3: finds all
        let imports_d3 = resolve_imports(&analysis, &main_sh, 3);
        assert!(imports_d3.functions.iter().any(|f| f.name == "func_c"),
            "Depth 3 should reach c.sh");
    }

    #[test]
    fn test_resolve_imports_depth_zero_no_imports() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("helpers.sh");
        fs::write(&lib, "helper() { echo; }\n").unwrap();

        let main_content = "import helpers\nmain() { echo; }\n";
        let main_sh = dir.path().join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 0);
        assert!(imports.functions.is_empty(), "Depth 0 should import nothing");
    }

    #[test]
    fn test_resolve_module_path_extensionless() {
        let dir = tempfile::tempdir().unwrap();
        // File without .sh extension (like lok8s scripts)
        let lib = dir.path().join("mylib");
        fs::write(&lib, "mylib_func() { echo hi; }").unwrap();

        let candidates = resolve_module_path("mylib", dir.path());
        assert!(candidates.iter().any(|p| p == &lib),
            "Should find extensionless file, candidates: {:?}", candidates);
    }

    #[test]
    fn test_resolve_module_path_prefers_no_extension_first() {
        let dir = tempfile::tempdir().unwrap();
        // Both extensionless and .sh exist
        let no_ext = dir.path().join("mylib");
        let with_sh = dir.path().join("mylib.sh");
        fs::write(&no_ext, "from_no_ext() { :; }").unwrap();
        fs::write(&with_sh, "from_sh() { :; }").unwrap();

        let candidates = resolve_module_path("mylib", dir.path());
        // The extensionless should come first (matches import::source order)
        let no_ext_idx = candidates.iter().position(|p| p == &no_ext);
        let sh_idx = candidates.iter().position(|p| p == &with_sh);
        assert!(no_ext_idx.is_some(), "Should include extensionless");
        assert!(sh_idx.is_some(), "Should include .sh");
        assert!(no_ext_idx.unwrap() < sh_idx.unwrap(),
            "Extensionless should come before .sh");
    }

    #[test]
    fn test_resolve_imports_extensionless_file() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("helpers");
        fs::write(&lib, "helper_func() {\n  echo help\n}\n").unwrap();

        let main_content = "#!/usr/bin/env bash\nimport helpers\nmain() { echo; }\n";
        let main_sh = dir.path().join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);
        assert!(imports.functions.iter().any(|f| f.name == "helper_func"),
            "Should find functions from extensionless imported file");
    }

    #[test]
    fn test_resolve_module_path_with_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let libs_dir = dir.path().join("libs");
        fs::create_dir_all(&libs_dir).unwrap();
        let lib = libs_dir.join("provision");
        fs::write(&lib, "provision_func() { :; }").unwrap();

        let candidates = resolve_module_path("libs/provision", dir.path());
        assert!(candidates.iter().any(|p| p == &lib),
            "Should find libs/provision (extensionless subdir path), candidates: {:?}", candidates);
    }

    #[test]
    fn test_resolve_at_prefix_import() {
        let dir = tempfile::tempdir().unwrap();
        // Set PATH_BASE to temp dir root (resolver prefers env var over walk-up)
        unsafe { std::env::set_var("PATH_BASE", dir.path()); }
        // Create project structure: root/libs/helper
        let libs_dir = dir.path().join("libs");
        fs::create_dir_all(&libs_dir).unwrap();
        let helper = libs_dir.join("helper");
        fs::write(&helper, "at_helper() { :; }").unwrap();

        // Script in a subdirectory
        let scripts_dir = dir.path().join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        let main_content = "#!/usr/bin/env bash\nimport @libs/helper\nmain() { echo; }\n";
        let main_sh = scripts_dir.join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);
        unsafe { std::env::remove_var("PATH_BASE"); }
        assert!(imports.functions.iter().any(|f| f.name == "at_helper"),
            "Should find function from @-prefixed import, got: {:?}",
            imports.functions.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    #[test]
    fn test_resolve_tilde_prefix_import() {
        let dir = tempfile::tempdir().unwrap();
        // ~ prefix resolves relative to the script file
        let helper = dir.path().join("helper.sh");
        fs::write(&helper, "tilde_helper() { :; }").unwrap();

        let main_content = "#!/usr/bin/env bash\nimport ~helper\nmain() { echo; }\n";
        let main_sh = dir.path().join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);
        assert!(imports.functions.iter().any(|f| f.name == "tilde_helper"),
            "Should find function from ~-prefixed import");
    }

    #[test]
    fn test_find_scripts_dir_dot_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = dir.path().join(".scripts");
        fs::create_dir_all(&scripts).unwrap();

        let found = find_scripts_dir(dir.path());
        assert_eq!(found, Some(scripts));
    }

    #[test]
    fn test_find_scripts_dir_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = dir.path().join("scripts");
        fs::create_dir_all(&scripts).unwrap();

        let found = find_scripts_dir(dir.path());
        assert_eq!(found, Some(scripts));
    }

    #[test]
    fn test_find_scripts_dir_bin() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();

        let found = find_scripts_dir(dir.path());
        assert_eq!(found, Some(bin));
    }

    #[test]
    fn test_find_scripts_dir_prefers_dot_scripts() {
        let dir = tempfile::tempdir().unwrap();
        // Both .scripts and scripts exist — .scripts should win
        fs::create_dir_all(dir.path().join(".scripts")).unwrap();
        fs::create_dir_all(dir.path().join("scripts")).unwrap();

        let found = find_scripts_dir(dir.path());
        assert_eq!(found, Some(dir.path().join(".scripts")));
    }

    #[test]
    fn test_find_scripts_dir_none() {
        let dir = tempfile::tempdir().unwrap();
        // No scripts directory at all
        let found = find_scripts_dir(dir.path());
        assert_eq!(found, None);
    }

    #[test]
    fn test_resolve_caret_prefix_import() {
        let dir = tempfile::tempdir().unwrap();
        // Create project structure: root/.git + root/.scripts/utils/verbose
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let scripts_dir = dir.path().join(".scripts");
        let utils_dir = scripts_dir.join("utils");
        fs::create_dir_all(&utils_dir).unwrap();
        let verbose = utils_dir.join("verbose");
        fs::write(&verbose, "verbose_func() { :; }").unwrap();

        // Script in a subdirectory
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        let main_content = "#!/usr/bin/env bash\nimport ^utils/verbose\nmain() { echo; }\n";
        let main_sh = sub.join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);
        assert!(imports.functions.iter().any(|f| f.name == "verbose_func"),
            "Should find function from ^-prefixed import, got: {:?}",
            imports.functions.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    #[test]
    fn test_resolve_caret_prefix_with_sh_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let scripts_dir = dir.path().join(".scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        let helper = scripts_dir.join("helper.sh");
        fs::write(&helper, "caret_helper() { :; }").unwrap();

        let main_content = "#!/usr/bin/env bash\nimport ^helper\nmain() { echo; }\n";
        let main_sh = dir.path().join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);
        assert!(imports.functions.iter().any(|f| f.name == "caret_helper"),
            "Should find function from ^-prefixed import with .sh extension");
    }

    #[test]
    fn test_resolve_caret_prefix_resolved_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let scripts_dir = dir.path().join(".scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        let lib = scripts_dir.join("mylib");
        fs::write(&lib, "mylib_func() { :; }").unwrap();

        let main_content = "#!/usr/bin/env bash\nimport ^mylib\nmain() { echo; }\n";
        let main_sh = dir.path().join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);
        assert!(imports.resolved_files.iter().any(|(name, _)| name == "^mylib"),
            "resolved_files should preserve ^ prefix in module name, got: {:?}",
            imports.resolved_files.iter().map(|(n, _)| n).collect::<Vec<_>>());
    }
}
