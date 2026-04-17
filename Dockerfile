# All the tools required to run the tests, lint and coverage Bash scripts
#
# Build modes:
#   1. Extract Rust binaries:  docker buildx build --target artifacts -o out .
#   2. Full image (local):     docker buildx build .
#   3. Full image (CI):        Uses GHA cache from the build job so the Rust
#                               stages are cache hits — only the final assembly
#                               layers (tools, COPY argsh.min.sh) run fresh.

# ── Rust build stages ────────────────────────────────────────────────────

# minify — build Rust minifier
FROM rust:1-slim-bookworm AS minifier-build
WORKDIR /build
COPY minifier/ .
RUN cargo build --release

# shdoc — build Rust documentation generator
FROM rust:1-slim-bookworm AS shdoc-build
WORKDIR /build
COPY shdoc/ .
RUN cargo build --release

# builtin — build Rust loadable builtins
# System lld (from apt) is required because export_name attributes contain
# colons (e.g. ":args_struct") which cause "syntax error in VERSION script"
# with both GNU ld and rust-lld. System lld (Debian LLD 14) handles them.
# We symlink system lld over the rust-lld shim in gcc-ld/ so that
# -fuse-ld=lld resolves to system lld on all architectures.
# (-Clinker-features=-lld would be cleaner but is only stable on x86_64.)
# See: https://github.com/rust-lang/rust/issues/38238
FROM rust:1-slim-bookworm AS builtin-build
RUN apt-get update && apt-get install -y --no-install-recommends lld && rm -rf /var/lib/apt/lists/*
RUN ln -sf /usr/bin/ld.lld "$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | awk '/host/{print $2}')/bin/gcc-ld/ld.lld"
ARG RUSTFLAGS
ARG CARGO_PROFILE_RELEASE_STRIP
ARG CARGO_PROFILE_RELEASE_LTO
ARG CARGO_PROFILE_RELEASE_PANIC
ENV RUSTFLAGS="${RUSTFLAGS} -C link-arg=-fuse-ld=lld"
WORKDIR /build
COPY builtin/ .
RUN cargo build --release

# argsh-lsp / argsh-lint — LSP server + CLI linter (share the same crate).
FROM rust:1-slim-bookworm AS lsp-build
WORKDIR /build
COPY crates/ crates/
RUN cargo build --release --manifest-path crates/argsh-lsp/Cargo.toml --bins

# artifacts — extract just the Rust binaries (used by CI for multi-arch release)
FROM scratch AS artifacts
COPY --from=minifier-build /build/target/release/minifier /minifier
COPY --from=shdoc-build /build/target/release/shdoc /shdoc
COPY --from=builtin-build /build/target/release/libargsh.so /libargsh.so
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lsp /argsh-lsp
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lint /argsh-lint

# ── Final image ──────────────────────────────────────────────────────────

FROM debian:bookworm-slim

# kcov — bash script coverage (binary copied from kcov image; runtime deps
# installed via apt below to keep them up-to-date with the base image)
COPY --from=kcov/kcov /usr/local/bin/kcov /usr/local/bin/kcov

# test — bats-core + standard helper libraries (support, assert, file)
# Pinned to latest release tags; shallow clones for faster builds.
RUN set -eux \
  && apt-get update \
  && apt-get install -y --no-install-recommends \
      git bash ca-certificates curl \
      # kcov runtime dependencies
      libcurl4 libdw1 libelf1 zlib1g \
      # envsubst
      gettext-base \
  && git clone --depth 1 --branch v1.13.0 https://github.com/bats-core/bats-core.git /tmp/bats \
  && /tmp/bats/install.sh /usr/local \
  && git clone --depth 1 --branch v0.3.0 https://github.com/bats-core/bats-support.git /usr/local/lib/bats-support \
  && git clone --depth 1 --branch v2.2.4 https://github.com/bats-core/bats-assert.git /usr/local/lib/bats-assert \
  && git clone --depth 1 --branch v0.4.0 https://github.com/bats-core/bats-file.git /usr/local/lib/bats-file \
  && rm -rf /tmp/bats \
      /usr/local/lib/bats-support/.git \
      /usr/local/lib/bats-assert/.git \
      /usr/local/lib/bats-file/.git \
  && apt-get remove -y git \
  && apt-get autoremove -y \
  && rm -rf /var/lib/apt/lists/*

# lint
COPY --from=koalaman/shellcheck:stable /bin/shellcheck /usr/local/bin/shellcheck

# tools
COPY --from=ghcr.io/jqlang/jq:1.8.1 /jq /usr/local/bin/jq
COPY --from=mikefarah/yq:4.52.5 /usr/bin/yq /usr/local/bin/yq

# argsh itself
COPY --from=minifier-build /build/target/release/minifier /usr/local/bin/minifier
COPY --from=shdoc-build /build/target/release/shdoc /usr/local/bin/shdoc
COPY --from=builtin-build /build/target/release/libargsh.so /usr/local/lib/argsh.so
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lsp /usr/local/bin/argsh-lsp
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lint /usr/local/bin/argsh-lint
COPY ./argsh.min.sh /usr/local/bin/argsh
ENV ARGSH_BUILTIN_PATH=/usr/local/lib/argsh.so
ENV BATS_LIB_PATH=/usr/local/lib
ENV PATH_BASE=/workspace

# docker
COPY ./.docker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT [ "docker-entrypoint.sh" ]
