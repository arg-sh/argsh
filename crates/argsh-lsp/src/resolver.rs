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

        // Try to resolve the module to a file path
        let candidates = resolve_module_path(module, base_dir);

        for path in candidates {
            let canonical = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };

            if visited.contains(&canonical) {
                continue;
            }
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
/// argsh import convention: `import foo` -> `{base_dir}/foo.sh`
fn resolve_module_path(module: &str, base_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Direct: base_dir/module.sh
    candidates.push(base_dir.join(format!("{}.sh", module)));

    // With libraries/ prefix
    candidates.push(
        base_dir
            .join("libraries")
            .join(format!("{}.sh", module)),
    );

    // Walk up to find project root with a libraries/ directory
    let mut dir = base_dir.to_path_buf();
    for _ in 0..5 {
        let lib_dir = dir.join("libraries");
        if lib_dir.is_dir() {
            candidates.push(lib_dir.join(format!("{}.sh", module)));
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
}
