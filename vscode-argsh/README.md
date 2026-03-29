<p align="center">
  <img src="icon.png" width="64" alt="argsh logo"/>
</p>

# argsh for Visual Studio Code

Language support for [argsh](https://arg.sh) — the structured Bash scripting framework.

## Features

- **Syntax highlighting** — `:args`, `:usage`, modifiers (`:+`, `:~int`, `:!`), `@` annotations, `::` namespaces, `import`
- **Diagnostics** (AG001–AG010) — missing variables, invalid modifiers, unpaired entries, duplicate flags, unresolved commands
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
# Build the LSP binary
cargo build --release --manifest-path crates/argsh-lsp/Cargo.toml

# Set up for the extension
mkdir -p vscode-argsh/bin
cp crates/argsh-lsp/target/release/argsh-lsp vscode-argsh/bin/

# Install extension
cd vscode-argsh && npm install && npm run compile
```

### Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `argsh.lsp.enabled` | `true` | Enable the language server |
| `argsh.lsp.path` | `""` | Path to `argsh-lsp` binary |
| `argsh.commandTree.enabled` | `true` | Show command tree panel |
| `argsh.codeLens.enabled` | `true` | Show counts above functions |
| `argsh.formatOnSave` | `true` | Auto-format on save |
| `argsh.resolveDepth` | `2` | Cross-file import depth (0–5) |

## Requirements

- [argsh](https://arg.sh) scripts (detected by `source argsh` or `#!/usr/bin/env argsh`)
- Rust toolchain (to build `argsh-lsp`)
- Node.js 20+ (to build the extension)
- **Linux or macOS** (Windows is not currently supported — argsh and bash scripts are primarily used on Unix systems)
