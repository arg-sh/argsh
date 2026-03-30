#!/usr/bin/env bash
# @file verbose
# @brief Debug and trace output utilities
# @tags core
set -euo pipefail

# @description Print debug message to stderr when ARGSH_DEBUG=1
# @arg $1 string Message to print
verbose::debug() {
  [[ "${ARGSH_DEBUG:-}" == "1" ]] || return 0
  echo "argsh:debug: ${1}" >&2
}
