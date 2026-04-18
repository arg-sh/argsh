# shdoc

Documentation generator for bash scripts and Rust source files. Extracts `# @annotation` tags from bash and `///` doc comments from Rust, producing Markdown, HTML, or JSON output.

## Usage

```bash
# stdin mode (backward compatible)
shdoc < file.sh

# File mode (batch processing)
shdoc -o docs/ libraries/*.sh builtin/src/*.rs

# With prefix template
shdoc -o docs/ -p _prefix.mdx libraries/*.sh

# Output formats
shdoc -f markdown < file.sh   # default
shdoc -f html < file.sh
shdoc -f json < file.sh

# Filter by tags
shdoc --filter core libraries/*.sh
shdoc --filter '!deprecated' libraries/*.sh
```

## Annotations

### File-level

```bash
# @file module-name
# @brief Short description
# @description
#   Multi-line description text.
# @tags core, validation
```

### Function-level

```bash
# @description Convert a value to an integer
# @arg $1 any value to convert
# @option -s | --strict Strict mode
# @stdout The converted integer
# @stderr Error message on failure
# @exitcode 0 Success
# @exitcode 1 Not a valid integer
# @example
#   to::int "42"    # 42
#   to::int "abc"   # error
# @set RESULT The conversion result
# @see to::float
# @internal
# @tags core
to::int() { ... }
```

### Rust doc comments

```rust
//! Module description (file-level)
//! Mirrors: libraries/to.sh

/// Function description
#[export_name = "to::int_struct"]
pub static mut TO_INT_STRUCT: BashBuiltin = ...;
```

## Cross-language Merge

When bash and Rust files define functions with the same canonical name, shdoc merges them into a single documentation entry. Bash annotations take priority.

```bash
# Input: libraries/to.sh + builtin/src/to.rs → docs/to.mdx
shdoc -o docs/ libraries/to.sh builtin/src/to.rs
```

## Output

Markdown output includes:
- YAML frontmatter (from `@tags`)
- Module description
- Function index
- Per-function sections: description, badges (`Bash`/`Rust`), examples, options, arguments, exit codes, see-also

## Build

```bash
cargo build --release
# Binary: target/release/shdoc
```

## Test

```bash
cargo test

# Fixture-based tests compare output against .expected.md files
```

## Architecture

```
src/
├── main.rs            CLI, file I/O, filtering, mode dispatch
├── model.rs           Data model (Document, FunctionDoc, ArgEntry)
├── toc.rs             Table-of-contents and anchor generation
├── parser/
│   ├── mod.rs         Parser dispatch by file extension
│   ├── bash.rs        State machine for # @annotation tags
│   ├── rust.rs        Doc comment parser for .rs files
│   └── merge.rs       Cross-language merge by canonical function name
└── render/
    ├── mod.rs         Renderer trait and factory
    ├── markdown.rs    GitHub-flavored Markdown (mdx)
    ├── html.rs        Standalone HTML pages
    └── json.rs        Structured JSON output
```
