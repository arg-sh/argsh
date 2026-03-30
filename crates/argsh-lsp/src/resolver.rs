use std::collections::{HashMap, HashSet};
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
    /// Whether import resolution actually ran (false when max_depth == 0).
    pub resolution_ran: bool,
}

impl Default for ResolvedImports {
    fn default() -> Self {
        Self {
            functions: Vec::new(),
            resolved_files: Vec::new(),
            resolution_ran: false,
        }
    }
}

/// Parse a `.envrc` file from the given directory and extract variable assignments.
///
/// Supported patterns:
/// - `: "${VAR:=value}"` and `: "${VAR:="value"}"` (argsh .envrc pattern)
/// - `export VAR=value` and `export VAR="value"`
/// - `VAR=value` and `VAR="value"` (plain assignment)
///
/// Lines containing `$(` (command substitution) are skipped.
fn parse_envrc(project_root: &Path) -> HashMap<String, String> {
    let envrc_path = project_root.join(".envrc");
    let content = match std::fs::read_to_string(&envrc_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let mut vars = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Skip lines with command substitution
        if trimmed.contains("$(") {
            continue;
        }

        // Pattern 1: : "${VAR:=value}" or : "${VAR:="value"}"
        if trimmed.starts_with(": \"${") {
            if let Some(inner) = trimmed.strip_prefix(": \"${").and_then(|s| s.strip_suffix("}\"")) {
                if let Some((name, raw_value)) = inner.split_once(":=") {
                    let value = raw_value.trim_matches('"');
                    if !name.is_empty() {
                        let expanded = expand_vars(value, &vars);
                        vars.insert(name.to_string(), expanded);
                    }
                }
            }
            continue;
        }

        // Pattern 2: export VAR=value or export VAR="value"
        if let Some(rest) = trimmed.strip_prefix("export ") {
            let rest = rest.trim();
            if let Some((name, raw_value)) = rest.split_once('=') {
                let name = name.trim();
                let value = raw_value.trim().trim_matches('"');
                if !name.is_empty() {
                    let expanded = expand_vars(value, &vars);
                    vars.insert(name.to_string(), expanded);
                }
            }
            continue;
        }

        // Pattern 3: VAR=value or VAR="value" (plain assignment, optional spaces around =)
        if let Some((name, raw_value)) = trimmed.split_once('=') {
            let name = name.trim();
            // Only accept simple variable names (alphanumeric + underscore)
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let value = raw_value.trim().trim_matches('"');
                let expanded = expand_vars(value, &vars);
                vars.insert(name.to_string(), expanded);
            }
        }
    }

    vars
}

/// Expand `${VAR}` and `$VAR` references using already-parsed variables.
fn expand_vars(value: &str, vars: &HashMap<String, String>) -> String {
    let mut result = value.to_string();
    // Expand ${VAR} patterns — leave unknown variables as-is
    let mut pos = 0;
    while let Some(start) = result[pos..].find("${") {
        let abs_start = pos + start;
        if let Some(end) = result[abs_start..].find('}') {
            let var_name = &result[abs_start + 2..abs_start + end];
            if let Some(replacement) = vars.get(var_name) {
                result = format!("{}{}{}", &result[..abs_start], replacement, &result[abs_start + end + 1..]);
                pos = abs_start + replacement.len();
            } else {
                pos = abs_start + end + 1; // skip unknown ${VAR}
            }
        } else {
            break;
        }
    }
    // Expand $VAR patterns (only word chars after $, not followed by {)
    let mut expanded = String::new();
    let mut chars = result.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '$' {
            if let Some(&(_, next_ch)) = chars.peek() {
                if next_ch != '{' && (next_ch.is_ascii_alphanumeric() || next_ch == '_') {
                    // Collect variable name
                    let start = i + 1;
                    let mut end = start;
                    while let Some(&(pos, c)) = chars.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            end = pos + c.len_utf8();
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let var_name = &result[start..end];
                    if let Some(val) = vars.get(var_name) {
                        expanded.push_str(val);
                    } else {
                        // Leave unknown $VAR as-is
                        expanded.push('$');
                        expanded.push_str(var_name);
                    }
                    continue;
                }
            }
        }
        expanded.push(ch);
    }
    expanded
}

/// Resolve imports from a document analysis, starting from the file at `base_path`.
/// Follows `import` and `source` statements up to `max_depth` levels.
/// Circular imports are handled via canonicalized path tracking.
pub fn resolve_imports(analysis: &DocumentAnalysis, base_path: &Path, max_depth: usize) -> ResolvedImports {
    let mut result = ResolvedImports::default();
    result.resolution_ran = max_depth > 0;
    if max_depth == 0 {
        return result;
    }
    let mut visited = HashSet::new();
    // Add the current file to visited to prevent self-referencing
    if let Ok(canonical) = base_path.canonicalize() {
        visited.insert(canonical);
    }
    let base_dir = base_path.parent().unwrap_or(Path::new("."));

    // Parse .envrc only when env vars are missing (avoids repeated I/O on every change)
    let project_root = find_project_root(base_dir).unwrap_or_else(|| base_dir.to_path_buf());
    // Parse .envrc when env vars are missing or point to non-existent dirs
    let envrc_vars = if std::env::var("PATH_BASE").ok().filter(|v| Path::new(v).is_dir()).is_none()
        || std::env::var("PATH_SCRIPTS").ok().filter(|v| Path::new(v).is_dir()).is_none()
    {
        parse_envrc(&project_root)
    } else {
        HashMap::new()
    };

    resolve_recursive(analysis, base_dir, &mut result, &mut visited, 0, max_depth, &envrc_vars, &project_root);
    result
}

