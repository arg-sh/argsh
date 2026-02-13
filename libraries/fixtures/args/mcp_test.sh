#!/usr/bin/env bash
# MCP test fixture — actual script for tools/call subprocess invocation
# shellcheck disable=SC2034,SC1091
set -euo pipefail

# Source argsh library. Try ARGSH_SOURCE first (Docker), fall back to relative path.
# shellcheck disable=SC1090
if [[ -f "${ARGSH_SOURCE:-}" ]]; then
  source "${ARGSH_SOURCE}"
else
  ARGSH_SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/args.sh"
  source "${ARGSH_SOURCE}"
fi

main() {
  local config
  local -a verbose args=(
    'verbose|v:+' "Enable verbose output"
    'config|c'    "Config file path"
  )
  local -a usage=(
    'serve'  "Start the server"
    'build'  "Build the project"
  )
  :usage "My test application" "${@}"
  "${usage[@]}"
}

serve() {
  :args "Start the server" "${@}"
  echo "serving"
  [[ "${verbose[*]:-}" != "1" ]] || echo "verbose=on"
  [[ -z "${config:-}" ]] || echo "config=${config}"
}

build() {
  :args "Build the project" "${@}"
  echo "building"
}

main "${@}"
