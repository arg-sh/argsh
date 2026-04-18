# Crates

This directory contains the Rust crates that power argsh's tooling:

| Crate | Purpose | Binaries |
|-------|---------|----------|
| [argsh-syntax](argsh-syntax.md) | Parsing library — field definitions, usage entries, document analysis | (library only) |
| [argsh-lsp](argsh-lsp.md) | Language server, CLI linter, and DAP debugger | `argsh-lsp`, `argsh-lint`, `argsh-dap` |

## Architecture

```
argsh-syntax (library)
    │
    ├── document analysis (functions, args, usage, imports)
    ├── field parsing (modifiers, types, flags vs positionals)
    ├── usage parsing (commands, aliases, annotations)
    └── scope analysis (variable declarations, shadowing)
         │
         ▼
argsh-lsp (library + 3 binaries)
    │
    ├── lib.rs ──── shared modules ────┐
    │   ├── diagnostics.rs             │ reused by all 3 binaries
    │   └── resolver.rs                │
    │                                  │
    ├── argsh-lsp (bin) ◄──────────────┤ Language Server Protocol
    │   ├── backend.rs                 │   (tower-lsp, stdio)
    │   ├── completion.rs              │
    │   ├── hover.rs                   │
    │   ├── goto_def.rs                │
    │   ├── format.rs                  │
    │   ├── codelens.rs                │
    │   ├── preview.rs                 │
    │   ├── rename.rs                  │
    │   ├── symbols.rs                 │
    │   └── util.rs                    │
    │                                  │
    ├── argsh-lint (bin) ◄─────────────┤ CLI linter
    │   └── shellcheck-compatible      │   (gcc/json/checkstyle output)
    │       flags & formats            │
    │                                  │
    └── argsh-dap (bin) ◄──────────────┘ Debug Adapter Protocol
        └── bash DEBUG trap                (breakpoints, stepping,
            + FIFO protocol                 variable inspection)
```

## Building

```bash
# Build all crates
cd crates/argsh-lsp && cargo build --release

# This produces 3 binaries:
# target/release/argsh-lsp   — language server
# target/release/argsh-lint  — CLI linter
# target/release/argsh-dap   — DAP debugger

# argsh-syntax is built automatically as a dependency
```

## Testing

```bash
# All tests (syntax + LSP + lint CLI + DAP integration + DAP E2E)
cargo test --release

# Individual test suites
cargo test --release --test integration    # LSP protocol tests
cargo test --release --test lint_cli       # argsh-lint CLI tests
cargo test --release --test dap_integration # DAP protocol tests
cargo test --release --test dap_e2e        # DAP end-to-end tests
```
