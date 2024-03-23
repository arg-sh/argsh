#!/usr/bin/env bash
# shellcheck disable=SC1091
set -euo pipefail

: "${PATH_BASE:="$(git rev-parse --show-toplevel)"}"

prerequisites() {
  [[ -n "${PATH_BASE}" ]] || {
    echo "■■ This script must be run from within a git repository"
    echo "Please run git init and try again"
    echo "Or set the PATH_BASE environment variable"
    exit 1
  } >&2

  command -v curl 1>/dev/null || {
    echo "■■ This script requires curl to run"
    exit 1
  } >&2

  command -v direnv 1>/dev/null || {
    echo "■■ Warning: direnv is not installed"
    echo "Please install direnv to use argsh effectively"
    echo "https://direnv.net"
  } >&2

  command -v docker 1>/dev/null || {
    echo "■■ Warning: docker is not installed"
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
  if [[ -f "${PATH_BASE}/.envrc" ]]; then
    echo "■■ Warning: .envrc already exists"
    echo "If you want to overwrite the file, execute the following command:"
    echo "curl -fsSL https://envrc.arg.sh > ${PATH_BASE}.envrc"
  else
    curl -fsSL https://envrc.arg.sh > "${PATH_BASE}/.envrc"
  fi

  if [[ -f "${PATH_BASE}/.github/workflows/argsh.yaml" ]]; then
    echo "■■ Warning: .github/workflows/argsh.yaml already exists"
    echo "If you want to overwrite the file, execute the following command:"
    echo "curl -fsSL https://github.arg.sh > ${PATH_BASE}/.github/workflows/argsh.yaml"
  else
    curl -fsSL https://github.arg.sh > "${PATH_BASE}/.github/workflows/argsh.yaml"
  fi

  if [[ -f "${PATH_BASE}/test/helper.bash" ]]; then
    echo "■■ Warning: test/helper.bash already exists"
    echo "If you want to overwrite the file, execute the following command:"
    echo "curl -fsSL https://test.arg.sh > ${PATH_BASE}/test/helper.bash"
  else
    curl -fsSL https://test.arg.sh > "${PATH_BASE}/test/helper.bash"
  fi

  if [[ -f "${PATH_BASE}/.bin/argsh" ]]; then
    echo "■■ Warning: .bin/argsh already exists"
    echo "If you want to overwrite the file, execute the following command:"
    echo "curl -fsSL https://min.arg.sh > ${PATH_BASE}/.bin/argsh"
  else
    curl -fsSL https://min.arg.sh > "${PATH_BASE}/.bin/argsh"
    chmod +x "${PATH_BASE}/.bin/argsh"
  fi

  if [[ -d "${PATH_BASE}/scripts" ]]; then
    echo "■■ Warning: scripts already exists"
    echo "If you want to overwrite the file, execute the following command:"
    echo "curl -fsSL https://tmpl-test.arg.sh > ${PATH_BASE}/scripts/main.bats"
    echo "curl -fsSL https://tmpl.arg.sh > ${PATH_BASE}/scripts/main.sh"
    echo "chmod +x ${PATH_BASE}/scripts/main.sh"
  else
    curl -fsSL https://tmpl-test.arg.sh > "${PATH_BASE}/scripts/main.bats"
    curl -fsSL https://tmpl.arg.sh > "${PATH_BASE}/scripts/main.sh"
    chmod +x "${PATH_BASE}/scripts/main.sh"
  fi
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