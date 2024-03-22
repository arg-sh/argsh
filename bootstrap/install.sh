#!/usr/bin/env bash
# shellcheck disable=SC1091
set -euo pipefail

: "${PATH_BASE:="$(git rev-parse --show-toplevel)"}"

prerequisites() {
  [[ -n "${PATH_BASE}" ]] || {
    echo "This script must be run from within a git repository"
    echo "Please run git init and try again"
    echo "Or set the PATH_BASE environment variable"
    exit 1
  } >&2

  command -v curl 1>/dev/null || {
    echo "This script requires curl to run"
    exit 1
  } >&2

  command -v direnv 1>/dev/null || {
    echo "Warning: direnv is not installed"
    echo "Please install direnv to use argsh effectively"
    echo "https://direnv.net"
  } >&2

  command -v docker 1>/dev/null || {
    echo "Warning: docker is not installed"
    echo "Please install docker to use argsh effectively"
    echo "sh -c \"\$(curl -fsSL https://get.docker.com)\""
  } >&2
}

prepare() {
  mkdir -p \
    "${PATH_BASE}/.github/workflows" \
    "${PATH_BASE}/.bin" \
    "${PATH_BASE}/scripts" \
    "${PATH_BASE}/test/fixtures"
}

download() {
  curl -fsSL https://tmpl.arg.sh > "${PATH_BASE}/scripts/main.sh"
  curl -fsSL https://tmpl-test.arg.sh > "${PATH_BASE}/scripts/main.bats"
  curl -fsSL https://envrc.arg.sh > "${PATH_BASE}/.envrc"
  curl -fsSL https://github.arg.sh > "${PATH_BASE}/.github/workflows/argsh.yaml"
  curl -fsSL https://test.arg.sh > "${PATH_BASE}/test/helper.bash"
  curl -fsSL https://min.arg.sh > "${PATH_BASE}/.bin/argsh"
  chmod +x "${PATH_BASE}/.bin/argsh" "${PATH_BASE}/scripts/main.sh"
}

main() {
  prerequisites
  prepare
  download

  direnv allow .
  export PATH="${PATH_BASE}/.bin:${PATH}"
  "${PATH_BASE}/scripts/main.sh" "${@}"
}

main "${@}"