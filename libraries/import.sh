#!/usr/bin/env bash
# @file import
# @brief Import libraries
# @description
#   This file contains functions for importing libraries
set -euo pipefail

declare -gA import_cache=()

# @description
#   Import a library, relative to the current script
#   If '@' is prepended to the library name, it will be imported from the base path
#   If '~' is prepended to the library name, it will be imported from the script entry point
# @arg $1 string Library name
# @example
#   import fmt
 import() { 
  local src="${1}"
  (( ${import_cache["${src}"]:-} )) || { 
    import_cache["${src}"]=1
    # shellcheck disable=SC1090
    if [[ ${src:0:1} == "@" ]]; then
      src="${PATH_BASE:?"PATH_BASE missing"}/${src:1}";
    elif [[ ${src:0:1} == "~" ]]; then
      local _s="${ARGSH_SOURCE:-${BASH_SOURCE[-1]}}"
      src="${_s%/*}/${src:1}"
    else
      src="${BASH_SOURCE[0]%/*}/${src}"
    fi
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
#   Clear the import cache
import::clear() {
  import_cache=()
}