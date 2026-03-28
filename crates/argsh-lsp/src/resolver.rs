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
