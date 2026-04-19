# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.8.x   | :white_check_mark: |
| < 0.8   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability in argsh, please report it responsibly.

**Do not open a public issue.**

Instead, use one of these methods:

1. **GitHub Security Advisories** (preferred): [Report a vulnerability](https://github.com/arg-sh/argsh/security/advisories/new)
2. **Email**: Send details to the maintainers listed in the repository

Please include:

- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Impact assessment (if known)

We aim to acknowledge reports within 48 hours and provide a fix or mitigation within 7 days for critical issues.

## Scope

The following components are in scope:

| Component | Description |
| --------- | ----------- |
| `libraries/*.sh` | Core bash libraries (args parsing, imports, string utilities) |
| `builtin/` | Rust loadable builtins (`.so` loaded via `enable -f`) |
| `crates/argsh-lsp` | Language server, linter, and DAP debugger |
| `minifier/` | Script minification and obfuscation |
| `.bin/argsh` | Entrypoint and Docker delegation |

## Security Considerations

### Shell Injection

argsh parses user-provided command-line arguments. The `:args` and `:usage` builtins use `local -n` (bash namerefs) to assign values to variables. Field specs are validated before use. However, scripts that pass unsanitized user input to `eval`, `source`, or command substitution are the script author's responsibility.

### Loadable Builtins (.so)

The `argsh.so` shared library is loaded into bash via `enable -f`. It has full access to the bash process. Only load `.so` files from trusted sources. The `argsh builtin update` command downloads from GitHub Releases over HTTPS and verifies the asset matches the expected release tag.

**Safety rule**: Never overwrite a loaded `.so` in place. The update command downloads to a temp file and does an atomic `mv` to prevent segfaults.

### Import Resolution

The `import` system resolves module paths using configurable prefixes (`@`, `^`, `~`). Scripts should not import from untrusted or user-controlled paths. The import cache prevents double-loading but does not verify file integrity.

### Docker Delegation

When argsh delegates commands to Docker (minify, test, lint, coverage), it mounts the project directory into the container. Environment variables matching `ARGSH_ENV_*` are forwarded. Avoid storing secrets in `ARGSH_ENV_*` variables.

### MCP Server

The `:usage::mcp` builtin exposes script commands as MCP tools for AI agent integration. It runs commands in the current shell context with full permissions. Only enable MCP on scripts you trust, and only expose it to trusted MCP clients.
