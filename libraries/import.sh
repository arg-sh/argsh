#!/usr/bin/env bash
# @file import
# @brief Import libraries
# @description
#   Caching import mechanism for bash libraries. Each module is sourced
#   at most once per session (unless cache is cleared).
#
#   Prefixes: @ → PATH_BASE/git root, ^ → PATH_SCRIPTS/directive/walk-up,
#   ~ → script entry point, bare → relative to caller.
set -euo pipefail

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
# Library directory for builtin import resolution (plain names like `import string`)
# obfus ignore variable
: "${__ARGSH_LIB_DIR:=${BASH_SOURCE[0]%/*}}"

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
        # Walk up from script dir looking for .git
        local _s="${ARGSH_SOURCE:-${BASH_SOURCE[-1]}}"
        local _sdir; [[ "${_s}" == */* ]] && _sdir="${_s%/*}" || _sdir="."
        _base="$(import::_find_git_root "$(cd "${_sdir}" 2>/dev/null && pwd)")" || {
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
      # Plain import: check if file exists, fallback to plugin libs
      if ! import::_file_exists "${src}"; then
        # Only try plugin libs for simple names (no path separators or traversal)
        if [[ "${1}" != */* && "${1}" != *..* ]]; then
          local _libs_dir
          _libs_dir="$(import::_libs_dir)"
          local _lib="${_libs_dir}/${1}/${1}"
          if import::_file_exists "${_lib}"; then
            src="${_lib}"
          fi
        fi
      fi
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
  _path="${_path#"${_path%%[![:space:]]*}"}"  # trim all leading whitespace
  _path="${_path%"${_path##*[![:space:]]}"}"  # trim all trailing whitespace
  # Resolve relative to script directory
  if [[ "${_path:0:1}" != "/" ]]; then
    _path="${_dir}/${_path}"
  fi
  # Normalize
  _path="$(cd "${_path}" 2>/dev/null && pwd)" || return 0
  echo "${_path}"
}

# @description Check if a file exists with any of the standard extensions.
# @arg $1 string Base path (without extension)
# @exitcode 0 If found
# @exitcode 1 If not found
# @internal
import::_file_exists() {
  local _ext
  for _ext in "" ".sh" ".bash"; do
    [[ -f "${1}${_ext}" ]] && return 0
  done
  return 1
}

# @description Resolve the plugin libs directory.
# Reads defaults.path_libs from .argsh.yaml (cached after first call).
# Falls back to .argsh/libs/.
# @stdout The libs directory path
# @internal
declare -g __ARGSH_LIBS_DIR=""
import::_libs_dir() {
  if [[ -n "${__ARGSH_LIBS_DIR}" ]]; then
    echo "${__ARGSH_LIBS_DIR}"
    return
  fi
  local _base="${PATH_BASE:-.}"
  if [[ -f "${_base}/.argsh.yaml" ]] && command -v yq &>/dev/null; then
    local _custom
    _custom="$(yq -r '.defaults.path_libs // ""' "${_base}/.argsh.yaml" 2>/dev/null)" || _custom=""
    if [[ -n "${_custom}" ]]; then
      if [[ "${_custom:0:1}" == "/" ]]; then
        __ARGSH_LIBS_DIR="${_custom}"
      else
        __ARGSH_LIBS_DIR="${_base}/${_custom}"
      fi
      echo "${__ARGSH_LIBS_DIR}"
      return
    fi
  fi
  __ARGSH_LIBS_DIR="${_base}/.argsh/libs"
  echo "${__ARGSH_LIBS_DIR}"
}

# @description Find git repository root by walking up from CWD looking for .git.
# Does not shell out to git — avoids safe.directory and PATH issues.
# @stdout The git root path
# @exitcode 1 If not found
# @internal
import::_find_git_root() {
  local _d
  _d="${1:-$(pwd)}"
  while [[ -n "${_d}" && "${_d}" != "/" ]]; do
    [[ -e "${_d}/.git" ]] && { echo "${_d}"; return 0; }
    _d="${_d%/*}"
  done
  return 1
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
  _root="$(import::_find_git_root "${_abs}" 2>/dev/null)" || _root="/"
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
