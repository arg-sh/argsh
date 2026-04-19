# argsh-syntax

Pure Rust parsing library for argsh scripts. Provides static analysis of bash files without executing them — extracts function definitions, `:args`/`:usage` declarations, field specifications, import statements, and variable scopes.

Used by the `argsh-lsp` crate (which produces the `argsh-lsp`, `argsh-lint`, and `argsh-dap` binaries) as the shared analysis foundation.

## Modules

### `document` — Document Analysis

The core module. `analyze(content: &str) -> DocumentAnalysis` parses a bash script and extracts:

- **Functions** — name, line range, whether it calls `:args` or `:usage`
- **Args entries** — field specs (`'name|n:!'`), descriptions, parsed field definitions
- **Usage entries** — command names, aliases, explicit function mappings (`:-func`), annotations (`@readonly`)
- **Imports** — `import` statements with module names
- **Argsh detection** — detects `#!/usr/bin/env argsh` shebang and `source argsh` markers

```rust
use argsh_syntax::document::analyze;

let analysis = analyze(r#"
main() {
  local name
  local -a args=(
    'name|n:!' "Name of the person"
  )
  :args "Greet someone" "${@}"
}
"#);

assert_eq!(analysis.functions.len(), 1);
assert_eq!(analysis.functions[0].name, "main");
assert!(analysis.functions[0].calls_args);
assert_eq!(analysis.functions[0].args_entries.len(), 1);
assert_eq!(analysis.functions[0].args_entries[0].spec, "name|n:!");
```

Key types:

| Type | Description |
|------|-------------|
| `DocumentAnalysis` | Top-level result: functions, imports, shebang flags |
| `FunctionInfo` | A function with its args/usage entries, line range, calls_args/calls_usage flags |
| `ArgsArrayEntry` | A single entry from the `args=()` array: spec, description, parsed FieldDef |
| `UsageEntry` | A single entry from the `usage=()` array: name, aliases, annotations |

### `field` — Field Specification Parser

Parses argsh field specs like `'name|n:~int!'` into structured definitions.

```rust
use argsh_syntax::field::parse_field;

let field = parse_field("port|p:~int!").unwrap();
assert_eq!(field.name, "port");
assert_eq!(field.short, Some("p".to_string()));
assert_eq!(field.type_name, "int");
assert!(field.required);
assert!(!field.is_boolean);
assert!(!field.is_positional); // has | separator → it's a flag
```

| Field | Meaning |
|-------|---------|
| `name` | Variable name (before `\|`) |
| `short` | Short alias (after `\|`, single char) |
| `type_name` | Type constraint after `:~` (int, float, file, etc.) |
| `is_boolean` | `:+` modifier — flag takes no value, sets to 1 |
| `required` | `:!` modifier — must be provided |
| `is_positional` | No `\|` in definition — positional parameter (not a `--flag`) |
| `is_inherited` | `:^` modifier — yields to non-`:^` duplicates (for parent/child inheritance) |
| `hidden` | `#` prefix — excluded from help text |
| `display_name` | Original name preserving dashes (vs `name` which replaces `-` with `_`) |
| `raw` | Original spec string, preserved for diagnostics |

### `usage` — Usage Entry Parser

Parses `:usage` array entries like `'deploy|d@destructive'`.

```rust
use argsh_syntax::usage::parse_usage_entry;

let entry = parse_usage_entry("deploy|d@destructive");
assert_eq!(entry.name, "deploy");
assert_eq!(entry.aliases, vec!["deploy", "d"]);
assert_eq!(entry.annotations, vec!["destructive"]);
assert!(!entry.hidden);
assert!(entry.explicit_func.is_none());
```

Special syntax:
- `cmd|alias` — command with alias
- `cmd:-func` — explicit function mapping (bypasses namespace resolution)
- `cmd@annotation` — annotations (readonly, destructive, json, etc.)
- `#cmd` — hidden from help text
- `-` — group separator

### `scope` — Variable Scope Analysis

Tracks `local` variable declarations per function for diagnostics like AG004 (missing local declaration) and AG012 (variable shadows parent scope).

## No Dependencies

`argsh-syntax` depends only on `regex` — no async runtime, no LSP types, no I/O. This makes it suitable for embedding in any Rust tool that needs to understand argsh scripts.
