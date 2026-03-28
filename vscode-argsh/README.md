# argsh for Visual Studio Code

Language support for [argsh](https://arg.sh) -- the structured Bash scripting framework.

## Features

- **Syntax highlighting** -- `:args`, `:usage`, modifiers, `@` annotations, `::` namespaces, `import`
- **Diagnostics** -- missing variables, invalid modifiers, unpaired entries, duplicate flags
- **Completions** -- modifiers, types, annotations, function names, import modules
- **Help preview** -- hover over functions to see generated `--help` output
- **Code lens** -- flag/subcommand counts above functions
- **Script preview** -- dashboard with command tree, MCP schema, docgen export
- **Go to definition** -- Ctrl+Click from usage entries to function definitions
- **Document outline** -- function hierarchy with namespace nesting
- **Snippets** -- `argsh-main`, `argsh-func`, `argsh-args`, `argsh-flag-*`, `argsh-import`

## Commands

| Command | Shortcut | Description |
|---------|----------|-------------|
| argsh: Show Script Preview | Ctrl+Shift+A | Open the script dashboard |
| argsh: Show Help for Current Function | -- | Show help at cursor |
| argsh: Validate Script | -- | Force re-validation |
| argsh: Restart Language Server | -- | Restart the LSP |

## Installation

### From Source

```bash
# Build the LSP binary
cd crates/argsh-lsp && cargo build --release

# Symlink into extension
mkdir -p vscode-argsh/bin
ln -sf $(pwd)/crates/argsh-lsp/target/release/argsh-lsp vscode-argsh/bin/argsh-lsp

# Install extension
cd vscode-argsh && npm install && npm run compile
```

### Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `argsh.lsp.enabled` | `true` | Enable the language server |
| `argsh.lsp.path` | `""` | Path to `argsh-lsp` binary |

## Requirements

- [argsh](https://arg.sh) scripts (detected by `source argsh` or `#!/usr/bin/env argsh`)
- Rust toolchain (to build `argsh-lsp`)
- Node.js 20+ (to build the extension)
