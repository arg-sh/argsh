# Minifier — Operational Tips

## Build & Test

- Build: `cargo build --release --manifest-path minifier/Cargo.toml`
- Test: `cargo test --manifest-path minifier/Cargo.toml`
- Lint: `cargo clippy --manifest-path minifier/Cargo.toml`

## Key Paths

- Source: `minifier/src/`
- Entry: `minifier/src/main.rs`
- Modules: bundle, discover, flatten, join, obfuscate, quote, strip
- Tests: `minifier/tests/integration.rs`
- Config: `minifier/Cargo.toml`

## Gotchas

- CLI uses `clap` derive API for argument parsing
- Integration tests use `assert_cmd` + `tempfile` + `predicates`
- (Add operational gotchas as you discover them)
