#!/usr/bin/env bash
# @file error
# @brief Functions for error handling
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
    for i in $(seq 1 $((${#FUNCNAME[@]} - 2))); do
      echo -e "${i}. ${BASH_SOURCE[i]}:${BASH_LINENO[i-1]} ➜ ${FUNCNAME[i]}()"
    done
    echo -e "\033[0m"
    return "${code}"
  fi
}

:args::_error() {
  declare -p field &>/dev/null || local field="???"
  echo ":args error [${field}] ➜ ${1}" >&2
  exit 2
}

:args::error() {
  declare -p field &>/dev/null || local field="???"
  echo -e "[ ${field/[:|]*} ] invalid argument\n➜ ${1}\n" >&2
  exit 2
}

:args::error_usage() {
  declare -p field &>/dev/null || local field="???"
  echo -e "[ ${field/[:|]*} ] invalid usage\n➜ ${1}\n" >&2
  echo -e "Use \"${0##*/} -h\" for more information" >&2
  exit 2
}