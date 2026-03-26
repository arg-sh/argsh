# DevOps — Operational Tips

## Build & Test

- Build: `docker build .`
- Artifacts only: `docker build --target artifacts .`
- Test: runs inside Docker (bats + shellcheck + kcov)
- Lint: `shellcheck` (installed in Docker image)

## Key Paths

- Dockerfile: `Dockerfile` (multi-stage: minifier, shdoc, builtin, artifacts, test)
- Entrypoint: `.docker/docker-entrypoint.sh`
- Test fixtures: `.docker/test/fixtures/`
- CI workflows: `.github/workflows/docs.yml`
- Bootstrap installer: `bootstrap/install.sh`, `bootstrap/scripts/main.sh`
- Documentation: `docs/`

## Gotchas

- Dockerfile has multi-stage builds: minifier-build, shdoc-build, builtin-build, artifacts, final
- `artifacts` stage is `FROM scratch` — used by CI for multi-arch release
- Final image is based on kcov/kcov for coverage support
- Builtin build requires `lld` linker (arm64 compat for colon-containing export names)
- Docs deploy triggers via `.github/workflows/docs.yml` → gh-pages branch
- (Add operational gotchas as you discover them)
