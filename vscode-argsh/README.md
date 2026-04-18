<p align="center">
  <img src="icon.png" width="64" alt="argsh logo"/>
</p>

# argsh for Visual Studio Code

Language support, linting, and debugging for [argsh](https://arg.sh) — the structured Bash scripting framework.

## Features

### Language Server

- **Syntax highlighting** — `:args`, `:usage`, modifiers (`:+`, `:~int`, `:!`), `@` annotations, `::` namespaces, `import`
- **Diagnostics** (AG001–AG013) — missing variables, invalid modifiers, unpaired entries, duplicate flags, unresolved commands/imports
- **Suppressible** — `# argsh disable=AG004` (like shellcheck)
- **Completions** — modifiers, types (built-in + custom `to::`), annotations, function names, library functions (`is::`, `to::`, `string::`, etc.)
- **Help preview** — hover over functions to see generated `--help` output with flags table
- **Hover on args/usage** — hover the keyword to see all defined entries
- **Hover on subcommands** — see target function's full help with flags
- **Code lens** — branch/leaf icons with flag/subcommand counts and parent link
- **Script preview** — dashboard with command tree, MCP tools, export links (Ctrl+Shift+A)
- **Go to definition** — Ctrl+Click on usage entries, `:-` mappings, `:~custom` types, imports
- **Cross-file resolution** — follows `import` and `source argsh` across files
- **Auto formatter** — aligns args/usage array entries (format on save)
- **Command tree panel** — bottom panel with function hierarchy and active function highlighting
- **Document outline** — function hierarchy with namespace nesting
- **Snippets** — `argsh-main`, `argsh-func`, `argsh-args`, `argsh-flag-*`, `argsh-import`
- **Export** — MCP JSON, YAML, JSON via command palette

### Debugger

Step-through debugging with no external dependencies (uses bash's built-in `DEBUG` trap):

- **Breakpoints** — file:line, conditional (`(( i == 3 ))`), and by subcommand name
- **Stepping** — step in (F11), step over (F10), step out (Shift+F11)
- **Call stack** — full `FUNCNAME`/`BASH_SOURCE` trace with argsh namespace resolution
- **Variable inspection** — argsh Args inspector shows `:args` field definitions with types
- **Watch expressions** and **set variable at runtime**
- **Subshell support** — `$()`, pipes, `{ ...; } &` don't deadlock

Press **F5** to start debugging the current script, or create a `launch.json`:

```json
{
  "type": "argsh",
  "request": "launch",
  "name": "Debug script",
  "program": "${file}",
  "args": ["deploy", "--env", "staging"],
  "stopOnEntry": true
}
```

### Linter

The extension bundles `argsh-lint` for static analysis of argsh-specific patterns (AG001–AG013). Works alongside shellcheck — argsh handles framework-specific validation, shellcheck handles general bash.

## Commands

| Command | Shortcut | Description |
|---------|----------|-------------|
| Show Script Preview | Ctrl+Shift+A | Open the script dashboard |
| Show Help for Current Function | — | Show help at cursor |
| Format argsh Arrays | Shift+Alt+F | Align array entries |
| Validate Script | — | Force re-validation |
| Export MCP JSON | — | MCP tool schema |
| Export YAML | — | Docgen YAML output |
| Export JSON | — | Docgen JSON output |
| Restart Language Server | — | Restart the LSP |

## Installation

### From Source

```bash
# Build all binaries (LSP, linter, debugger)
cargo build --release --manifest-path crates/argsh-lsp/Cargo.toml

# Set up for the extension
mkdir -p vscode-argsh/bin
cp crates/argsh-lsp/target/release/argsh-lsp vscode-argsh/bin/
cp crates/argsh-lsp/target/release/argsh-dap vscode-argsh/bin/

# Install extension
cd vscode-argsh && npm install && npm run compile
```

### Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `argsh.lsp.enabled` | `true` | Enable the language server |
| `argsh.lsp.path` | `""` | Path to `argsh-lsp` binary |
| `argsh.dap.path` | `""` | Path to `argsh-dap` binary |
| `argsh.commandTree.enabled` | `true` | Show command tree panel |
| `argsh.codeLens.enabled` | `true` | Show counts above functions |
| `argsh.formatOnSave` | `true` | Auto-format on save |
| `argsh.resolveDepth` | `2` | Cross-file import depth (0–5) |

## Requirements

- [argsh](https://arg.sh) scripts (detected by `source argsh` or `#!/usr/bin/env argsh`)
- Rust toolchain (to build from source)
- Node.js 20+ (to build the extension)
- **Linux or macOS** (Windows is not currently supported)
