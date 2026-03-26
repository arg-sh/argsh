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
    'serve@readonly'     "Start the server"
    'build@destructive'  "Build the project"
    'cluster'            "Cluster management"
    'status@json'        "Get status as JSON"
  )
  :usage "My test application" "${@}"
  "${usage[@]}"
}

serve() {
  local port
  local -a args=(
    'port|p:~int' "Port number"
  )
  :args "Start the server" "${@}"
  echo "serving on port ${port:-8080}"
  [[ "${verbose[*]:-}" != "1" ]] || echo "verbose=on"
  [[ -z "${config:-}" ]] || echo "config=${config}"
}

build() {
  local output
  local -a args=(
    'output|o' "Output directory"
  )
  :args "Build the project" "${@}"
  echo "building to ${output:-dist}"
}

status() {
  :args "Get status as JSON" "${@}"
  echo '{"status":"ok","uptime":42}'
}

cluster() {
  local -a usage=(
    'up'   "Start cluster"
    'down' "Stop cluster"
  )
  :usage "Cluster management" "${@}"
  "${usage[@]}"
}

cluster::up() {
  local nodes
  local -a args=(
    'nodes|n:~int' "Number of nodes"
  )
  :args "Start cluster" "${@}"
  echo "starting ${nodes:-3} nodes"
}

cluster::down() {
  local force
  local -a args=(
    'force|f:+' "Force shutdown"
  )
  :args "Stop cluster" "${@}"
  echo "stopping cluster"
  [[ "${force:-}" != "1" ]] || echo "force=on"
}

main "${@}"
