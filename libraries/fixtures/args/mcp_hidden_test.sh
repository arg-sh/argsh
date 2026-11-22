#!/usr/bin/env bash
# MCP test fixture: a visible boolean flag beside a `:@` flag. Each start that
# is not the MCP server appends its arguments to the file $MCP_TEST_STARTS, so
# a test can see if a tools/call started the script.
# shellcheck disable=SC2034,SC1091
set -euo pipefail

if [[ "${1:-}" != "mcp" && -n "${MCP_TEST_STARTS:-}" ]]; then
  printf '%s\n' "${*}" >> "${MCP_TEST_STARTS}"
fi

# Source argsh library. Try ARGSH_SOURCE first (Docker), fall back to relative path.
# shellcheck disable=SC1090
if [[ -f "${ARGSH_SOURCE:-}" ]]; then
  source "${ARGSH_SOURCE}"
else
  ARGSH_SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/args.sh"
  source "${ARGSH_SOURCE}"
fi

main() {
  local -a usage=(
    'login' "Log in"
  )
  :usage "MCP hidden flag test" "${@}"
  "${usage[@]}"
}

login() {
  local verbose print_token name
  local -a args=(
    'verbose|v:+'      "Verbose output"
    'print-token|t:+@' "Print the access token"
    'name|n'           "Name to log in as"
  )
  :args "Log in" "${@}"
  echo "logged in as ${name:-nobody}"
  [[ "${verbose:-0}" != "1" ]] || echo "verbose=on"
  [[ "${print_token:-0}" != "1" ]] || echo "token=printed"
}

main "${@}"
