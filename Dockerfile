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
FROM rust:1-slim-trixie AS minifier-build
WORKDIR /build
COPY minifier/ .
RUN cargo build --release

# shdoc — build Rust documentation generator
FROM rust:1-slim-trixie AS shdoc-build
WORKDIR /build
COPY shdoc/ .
RUN cargo build --release

# builtin — build Rust loadable builtins
# System lld (from apt) is required because export_name attributes contain
# colons (e.g. ":args_struct") which cause "syntax error in VERSION script"
# with both GNU ld and rust-lld. The system lld (from apt) handles them.
# We symlink system lld over the rust-lld shim in gcc-ld/ so that
# -fuse-ld=lld resolves to system lld on all architectures.
# (-Clinker-features=-lld would be cleaner but is only stable on x86_64.)
# See: https://github.com/rust-lang/rust/issues/38238
# Build on bookworm (glibc 2.36) for maximum glibc compatibility.
FROM rust:1-slim-bookworm AS builtin-build
RUN apt-get update && apt-get install -y --no-install-recommends lld && rm -rf /var/lib/apt/lists/*
RUN ln -sf /usr/bin/ld.lld "$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | awk '/host/{print $2}')/bin/gcc-ld/ld.lld"
ARG RUSTFLAGS
ARG CARGO_PROFILE_RELEASE_STRIP
ARG CARGO_PROFILE_RELEASE_LTO
ARG CARGO_PROFILE_RELEASE_PANIC
ARG ARGSH_SO_VERSION
ARG ARGSH_SO_COMMIT
ENV RUSTFLAGS="${RUSTFLAGS} -C link-arg=-fuse-ld=lld"
WORKDIR /build
COPY builtin/ .
RUN ARGSH_SO_VERSION="${ARGSH_SO_VERSION}" ARGSH_SO_COMMIT="${ARGSH_SO_COMMIT}" \
    cargo build --release

# Musl build for Alpine — cdylib works with -C target-feature=-crt-static.
FROM rust:1-alpine AS builtin-build-musl
RUN apk add --no-cache lld
# Symlink system lld over rust-lld (same workaround as glibc build — colon
# symbols in export_name break rust-lld's version script parser).
RUN ln -sf /usr/bin/ld.lld "$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | awk '/host/{print $2}')/bin/gcc-ld/ld.lld"
ARG RUSTFLAGS
ARG CARGO_PROFILE_RELEASE_STRIP
ARG CARGO_PROFILE_RELEASE_LTO
ARG CARGO_PROFILE_RELEASE_PANIC
ARG ARGSH_SO_VERSION
ARG ARGSH_SO_COMMIT
ENV RUSTFLAGS="${RUSTFLAGS} -C target-feature=-crt-static -C link-arg=-fuse-ld=lld"
WORKDIR /build
COPY builtin/ .
RUN ARGSH_SO_VERSION="${ARGSH_SO_VERSION}" ARGSH_SO_COMMIT="${ARGSH_SO_COMMIT}" \
    cargo build --release

# argsh-lsp / argsh-lint — LSP server + CLI linter (share the same crate).
FROM rust:1-slim-trixie AS lsp-build
WORKDIR /build
COPY crates/ crates/
RUN cargo build --release --manifest-path crates/argsh-lsp/Cargo.toml --bins

# artifacts — extract just the Rust binaries (used by CI for multi-arch release)
FROM scratch AS artifacts
COPY --from=minifier-build /build/target/release/minifier /minifier
COPY --from=shdoc-build /build/target/release/shdoc /shdoc
COPY --from=builtin-build /build/target/release/libargsh.so /libargsh.so
COPY --from=builtin-build-musl /build/target/release/libargsh.so /libargsh-musl.so
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lsp /argsh-lsp
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lint /argsh-lint
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-dap /argsh-dap

# ── Final image ──────────────────────────────────────────────────────────

FROM debian:trixie-slim

# kcov — bash script coverage (binary copied from pinned kcov image;
# runtime deps installed via apt below to keep them up-to-date with the base)
COPY --from=kcov/kcov@sha256:5c61bd03d2b7f4fa74131b18e4e80356a92a7517872b5b9a505022c38cd6d123 /usr/local/bin/kcov /usr/local/bin/kcov

# test — bats-core + standard helper libraries (support, assert, file)
# Pinned to immutable commit SHAs for reproducibility.
RUN set -eux \
  && apt-get update \
  && apt-get install -y --no-install-recommends \
      git bash ca-certificates curl \
      # kcov runtime dependencies
      libcurl4 libdw1 libelf1 zlib1g \
      # envsubst
      gettext-base \
  && git init /tmp/bats \
  && git -C /tmp/bats remote add origin https://github.com/bats-core/bats-core.git \
  && git -C /tmp/bats fetch --depth 1 origin 3bca150ec86275d6d9d5a4fd7d48ab8b6c6f3d87 \
  && git -C /tmp/bats checkout FETCH_HEAD \
  && /tmp/bats/install.sh /usr/local \
  && git clone --depth 1 https://github.com/bats-core/bats-support.git /usr/local/lib/bats-support \
  && git -C /usr/local/lib/bats-support fetch --depth 1 origin 24a72e14349690bcbf7c151b9d2d1cdd32d36eb1 \
  && git -C /usr/local/lib/bats-support checkout FETCH_HEAD \
  && git clone --depth 1 https://github.com/bats-core/bats-assert.git /usr/local/lib/bats-assert \
  && git -C /usr/local/lib/bats-assert fetch --depth 1 origin f1e9280eaae8f86cbe278a687e6ba755bc802c1a \
  && git -C /usr/local/lib/bats-assert checkout FETCH_HEAD \
  && git clone --depth 1 https://github.com/bats-core/bats-file.git /usr/local/lib/bats-file \
  && git -C /usr/local/lib/bats-file fetch --depth 1 origin 13ad5e2ffcc360281432db3d43a306f7b3667d60 \
  && git -C /usr/local/lib/bats-file checkout FETCH_HEAD \
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
COPY --from=mikefarah/yq:4.53.2 /usr/bin/yq /usr/local/bin/yq

# argsh itself
COPY --from=minifier-build /build/target/release/minifier /usr/local/bin/minifier
COPY --from=shdoc-build /build/target/release/shdoc /usr/local/bin/shdoc
COPY --from=builtin-build /build/target/release/libargsh.so /usr/local/lib/argsh.so
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lsp /usr/local/bin/argsh-lsp
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-lint /usr/local/bin/argsh-lint
COPY --from=lsp-build /build/crates/argsh-lsp/target/release/argsh-dap /usr/local/bin/argsh-dap
COPY ./argsh.min.sh /usr/local/bin/argsh
ENV ARGSH_BUILTIN_PATH=/usr/local/lib/argsh.so
ENV BATS_LIB_PATH=/usr/local/lib
ENV PATH_BASE=/workspace

# docker
COPY ./.docker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT [ "docker-entrypoint.sh" ]
