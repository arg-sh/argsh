# minifier

A general-purpose bash script minifier with optional source bundling and variable obfuscation.

## Usage

```
minifier -i <INPUT> -o <OUTPUT> [OPTIONS]

Options:
  -i <file>        Input bash script
  -o <file>        Output file
  -B, --bundle     Enable source bundling (resolve and inline imports)
  -S, --search-path <dir>  Search directory for resolving imports (repeatable), requires -B
  -O, --obfuscate  Enable variable name obfuscation
  -V <pattern>     Exclude variables matching pattern from obfuscation (repeatable), requires -O
  -I <patterns>    Ignore variables matching regex (comma-separated, default: "usage,args"), requires -O
  -h, --help       Print help
```

### Minify only

```bash
minifier -i script.sh -o script.min.sh
```

### Bundle + minify

```bash
minifier -i script.sh -o script.min.sh -B -S ./libraries
```

### Minify + obfuscate

```bash
minifier -i script.sh -o script.min.sh -O
```

### Bundle + obfuscate (single executable)

```bash
minifier -i script.sh -o script.min.sh -B -S ./libraries -O
```

### Exclude variables from obfuscation

```bash
minifier -i script.sh -o script.min.sh -O -V MYVAR -V CONFIG_ -I "usage,args"
```

## Pipeline

The minifier processes input through up to 5 phases:

```
Input → Bundle (if -B) → Strip → Flatten → Obfuscate (if -O) → Join → Output
```

### 1. Bundle (optional, `-B`)

Resolves `import`, `source`, and `.` (dot-source) statements, recursively inlining
referenced files to produce a single self-contained bash script.

#### Import patterns detected

| Pattern | Example |
|---------|---------|
| `import <target>` | `import fmt`, `import @core/utils` |
| `source <path>` | `source ./lib.sh` |
| `. <path>` | `. ./helper.sh` |

Lines with `$` variable expansions in the path (e.g. `source "${CONFIG}"`) are left as-is.

#### Path resolution

For each import target (after stripping `@`/`~` prefix):

1. Relative to the directory of the file containing the import
2. Each `--search-path` directory, in CLI order
3. For each candidate, tries: as-is, `.sh`, `.bash`

Unresolvable imports are left as-is (strip phase removes `import` lines later).

#### Dedup rules

| Context                                 | Behavior                              |
|-----------------------------------------|---------------------------------------|
| Top-level (brace depth == 0)            | Dedup: skip if file already inlined   |
| Inside function body (brace depth > 0)  | Always inline — content is scoped     |
| `# minifier force source` annotation    | Always inline regardless              |

```bash
# Top-level: inlined once, second import is deduped
import shared
import shared    # skipped

# Inside function: always inlined (scoped)
foo() {
  import shared  # inlined again
}

# Force annotation: overrides dedup at top level
# minifier force source
import shared    # inlined even if already seen
```

### 2. Strip

Removes entire lines that are unnecessary in minified output:

- Full-line comments (including shebangs)
- Blank / whitespace-only lines
- `import <word>` calls and single-line `import() { ... }` definitions
- `set -euo pipefail`
- Mid-line shebangs from file concatenation (`}#!/usr/bin/env bash` → split)

### 3. Flatten

Per-line cleanup:

- Remove leading whitespace (indentation)
- Remove trailing standalone `;` (but not `;;`)
- Remove end-of-line comments (`# ...` not inside quotes)

### 4. Obfuscate (optional, `-O`)

Discovers local variables from 8 patterns:

| Pattern | Example |
|---------|---------|
| Assignment | `var=value` |
| Local | `local var` / `local -a var` |
| Read | `read -r var` |
| For | `for var in ...` |
| Array | `var[0]=value` |
| Pre-increment | `(( ++var ))` |
| Post-increment | `(( var++ ))` |
| Declare (skipped) | `declare` on same line suppresses discovery |

Renames variables using 21 substitution rules across assignments, `$var`, `${var}`,
parameter expansions, arithmetic contexts, arrays, and more — while respecting
single-quote boundaries.

#### Annotations

Place `# obfus ignore variable` on the line **before** a declaration to exclude
it from obfuscation:

```bash
# obfus ignore variable
local keep_this_name="important"
```

### 5. Join

Aggressively joins newlines into single-line output while preserving:

- Heredoc content (verbatim between `<<DELIM` ... `DELIM`)
- Case statement structure (bash requires newline after `in`)
- Multi-line arrays, quoted strings, backslash continuations

Fixes `then;`/`do;`/`else;` → `then `/`do `/`else ` (improvement over Perl predecessor).

## Build

```bash
cargo build --release
# Binary: target/release/minifier
```

## Test

```bash
# Unit + integration tests
cargo test

# Code coverage (requires llvm-tools: rustup component add llvm-tools)
# Run via the project helper:
#   argsh coverage minifier
```

## Architecture

```
src/
├── main.rs        CLI + pipeline orchestration
├── bundle.rs      Source-file bundling (recursive import inlining)
├── strip.rs       Line-level stripping (comments, blanks, imports)
├── flatten.rs     Per-line cleanup (indentation, trailing ;, EOL comments)
├── discover.rs    Variable discovery from bash source (8 patterns)
├── obfuscate.rs   Variable renaming (21 substitution rules per variable)
├── join.rs        Aggressive newline joining (heredoc/case/array/quote-aware)
└── quote.rs       Quote-tracking for multi-line string detection
```
