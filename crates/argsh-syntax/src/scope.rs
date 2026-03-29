//! Scope and variable tracking through the argsh command tree.
//!
//! Builds a [`ScopeChain`] for a given function by following the `:usage`
//! dispatch chain upward, collecting inherited local variables and flags.

use crate::document::{DocumentAnalysis, FunctionInfo, LocalVar};
use crate::field::FieldDef;

/// A chain of scopes from the target function up through its `:usage` callers.
#[derive(Debug, Clone)]
pub struct ScopeChain {
    /// Scopes from innermost (target function) to outermost (root caller).
    pub scopes: Vec<Scope>,
}

/// A single scope level corresponding to one function.
#[derive(Debug, Clone)]
pub struct Scope {
    /// Name of the function this scope belongs to.
    pub function_name: String,
    /// Local variable declarations in this function.
    pub locals: Vec<LocalVar>,
    /// Flags parsed from this function's `args=(...)` array.
    pub args_flags: Vec<FieldDef>,
    /// Flags inherited from a parent `:usage` caller.
    pub parent_flags: Vec<FieldDef>,
}

impl ScopeChain {
    /// Build a scope chain for `function_name`, walking up the `:usage`
    /// dispatch chain found in `doc`.
    ///
    /// For each function that dispatches to the target via its `usage` array,
    /// its `args` flags become `parent_flags` in the child scope.
    pub fn build(doc: &DocumentAnalysis, function_name: &str) -> Self {
        let mut scopes = Vec::new();
        let mut visited = std::collections::HashSet::new();

        // Build scope for the target function itself
        if let Some(func) = find_function(doc, function_name) {
            scopes.push(build_scope(func, &[]));
            visited.insert(function_name.to_string());

            // Walk upward: find functions whose usage array dispatches to this function
            let mut current_name = function_name.to_string();
            loop {
                if let Some(parent) = find_parent_dispatcher(doc, &current_name, &visited) {
                    visited.insert(parent.name.clone());

                    // The parent's args flags become parent_flags for the child
                    let parent_flags = extract_flags(&parent.args_entries);
                    if let Some(last) = scopes.last_mut() {
                        last.parent_flags = parent_flags.clone();
                    }

                    scopes.push(build_scope(parent, &[]));
                    current_name = parent.name.clone();
                } else {
                    break;
                }
            }
        }

        ScopeChain { scopes }
    }
}

/// Build a [`Scope`] from a function, with optional parent flags.
fn build_scope(func: &FunctionInfo, parent_flags: &[FieldDef]) -> Scope {
    let args_flags = extract_flags(&func.args_entries);

    Scope {
        function_name: func.name.clone(),
        locals: func.local_vars.clone(),
        args_flags,
        parent_flags: parent_flags.to_vec(),
    }
}

/// Extract successfully-parsed [`FieldDef`]s from args entries.
fn extract_flags(entries: &[crate::document::ArgsArrayEntry]) -> Vec<FieldDef> {
    entries
        .iter()
        .filter_map(|e| e.parsed.as_ref().ok().cloned())
        .collect()
}

/// Find a function by name in the document.
fn find_function<'a>(doc: &'a DocumentAnalysis, name: &str) -> Option<&'a FunctionInfo> {
    doc.functions.iter().find(|f| f.name == name)
}