fn resolve_recursive(
    analysis: &DocumentAnalysis,
    base_dir: &Path,
    result: &mut ResolvedImports,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
    max_depth: usize,
    envrc_vars: &HashMap<String, String>,
    project_root: &Path,
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
            // @ prefix: prefer PATH_BASE env var, then .envrc fallback, then project root
            let stripped = &module[1..];
            let resolved_base = std::env::var("PATH_BASE")
                .ok()
                .map(PathBuf::from)
                .filter(|p| p.is_dir())
                .or_else(|| {
                    // .envrc fallback: resolve relative paths against project root
                    envrc_vars.get("PATH_BASE").map(|v| {
                        let p = PathBuf::from(v);
                        if p.is_relative() { project_root.join(&p) } else { p }
                    }).filter(|p| p.is_dir())
                })
                .unwrap_or_else(|| project_root.to_path_buf());
            (stripped.to_string(), resolved_base)
        } else if module.starts_with('^') {
            // ^ prefix: relative to PATH_SCRIPTS — skip if no scripts dir found
            // (matches runtime behavior: fails when PATH_SCRIPTS is unset)
            let stripped = &module[1..];
            match find_scripts_dir(project_root, envrc_vars) {
                Some(scripts_dir) => (stripped.to_string(), scripts_dir),
                None => continue, // unresolvable — AG013 will flag it
            }
        } else if module.starts_with('~') {
            (module[1..].to_string(), base_dir.to_path_buf())
        } else {
            (module.clone(), base_dir.to_path_buf())
        };

        let candidates = resolve_module_path(&clean_module, &search_dir);

        // First-match-wins: mirrors import::source which returns on the first
        // existing file. Without this, both `foo` and `foo.sh` could be imported.
        let resolved = candidates.iter().find(|p| p.is_file());

        if let Some(path) = resolved {
            let canonical = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Always record in resolved_files for goto-def/AG013, even if
            // the file was already analyzed via a different module name
            // (e.g. `import fmt` + `import @libraries/fmt`).
            result
                .resolved_files
                .push((module.clone(), canonical.clone()));

            // Skip analysis/recursion if already visited
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

            // Recurse into the imported file's imports
            let import_dir = canonical.parent().unwrap_or(base_dir);
            resolve_recursive(&imported_analysis, import_dir, result, visited, depth + 1, max_depth, envrc_vars, project_root);
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
fn find_scripts_dir(project_root: &Path, envrc_vars: &HashMap<String, String>) -> Option<PathBuf> {
    // Prefer explicit PATH_SCRIPTS env var (matches runtime behavior)
    if let Ok(path) = std::env::var("PATH_SCRIPTS") {
        let p = PathBuf::from(path);
        if p.is_dir() {
            return Some(p);
        }
    }
    // Second: .envrc fallback
    if let Some(path) = envrc_vars.get("PATH_SCRIPTS") {
        let p = if Path::new(path).is_relative() {
            project_root.join(path)
        } else {
            PathBuf::from(path)
        };
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
    use std::sync::Mutex;

    // Serialize tests that mutate process-wide env vars (PATH_BASE, PATH_SCRIPTS).
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// RAII guard that sets an env var and restores it on drop (even on panic).
    struct EnvGuard {
        name: &'static str,
        prev: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(name: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let lock = ENV_MUTEX.lock().unwrap();
            let prev = std::env::var_os(name);
            unsafe { std::env::set_var(name, value); }
            Self { name, prev, _lock: lock }
        }

        fn clear(name: &'static str) -> Self {
            let lock = ENV_MUTEX.lock().unwrap();
            let prev = std::env::var_os(name);
            unsafe { std::env::remove_var(name); }
            Self { name, prev, _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.name, v); },
                None => unsafe { std::env::remove_var(self.name); },
            }
        }
    }

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
        let _guard = EnvGuard::set("PATH_BASE", dir.path());
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
        let _guard = EnvGuard::clear("PATH_SCRIPTS");
        let dir = tempfile::tempdir().unwrap();
        let scripts = dir.path().join(".scripts");
        fs::create_dir_all(&scripts).unwrap();
        assert_eq!(find_scripts_dir(dir.path(), &HashMap::new()), Some(scripts));
    }

    #[test]
    fn test_find_scripts_dir_scripts() {
        let _guard = EnvGuard::clear("PATH_SCRIPTS");
        let dir = tempfile::tempdir().unwrap();
        let scripts = dir.path().join("scripts");
        fs::create_dir_all(&scripts).unwrap();
        assert_eq!(find_scripts_dir(dir.path(), &HashMap::new()), Some(scripts));
    }

    #[test]
    fn test_find_scripts_dir_bin() {
        let _guard = EnvGuard::clear("PATH_SCRIPTS");
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        assert_eq!(find_scripts_dir(dir.path(), &HashMap::new()), Some(bin));
    }

    #[test]
    fn test_find_scripts_dir_prefers_dot_scripts() {
        let _guard = EnvGuard::clear("PATH_SCRIPTS");
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".scripts")).unwrap();
        fs::create_dir_all(dir.path().join("scripts")).unwrap();
        assert_eq!(find_scripts_dir(dir.path(), &HashMap::new()), Some(dir.path().join(".scripts")));
    }

    #[test]
    fn test_find_scripts_dir_none() {
        let _guard = EnvGuard::clear("PATH_SCRIPTS");
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_scripts_dir(dir.path(), &HashMap::new()), None);
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

    // -------------------------------------------------------------------------
    // .envrc parsing tests

    #[test]
    fn test_parse_envrc_colon_pattern() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".envrc"), ": \"${PATH_BASE:=/some/path}\"\n: \"${PATH_SCRIPTS:=.scripts}\"\n").unwrap();
        let vars = parse_envrc(dir.path());
        assert_eq!(vars.get("PATH_BASE").map(|s| s.as_str()), Some("/some/path"));
        assert_eq!(vars.get("PATH_SCRIPTS").map(|s| s.as_str()), Some(".scripts"));
    }

    #[test]
    fn test_parse_envrc_export_pattern() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".envrc"), "export PATH_BASE=\"/my/project\"\nexport PATH_SCRIPTS=scripts\n").unwrap();
        let vars = parse_envrc(dir.path());
        assert_eq!(vars.get("PATH_BASE").map(|s| s.as_str()), Some("/my/project"));
        assert_eq!(vars.get("PATH_SCRIPTS").map(|s| s.as_str()), Some("scripts"));
    }

    #[test]
    fn test_parse_envrc_skips_command_substitution() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".envrc"), ": \"${PATH_BASE:=$(git rev-parse --show-toplevel)}\"\nPATH_SCRIPTS=.scripts\n").unwrap();
        let vars = parse_envrc(dir.path());
        assert!(vars.get("PATH_BASE").is_none(), "Should skip command substitution");
        assert_eq!(vars.get("PATH_SCRIPTS").map(|s| s.as_str()), Some(".scripts"));
    }

    #[test]
    fn test_parse_envrc_expands_variables() {
        let dir = tempfile::tempdir().unwrap();
        let content = format!(
            ": \"${{PATH_BASE:={}}}\"\nPATH_SCRIPTS=${{PATH_BASE}}/.scripts\nexport PATH_BIN=\"$PATH_BASE/.bin\"\n",
            dir.path().display()
        );
        fs::write(dir.path().join(".envrc"), &content).unwrap();
        let vars = parse_envrc(dir.path());
        assert_eq!(vars.get("PATH_BASE").map(|s| s.as_str()), Some(dir.path().to_str().unwrap()));
        let expected_scripts = format!("{}/.scripts", dir.path().display());
        assert_eq!(vars.get("PATH_SCRIPTS").map(|s| s.as_str()), Some(expected_scripts.as_str()),
            "Should expand ${{PATH_BASE}} in PATH_SCRIPTS");
        let expected_bin = format!("{}/.bin", dir.path().display());
        assert_eq!(vars.get("PATH_BIN").map(|s| s.as_str()), Some(expected_bin.as_str()),
            "Should expand $PATH_BASE in PATH_BIN");
    }

    #[test]
    fn test_parse_envrc_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let vars = parse_envrc(dir.path());
        assert!(vars.is_empty());
    }

    #[test]
    fn test_resolve_at_prefix_from_envrc() {
        let _guard = EnvGuard::clear("PATH_BASE");
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        // Write .envrc with PATH_BASE pointing to temp dir
        fs::write(dir.path().join(".envrc"),
            &format!(": \"${{PATH_BASE:={}}}\"\n", dir.path().display())).unwrap();
        let libs = dir.path().join("libs");
        fs::create_dir_all(&libs).unwrap();
        fs::write(libs.join("helper"), "envrc_func() { :; }").unwrap();

        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        let main_content = "#!/usr/bin/env bash\nimport @libs/helper\nmain() { echo; }\n";
        let main_sh = sub.join("main.sh");
        fs::write(&main_sh, main_content).unwrap();

        let analysis = analyze(main_content);
        let imports = resolve_imports(&analysis, &main_sh, 2);
        assert!(imports.functions.iter().any(|f| f.name == "envrc_func"),
            "Should find function via .envrc PATH_BASE, got: {:?}",
            imports.functions.iter().map(|f| &f.name).collect::<Vec<_>>());
    }
}
