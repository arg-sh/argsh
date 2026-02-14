# All the tools required to run the tests, lint and coverage Bash scripts

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
FROM rust:1-slim-bookworm AS builtin-build
ARG RUSTFLAGS
ARG CARGO_PROFILE_RELEASE_STRIP
ARG CARGO_PROFILE_RELEASE_LTO
ARG CARGO_PROFILE_RELEASE_PANIC
WORKDIR /build
COPY builtin/ .
RUN cargo build --release

# artifacts — extract just the Rust binaries (used by CI for multi-arch release)
FROM scratch AS artifacts
COPY --from=minifier-build /build/target/release/minifier /minifier
COPY --from=shdoc-build /build/target/release/shdoc /shdoc
COPY --from=builtin-build /build/target/release/libargsh.so /libargsh.so

# coverage
FROM kcov/kcov

# test
RUN set -eux \
  && apt update \
  && apt install -y git \
  && git clone https://github.com/bats-core/bats-core.git \
  && cd bats-core \
  && ./install.sh /usr/local \
  && cd .. \
  && rm -rf bats-core \
  && apt remove -y git \
  && apt autoremove -y \
  && rm -rf /var/lib/apt/lists/*

# lint
COPY --from=koalaman/shellcheck:stable /bin/shellcheck /usr/local/bin/shellcheck

# tools
COPY --from=ghcr.io/jqlang/jq:latest /jq /usr/local/bin/jq
RUN set -eux \
  && apt update \
  && apt install -y gettext-base \
  && rm -rf /var/lib/apt/lists/*

# argsh itself
COPY --from=minifier-build /build/target/release/minifier /usr/local/bin/minifier
COPY --from=shdoc-build /build/target/release/shdoc /usr/local/bin/shdoc
COPY --from=builtin-build /build/target/release/libargsh.so /usr/local/lib/argsh.so
COPY ./argsh.min.sh /usr/local/bin/argsh
ENV ARGSH_BUILTIN_PATH=/usr/local/lib/argsh.so

# docker
COPY ./.docker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT [ "docker-entrypoint.sh" ]