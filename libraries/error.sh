#!/usr/bin/env bash
# @file error
# @brief Functions for error handling
# @tags core
# @description
#   This file contains functions for error handling
set -euo pipefail

# @description Print a stacktrace
# @arg $1 int The exit code
# @exitcode 0 Always
# @example
#   trap "error::stacktrace" EXIT
error::stacktrace() {
  local -r code="${1:-${?}}"
  if (( code )); then
    echo -e "\n\033[38;5;196m■■ Stacktrace(${code}): \e[1m${BASH_COMMAND}\e[22m"
    for (( i = 1; i <= ${#FUNCNAME[@]} - 2; i++ )); do
      echo -e "${i}. ${BASH_SOURCE[i]}:${BASH_LINENO[i-1]} ➜ ${FUNCNAME[i]}()"
    done
    echo -e "\033[0m"
    return "${code}"
  fi
}

:args::_error() {
  echo -e "Error: ${1}" >&2
  exit 2
}

:args::error() {
  echo -e "Error: ${1}\n" >&2
  echo "  Run \"${0##*/} -h\" for more information." >&2
  exit 2
}

:args::error_usage() {
  echo -e "Error: ${1}\n" >&2
  echo "  Run \"${0##*/} -h\" for more information." >&2
  exit 2
}