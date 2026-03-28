# argsh for Visual Studio Code

Language support for [argsh](https://arg.sh) -- the structured Bash scripting framework.

## Features

- **Syntax highlighting** for `:args`, `:usage`, field modifiers, annotations, namespaces
- **Code snippets** for common argsh patterns (functions, args arrays, usage arrays)
- **Language server** with diagnostics, completions, and go-to-definition (requires `argsh-lsp`)

## Installation

Install from the VSCode marketplace or build from source:

```bash
cd vscode-argsh
npm install
npm run compile
```

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `argsh.lsp.enabled` | `true` | Enable the argsh language server |
| `argsh.lsp.path` | `""` | Path to `argsh-lsp` binary (auto-detected if empty) |
