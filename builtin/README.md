# builtin

Bash loadable builtins implemented in Rust. Provides native-speed implementations of argsh's core functions, loaded at runtime via `enable -f`.

## Builtins

| Builtin | Description |
| ------- | ----------- |
| `:args` | Argument parsing with type checking and flag validation |
| `args::field_name` | Extract variable name from field spec (`'flag\|f:~int!'` → `flag`) |
| `:usage` | Subcommand dispatch from usage array |
| `:usage::help` | Format and display help text |
| `:usage::completion` | Generate bash/zsh/fish shell completions |
| `:usage::docgen` | Generate docs (man, md, rst, yaml, llm formats) |
| `:usage::mcp` | MCP server — expose commands as AI-callable tools |
| `import` | Module loading with caching and selective function aliasing |
| `import::clear` | Clear import cache for re-sourcing |
| `is::array` | Test if variable is an array |
| `is::set` | Test if variable is set |
| `is::uninitialized` | Test if variable is uninitialized |
| `is::tty` | Test if stdin is a TTY |
| `to::int` | Validate integer |
| `to::float` | Validate float |
| `to::boolean` | Validate boolean |
| `to::file` | Validate file path |
| `to::string` | Validate string |

All builtins have pure-bash fallbacks in `libraries/`. The `.so` is optional — argsh works without it, just slower.

## Build

```bash
cargo build --release
# Output: target/release/libargsh.so
```

Build profile optimizes for size: LTO, `opt-level = "s"`, stripped symbols, single codegen unit.

## Loading

Cargo produces `target/release/libargsh.so`. At runtime, argsh searches for `argsh.so` (without the `lib` prefix) in this order:

1. `ARGSH_BUILTIN_PATH` (explicit path to the `.so` file)
2. `PATH_LIB/argsh.so`
3. `PATH_BIN/argsh.so`
4. `LD_LIBRARY_PATH` directories
5. `BASH_LOADABLES_PATH` directories

The default install target is `~/.local/lib/bash/argsh.so` — found via `BASH_LOADABLES_PATH` if configured.

Once found, all 18 builtins are registered via `enable -f <path> <name> ...`.

## Architecture

```
src/
├── lib.rs          FFI types (WordList, WordDesc, BashBuiltin), word list parsing
├── shell.rs        FFI bindings to bash internals (find_variable, parse_and_execute)
├── args.rs         :args builtin — argument parsing with type checking
├── field.rs        args::field_name — field definition parser
├── shared.rs       Shared error handling and flag parsing for :args/:usage
├── is.rs           is::* builtins — variable introspection
├── to.rs           to::* builtins — type validation/conversion
├── import.rs       import/import::clear — module loading with caching
└── usage/
    ├── mod.rs          :usage/:usage::help — subcommand dispatch and help
    ├── completion.rs   :usage::completion — shell completion generation
    ├── docgen.rs       :usage::docgen — documentation generation
    └── mcp.rs          :usage::mcp — MCP server for AI agent integration
```

## Testing

```bash
# Unit + integration tests
cargo test

# Test with builtin loaded (in argsh test suite)
ARGSH_BUILTIN_TEST=1 bats tests/

# Test pure-bash fallbacks (skips builtin)
ARGSH_PURE_BASH_TEST=1 bats tests/
```

## Safety

Never overwrite a loaded `.so` in place — bash will segfault. The `argsh builtin update` command downloads to a temp file and does an atomic `mv`.
