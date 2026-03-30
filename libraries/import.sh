#!/usr/bin/env bash
# @file import
# @brief Import libraries
# @description
#   This file contains functions for importing libraries
set -euo pipefail

declare -gA import_cache=()
# Library directory for builtin import resolution (plain names like `import string`)
# obfus ignore variable
: "${__ARGSH_LIB_DIR:=${BASH_SOURCE[0]%/*}}"

# @description
#   Import a library, relative to the current script
#   If '@' is prepended to the library name, it will be imported from the base path (PATH_BASE)
#   If '~' is prepended to the library name, it will be imported from the script entry point
#   If '^' is prepended to the library name, it will be imported from PATH_SCRIPTS
# @arg $1 string Library name
# @example
#   import fmt
#   import @libs/helper
#   import ^utils/verbose
 import() {
  local src="${1}"
  [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: import ${src}" >&2
  (( ${import_cache["${src}"]:-} )) && {
    [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: import ${src} (cached)" >&2
    return 0
  } || {
    import_cache["${src}"]=1
    # shellcheck disable=SC1090
    if [[ ${src:0:1} == "@" ]]; then
      src="${PATH_BASE:?"PATH_BASE missing"}/${src:1}";
    elif [[ ${src:0:1} == "^" ]]; then
      src="${PATH_SCRIPTS:?"PATH_SCRIPTS missing"}/${src:1}";
    elif [[ ${src:0:1} == "~" ]]; then
      local _s="${ARGSH_SOURCE:-${BASH_SOURCE[-1]}}"
      src="${_s%/*}/${src:1}"
    else
      local _s="${ARGSH_SOURCE:-${BASH_SOURCE[0]}}"
      src="${_s%/*}/${src}"
    fi
    [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: import resolved ${1} -> ${src}" >&2
    import::source "${src}" || exit 1
  }
}

import::source() {
  local src="${1}"
  for ext in "" ".sh" ".bash"; do
    if [[ -f "${src}${ext}" ]]; then
      # shellcheck disable=SC1090
      . "${src}${ext}"
      return
    fi
  done
  echo "Library not found ${src}" >&2
  return 1
}

# @description
#   Clear the import cache, allowing previously loaded libraries to be re-sourced
import::clear() {
  import_cache=()
}