# argsh-lsp

Library and three binaries for argsh tooling: language server, CLI linter, and DAP debugger. Built on top of `argsh-syntax` for all analysis.

## Binaries

### `argsh-lsp` — Language Server

Full LSP implementation over stdio using `tower-lsp`. Activates on `shellscript` files containing `source argsh`, `#!/usr/bin/env argsh`, or any function that calls `:args`/`:usage`.

Features:

| Feature | Module | Description |
| ------- | ------ | ----------- |
| Diagnostics | `diagnostics.rs` | AG001–AG013 checks (see below) |
| Completions | `completion.rs` | Modifiers, types, annotations, function names, library functions |
| Hover | `hover.rs` | Generated `--help` output, args/usage entry details, subcommand help |
| Go to definition | `goto_def.rs` | Usage entries, `:-` mappings, `:~custom` types, imports |
| Code lens | `codelens.rs` | Branch/leaf icons with flag/subcommand counts |
| Formatting | `format.rs` | Aligns `args=()`/`usage=()` array entries |
| Preview | `preview.rs` | HTML dashboard with command tree, MCP tools, export links |
| Rename | `rename.rs` | Rename function across definition, usage entries, and references |
| Symbols | `symbols.rs` | Document outline with namespace nesting |
| Cross-file | `resolver.rs` | Follows `import` across files plus special-case `source argsh` (configurable depth) |

### `argsh-lint` — CLI Linter

Standalone linter that reuses the same diagnostics engine as the LSP. Designed for CI pipelines and editor-less workflows.

```bash
# Lint a single file
argsh-lint script.sh

# Lint multiple files
argsh-lint lib/*.sh

# Output formats (shellcheck-compatible flags)
argsh-lint -f gcc script.sh        # default: file:line:col: level: message [CODE]
argsh-lint -f tty script.sh        # colorized output with ANSI codes
argsh-lint -f json script.sh       # JSON object per line
argsh-lint -f checkstyle script.sh # Checkstyle XML
argsh-lint -f quiet script.sh      # no output, exit code only

# Filter by severity
argsh-lint -S warning script.sh    # warnings and above
argsh-lint -S error script.sh      # errors only

# Exclude / include specific codes
argsh-lint -e AG004,AG010 script.sh
argsh-lint -i AG001,AG003 script.sh  # only these codes

# Skip cross-file resolution (faster, skips AG013)
argsh-lint --no-resolve script.sh

# Colorize control
argsh-lint -C always script.sh     # auto (default), always, never
```

Exit codes: `0` = no issues, `1` = issues found, `2` = CLI/IO error.

### `argsh-dap` — Debug Adapter Protocol

Step-through bash debugger using bash's built-in `DEBUG` trap (no external dependencies like bashdb).

```bash
# Start DAP server (communicates over stdio, same framing as LSP)
argsh-dap
```

Architecture:

```
VSCode ←→ argsh-dap (Rust, DAP over stdio)
               ↕
         Named FIFOs (per-PID)
               ↕
         bash process (DEBUG trap + FIFO protocol)
```

Features:
- Breakpoints: file:line, conditional (`(( i == 3 ))`), by subcommand name (smart breakpoints)
- Stepping: step in (into functions), step over, step out
- Call stack: full `FUNCNAME`/`BASH_SOURCE` trace with argsh namespace resolution
- Variable inspection: argsh Args scope shows `:args` field definitions with types
- Watch expressions and set-variable-at-runtime
- Subshell/pipe/background job support (per-PID control FIFOs + flock serialization)
- Auto-launch configs: suggests debug configurations based on script analysis

## Shared Library (`lib.rs`)

The `argsh-lsp` crate re-exports two modules used by all three binaries:

```rust
pub mod diagnostics;  // AG001–AG013 diagnostic generation
pub mod resolver;     // Cross-file import resolution
```

## Diagnostics

| Code | Severity | Description |
| ---- | -------- | ----------- |
| AG001 | Error | `args` entry missing description |
| AG002 | Error | `usage` entry missing description |
| AG003 | Error | Invalid field spec (bad modifier) |
| AG004 | Warning | Missing `local` variable declaration |
| AG005 | Error | `args` array declared but `:args` not called |
| AG006 | Error | `usage` array declared but `:usage` not called |
| AG007 | Warning | Usage target function not found |
| AG008 | Warning | Duplicate flag name (suppressed when `:^` involved) |
| AG009 | Warning | Duplicate short alias (suppressed when `:^` on same field name) |
| AG010 | Warning | Command resolves to bare function (not namespaced) |
| ~~AG011~~ | — | *(removed)* Trailing `\|` is valid syntax for long-only flags |
| AG012 | Hint | Local variable shadows parent scope args field |
| AG013 | Warning | Import could not be resolved |
| AG014 | Warning | `:^` field without `${var:-...}` default (won't inherit parent value) |
| AG015 | Warning | `# argsh source=` path does not exist or is not a directory |

Suppress per-line with `# argsh-ignore=AG004,AG012` or `# argsh disable=AG004`.

## Resolver

`resolver.rs` follows `import` statements across files to build a merged `DocumentAnalysis`, and handles the special case `source argsh` by loading `main.sh`/`args.sh`. Resolution depth is configurable (0+, default 2).

Search order for imports:
1. Relative to the importing file's directory
2. Standard library paths (`libraries/`)
3. Project root (detected by `.git`, `.envrc`, or `.bin/argsh`)

Import prefix fallbacks:
- `@` → `PATH_BASE` env var → project root (`.git`, `.envrc`, or `.bin/argsh`)
- `^` → `PATH_SCRIPTS` env var → `# argsh source=` directive → walk up from script dir
- `~` → relative to the importing file

## Source Layout

```
src/
├── lib.rs             Shared modules (diagnostics, resolver)
├── main.rs            argsh-lsp binary entry point
├── backend.rs         LSP server implementation (tower-lsp Backend trait)
├── completion.rs      Completions (modifiers, types, annotations, functions)
├── diagnostics.rs     AG001–AG013 diagnostic generation
├── hover.rs           Hover info (help text, args/usage details)
├── goto_def.rs        Go-to-definition (usage entries, imports, types)
├── codelens.rs        Code lens (flag/subcommand counts)
├── format.rs          Array entry alignment formatter
├── preview.rs         HTML preview generation (script dashboard)
├── rename.rs          Rename support (functions across refs)
├── symbols.rs         Document symbols (outline with namespaces)
├── resolver.rs        Cross-file import resolution
├── util.rs            Word extraction helper
└── bin/
    ├── argsh-lint.rs  CLI linter binary
    └── argsh-dap.rs   DAP debugger binary

tests/
├── integration.rs     LSP protocol tests
├── lint_cli.rs        argsh-lint CLI tests
├── dap_integration.rs DAP protocol tests
└── dap_e2e.rs         DAP end-to-end tests
```

## Dependencies

- `tower-lsp` — LSP server framework
- `tokio` — async runtime
- `serde` / `serde_json` — DAP message serialization
- `argsh-syntax` — parsing library (path dependency)
- `dashmap` — concurrent document cache
- `regex` — pattern matching
- `tempfile` — temporary file handling
- `libc` — platform bindings (unix only)
