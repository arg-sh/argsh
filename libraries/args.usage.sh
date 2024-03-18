#!/usr/bin/env bash
# @file args.usage
# @brief Functions for usage information
# @description
#   This file contains functions for usage information
set -euo pipefail

: "${ARGSH_FIELD_WIDTH:=24}"
: "${ARGSH_PATH_IMPORT:=${BASH_SOURCE[0]%/*}}"

# @internal
# shellcheck disable=SC1090
import() { declare -A _i; (( ${_i[${1}]:-} )) || { _i[${1}]=1; . "${ARGSH_PATH_IMPORT}/${1}.sh"; } }
import error

# @description Print usage information
# @arg $1 string The title of the usage
# @arg $@ array User arguments
# @set usage array Usage information for the command
# @exitcode 0 If user arguments are correct
# @exitcode 2 If user arguments are incorrect
# @example
#   local -a usage
#   usage=(
#     command "Description of command"
#     [...]
#   )
#  :usage "Title" "${@}"
:usage() {
  local title="${1}"; shift
  # shellcheck disable=SC2154
  declare -p usage &>/dev/null || local -a usage=()
  [[ $(( ${#usage[@]} % 2 )) -eq 0 ]] ||
    :args::_error "usage must be an associative array"

  if [[ -z ${1:-} || ${1} == "-h" || ${1} == "--help" ]]; then
    :usage::text "${title}"
    exit 0
  fi
  for (( i=0; i < ${#usage[@]}; i+=2 )); do
    if [[ ${usage[i]/::*} == "${1}" ]]; then
      ! declare -f "${usage[i]}" &>/dev/null ||
        "${usage[i]}" "${@:2}"
      return 0
    fi
  done

  :args::error_usage "Invalid command: ${1}"
}

# @description Print usage information
# @arg $1 string The title of the usage
# @set usage array Usage information for the command
# @internal
:usage::text() {
  local title="${1:-}"
  local base="${ARGSH_SOURCE:-"${0}"}"
  base="${base##*/}"
  echo "${title}"
  echo
  echo "Usage: ${base} <command> [args]"
  echo
  echo "Available Commands:"
  for (( i=0; i < ${#usage[@]}; i+=2 )); do
    printf "  %-${ARGSH_FIELD_WIDTH}s %s\n" "${usage[i]/::*}" "${usage[i+1]}"
  done
  echo
  printf "  %-${ARGSH_FIELD_WIDTH}s %s\n" "-h, --help" "Show this help message"
  echo
  echo "Use \"${base} [command] --help\" for more information about a command."
}