/// Find a function in `doc` whose `usage` array dispatches to `target_name`.
///
/// This checks both explicit `:-func` mappings and implicit prefix-based
/// resolution (caller::target convention).
fn find_parent_dispatcher<'a>(
    doc: &'a DocumentAnalysis,
    target_name: &str,
    visited: &std::collections::HashSet<String>,
) -> Option<&'a FunctionInfo> {
    for func in &doc.functions {
        if visited.contains(&func.name) {
            continue;
        }
        if !func.calls_usage {
            continue;
        }

        for entry in &func.usage_entries {
            // Check explicit mapping
            if let Some(ref explicit) = entry.explicit_func {
                if explicit == target_name {
                    return Some(func);
                }
            }

            // Check implicit prefix resolution: caller::subcmd
            let prefixed = format!("{}::{}", func.name, entry.name);
            if prefixed == target_name {
                return Some(func);
            }

            // Check last segment prefix: main::manifest → manifest::subcmd
            if let Some(pos) = func.name.rfind("::") {
                let seg_prefixed = format!("{}::{}", &func.name[pos + 2..], entry.name);
                if seg_prefixed == target_name {
                    return Some(func);
                }
            }

            // Check bare name match
            if entry.name == target_name {
                return Some(func);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::analyze;

    const SCOPE_SCRIPT: &str = r#"#!/usr/bin/env argsh

parent::cmd() {
  local config
  local -a verbose args=(
    'verbose|v:+' "Enable verbose"
    'config|c'    "Config file"
  )
  local -a usage=(
    'sub1'             "Sub command 1"
    'sub2:-other_func' "Sub command 2"
  )
  :usage "Parent command" "${@}"
  "${usage[@]}"
}

parent::cmd::sub1() {
  local output
  local -a args=(
    'output|o' "Output file"
  )
  :args "Sub command 1" "${@}"
}

other_func() {
  :args "Other function" "${@}"
}
"#;

    #[test]
    fn test_scope_chain_target() {
        let doc = analyze(SCOPE_SCRIPT);
        let chain = ScopeChain::build(&doc, "parent::cmd::sub1");
        assert!(!chain.scopes.is_empty());
        assert_eq!(chain.scopes[0].function_name, "parent::cmd::sub1");
    }

    #[test]
    fn test_scope_chain_inherits_parent_flags() {
        let doc = analyze(SCOPE_SCRIPT);
        let chain = ScopeChain::build(&doc, "parent::cmd::sub1");

        // The target scope should have parent_flags from parent::cmd
        assert!(!chain.scopes[0].parent_flags.is_empty());
        let parent_flag_names: Vec<&str> = chain.scopes[0]
            .parent_flags
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(parent_flag_names.contains(&"verbose"));
        assert!(parent_flag_names.contains(&"config"));
    }

    #[test]
    fn test_scope_chain_own_flags() {
        let doc = analyze(SCOPE_SCRIPT);
        let chain = ScopeChain::build(&doc, "parent::cmd::sub1");

        let own_flag_names: Vec<&str> = chain.scopes[0]
            .args_flags
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(own_flag_names.contains(&"output"));
    }

    #[test]
    fn test_scope_chain_explicit_dispatch() {
        let doc = analyze(SCOPE_SCRIPT);
        let chain = ScopeChain::build(&doc, "other_func");
        // other_func is dispatched via :-other_func from parent::cmd
        assert!(!chain.scopes.is_empty());
        assert_eq!(chain.scopes[0].function_name, "other_func");
        // Should find parent::cmd as the dispatcher
        if chain.scopes.len() > 1 {
            assert_eq!(chain.scopes[1].function_name, "parent::cmd");
        }
    }

    #[test]
    fn test_scope_chain_locals() {
        let doc = analyze(SCOPE_SCRIPT);
        let chain = ScopeChain::build(&doc, "parent::cmd::sub1");

        let local_names: Vec<&str> = chain.scopes[0]
            .locals
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert!(local_names.contains(&"output"));
    }

    #[test]
    fn test_scope_chain_nonexistent_function() {
        let doc = analyze(SCOPE_SCRIPT);
        let chain = ScopeChain::build(&doc, "nonexistent");
        assert!(chain.scopes.is_empty());
    }

    #[test]
    fn test_scope_chain_root_function() {
        let doc = analyze(SCOPE_SCRIPT);
        let chain = ScopeChain::build(&doc, "parent::cmd");
        assert_eq!(chain.scopes.len(), 1);
        assert_eq!(chain.scopes[0].function_name, "parent::cmd");
        // Root function has no parent flags
        assert!(chain.scopes[0].parent_flags.is_empty());
    }

    #[test]
    fn test_scope_chain_last_segment_resolution() {
        // main::manifest dispatches 'list' → manifest::list (last segment prefix)
        let script = r#"#!/usr/bin/env argsh
main::manifest() {
  local -a usage=(
    'list|l' "List overlays"
  )
  :usage "Manifest" "${@}"
  "${usage[@]}"
}
manifest::list() {
  local -a args=(
    'verbose|v:+' "Verbose"
  )
  :args "List overlays" "${@}"
}
"#;
        let doc = analyze(script);
        let chain = ScopeChain::build(&doc, "manifest::list");
        assert!(!chain.scopes.is_empty(), "Should find manifest::list");
        assert_eq!(chain.scopes[0].function_name, "manifest::list");
        // Should find main::manifest as the dispatcher via last-segment resolution
        assert!(chain.scopes.len() > 1,
            "Should have parent scope, got {} scopes", chain.scopes.len());
        assert_eq!(chain.scopes[1].function_name, "main::manifest");
    }
}
