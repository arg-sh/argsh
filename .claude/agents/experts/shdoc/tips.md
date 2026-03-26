# Shdoc — Operational Tips

## Build & Test

- Build: `cargo build --release --manifest-path shdoc/Cargo.toml`
- Test: `cargo test --manifest-path shdoc/Cargo.toml`
- Lint: `cargo clippy --manifest-path shdoc/Cargo.toml`

## Key Paths

- Source: `shdoc/src/`
- Entry: `shdoc/src/main.rs`
- Parsers: `shdoc/src/parser/` (bash.rs, rust.rs, merge.rs)
- Renderers: `shdoc/src/render/` (markdown.rs, html.rs, json.rs)
- Model: `shdoc/src/model.rs`
- TOC: `shdoc/src/toc.rs`
- Tests: `shdoc/tests/`, `shdoc/tests/fixtures/`
- Config: `shdoc/Cargo.toml`

## Gotchas

- Parses both Bash and Rust source files (for builtin docs)
- Merge parser combines docs from both languages
- Outputs markdown, HTML, or JSON — check render module
- Test fixtures are shell scripts in `shdoc/tests/fixtures/`
- (Add operational gotchas as you discover them)
