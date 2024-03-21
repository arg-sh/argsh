# All the tools required to run the tests, lint and coverage Bash scripts

# coverage
FROM kcov/kcov
LABEL org.opencontainers.image.licenses MIT
LABEL org.opencontainers.image.source https://github.com/arg-sh/argsh
LABEL org.opencontainers.image.description \
  "argsh aims to enhance Bash scripting by promoting structure and maintainability, making it easier to write, understand, and maintain even complex scripts."

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

# minify
RUN set -eux \
  && apt update \
  && apt install -y perl \
  && rm -rf /var/lib/apt/lists/*
COPY .bin/obfus /usr/local/bin/obfus

# docs
RUN set -eux \
  && apt update \
  && apt install -y gawk \
  && rm -rf /var/lib/apt/lists/*
COPY .bin/shdoc /usr/local/bin/shdoc

# lint
COPY --from=koalaman/shellcheck:stable /bin/shellcheck /usr/local/bin/shellcheck

# tools
COPY --from=ghcr.io/jqlang/jq:latest /jq /usr/local/bin/jq
RUN set -eux \
  && apt update \
  && apt install -y gettext-base \
  && rm -rf /var/lib/apt/lists/*

# argsh itself
COPY ./argsh.min.sh /usr/local/bin/argsh

# docker
COPY ./.docker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT [ "docker-entrypoint.sh" ]