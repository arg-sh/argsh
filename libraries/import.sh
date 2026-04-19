#!/usr/bin/env bash
# @file import
# @brief Import libraries
# @description
#   Provides a caching import mechanism for bash libraries.
#   Each module is sourced at most once per session (unless cache is cleared).
#
#   The import function resolves module paths based on prefix:
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
# @internal
declare -gA import_cache=()

 import() {
  local src="${1}"
  (( ${import_cache["${src}"]:-} )) || {
    [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: import ${src}" >&2
    import_cache["${src}"]=1
    # shellcheck disable=SC1090
    if [[ ${src:0:1} == "@" ]]; then
      # @ prefix: PATH_BASE → git root → error
      local _base="${PATH_BASE:-}"
      if [[ -z "${_base}" ]]; then
        _base="$(git rev-parse --show-toplevel 2>/dev/null)" || {
          echo "import: @ prefix requires PATH_BASE or a git repository" >&2
          exit 1
        }
      fi
      src="${_base}/${src:1}"
    elif [[ ${src:0:1} == "^" ]]; then
      # ^ prefix: PATH_SCRIPTS → # argsh source= directive → walk up → error
      local _scripts="" _mod="${src:1}"
      _scripts="$(import::_resolve_scripts)"
      if [[ -n "${_scripts}" ]]; then
        src="${_scripts}/${_mod}"
      else
        # Walk up from script dir looking for the module
        local _s="${ARGSH_SOURCE:-${BASH_SOURCE[-1]}}"
        local _dir
        [[ "${_s}" == */* ]] && _dir="${_s%/*}" || _dir="."
        src="$(import::_walk_up "${_dir}" "${_mod}")" || {
          echo "import: cannot resolve ^${_mod} — set PATH_SCRIPTS or add '# argsh source=<path>'" >&2
          exit 1
        }
      fi
    elif [[ ${src:0:1} == "~" ]]; then
      local _s="${ARGSH_SOURCE:-${BASH_SOURCE[-1]}}"
      src="${_s%/*}/${src:1}"
    else
      local _s="${ARGSH_SOURCE:-${BASH_SOURCE[0]}}"
      src="${_s%/*}/${src}"
    fi
    import::source "${src}" || exit 1
  }
}

# @description Resolve the scripts directory for ^ imports.
# Priority: PATH_SCRIPTS env var → # argsh source= directive in calling script
# @stdout The resolved scripts directory path, or empty
# @internal
import::_resolve_scripts() {
  # Check PATH_SCRIPTS first (env var always wins)
  if [[ -n "${PATH_SCRIPTS:-}" ]]; then
    echo "${PATH_SCRIPTS}"
    return
  fi
  # Look for # argsh source= directive in the calling script (first 20 lines)
  local _s="${ARGSH_SOURCE:-${BASH_SOURCE[-1]}}"
  [[ -f "${_s}" ]] || return 0
  local _dir _line
  [[ "${_s}" == */* ]] && _dir="${_s%/*}" || _dir="."
  _line="$(head -20 "${_s}" | grep -m1 '^# argsh source=' 2>/dev/null)" || return 0
  local _path="${_line#*=}"
  _path="${_path## }"
  # Resolve relative to script directory
  if [[ "${_path:0:1}" != "/" ]]; then
    _path="${_dir}/${_path}"
  fi
  # Normalize
  _path="$(cd "${_path}" 2>/dev/null && pwd)" || return 0
  echo "${_path}"
}

# @description Walk up from a directory looking for a module file.
# Stops at git root or filesystem root.
# @arg $1 string Starting directory
# @arg $2 string Module path (e.g. utils/verbose)
# @stdout Resolved file path
# @exitcode 1 If not found
# @internal
import::_walk_up() {
  local _dir _mod="${2}" _root _ext _abs
  # Resolve to absolute path to prevent infinite loop on relative dirs
  _abs="$(cd "${1}" 2>/dev/null && pwd)" || return 1
  _dir="${_abs}"
  _root="$(git rev-parse --show-toplevel 2>/dev/null)" || _root="/"
  while [[ -n "${_dir}" && "${_dir}" != "/" ]]; do
    for _ext in "" ".sh" ".bash"; do
      [[ -f "${_dir}/${_mod}${_ext}" ]] && {
        echo "${_dir}/${_mod}"
        return 0
      }
    done
    # Stop at git root
    [[ "${_dir}" != "${_root}" ]] || break
    _dir="${_dir%/*}"
  done
  return 1
}

import::source() {
  local src="${1}"
  for ext in "" ".sh" ".bash"; do
    if [[ -f "${src}${ext}" ]]; then
      [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: import resolved -> ${src}${ext}" >&2
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
