# LSP Phase 2 — Future Work

## Workspace-Wide Analysis

### What
Scan all `.sh`/`.bash` files in the workspace (not just the open file + imports) to build a global function index. This enables:
- Cross-file diagnostics (unused functions, orphaned commands)
- Project-wide rename symbol
- Find all references
- Import suggestions ("did you mean to import X?")

### Why
Currently the LSP only analyzes the open file + files reachable via `import`/`source` (up to `resolveDepth` levels). Functions defined in files that aren't directly imported are invisible — no goto-def, no diagnostics suppression, no completions.

### How to Implement
1. **File watcher**: Register `workspace/didChangeWatchedFiles` to track `.sh` file changes
2. **Background indexer**: On workspace open, scan all `.sh` files and build a `DashMap<String, (Url, FunctionInfo)>` mapping function names to their locations
3. **Incremental updates**: On file save, re-analyze only the changed file and update the index
4. **Integration points**:
   - `goto_definition`: fall back to the workspace index when imports don't resolve
   - `diagnostics`: suppress AG007 if the function exists anywhere in the workspace
   - `completion`: suggest functions from the workspace index
   - `references`: find all files that reference a function name
5. **Performance**: Use `ignore` crate to respect `.gitignore`, limit scan to reasonable file sizes (<100KB), skip `node_modules`/`target`/`.git`

### Estimated effort
3-4 days for basic indexing + goto-def. 1-2 weeks for full references + rename across files.

---

## Bash Debugger Integration

### What
Integrate with `bashdb` (Bash Debugger) to provide step-through debugging for argsh scripts directly in VSCode.

### Why
Debugging bash scripts currently requires `set -x` or `echo` statements. A proper debugger with breakpoints, variable inspection, and step-through would significantly improve the development experience, especially for complex scripts with nested `:usage` dispatch.

### How to Implement

#### Option A: DAP (Debug Adapter Protocol) via bashdb
1. **Install bashdb**: Available via package managers (`apt install bashdb`, `brew install bashdb`)
2. **Create a DAP adapter**: Either:
   - Use the existing [Bash Debug](https://marketplace.visualstudio.com/items?itemName=rogalmic.bash-debug) extension (wraps bashdb)
   - Write a custom DAP adapter in Rust/TypeScript that understands argsh patterns
3. **argsh-specific enhancements**:
   - Auto-set breakpoints on `:args`/`:usage` calls
   - Show the current command path in the call stack (e.g., `main → deploy → main::deploy`)
   - Variable inspection that understands `args` and `usage` arrays
   - Conditional breakpoints on flag values (e.g., break when `--verbose` is set)

#### Option B: Built-in trace mode
1. Add an `argsh debug` command that wraps script execution with `PS4` and `set -x`
2. Parse the trace output and show it in a VSCode panel
3. Less powerful than bashdb but zero dependencies

#### Launch configuration
```json
{
    "type": "argsh",
    "request": "launch",
    "name": "Debug argsh script",
    "program": "${file}",
    "args": ["deploy", "--env", "staging"],
    "cwd": "${workspaceFolder}"
}
```

### Estimated effort
- Option A (bashdb wrapper): 2-3 weeks
- Option B (trace mode): 1 week
- argsh-specific enhancements: 1-2 additional weeks

### Prerequisites
- bashdb installed on the system
- Bash 4.3+ (for `PS4` and `BASH_XTRACEFD` support)

---

## Semantic Tokens

### What
Provide LSP semantic tokens for precise, context-aware highlighting that goes beyond TextMate grammar patterns.

### Why
TextMate grammar injection into `source.shell` has limitations — it can't reliably highlight inside strings or distinguish argsh-specific constructs from regular bash. Semantic tokens from the LSP override TextMate with precise scopes based on actual analysis.

### How to Implement
1. Register `semanticTokensProvider` in server capabilities
2. For each analyzed function, emit tokens for:
   - Field specs inside `args=()` arrays (type: `parameter`, modifiers: `declaration`)
   - Descriptions (type: `string`, modifiers: `documentation`)
   - Command names in `usage=()` (type: `function`, modifiers: `declaration`)
   - Annotations `@readonly` etc. (type: `decorator`)
   - Type modifiers `:~int` (type: `type`)
3. Map to VSCode's semantic token types/modifiers

### Estimated effort
2-3 days for basic tokens, 1 week for full coverage.
