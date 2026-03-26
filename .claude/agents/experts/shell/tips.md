# Shell — Operational Tips

## Build & Test

- Build: `docker build --target artifacts .`
- Test: `bats libraries/*.bats`
- Lint: `shellcheck libraries/*.sh`

## Key Paths

- Source: `libraries/*.sh`
- Tests: `libraries/*.bats`, `libraries/fixtures/`
- Runtime: `argsh.min.sh`, `argsh.min.tmpl`, `argsh-so.min.tmpl`
- Coverage: `coverage/`
- Benchmarks: `bench/`
- Test helpers: `test/helper.bash`

## Gotchas

- Libraries use `source argsh` pattern — the import system resolves paths
- `.bats` files use bats-core testing framework
- `argsh.min.sh` is the minified/bundled runtime — don't edit directly
- (Add operational gotchas as you discover them)
