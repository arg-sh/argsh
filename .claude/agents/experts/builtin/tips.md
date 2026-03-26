# Builtin — Operational Tips

## Build & Test

- Build: `cargo build --release --manifest-path builtin/Cargo.toml`
- Test: `cargo test --manifest-path builtin/Cargo.toml`
- Lint: `cargo clippy --manifest-path builtin/Cargo.toml`

## Key Paths

- Source: `builtin/src/`
- Entry: `builtin/src/lib.rs`
- Args parsing: `builtin/src/args.rs`
- Import system: `builtin/src/import.rs`
- Usage/completion/MCP: `builtin/src/usage/`
- Config: `builtin/Cargo.toml`

## Gotchas

- Crate type is `cdylib` — produces a loadable `.so` for Bash `enable -f`
- Depends on `bash-builtins` crate for Bash integration
- Export names contain colons (e.g. `:args_struct`) — requires `lld` linker on arm64
- RUSTFLAGS must include `-C link-arg=-fuse-ld=lld` for cross-platform builds
- (Add operational gotchas as you discover them)
