#!/usr/bin/env bash
# @file main
# @brief Main function for running a bash script
# @description
#   This file contains the main function for running a bash script
set -euo pipefail

# @internal
# shellcheck disable=SC1090
import() { declare -A _i; (( ${_i[${1}]:-} )) || { _i[${1}]=1; . "${BASH_SOURCE[0]%/*}/${1}.sh"; } }
import bash
import binary
import docker
import github

# @description Try loading argsh native builtins (.so).
# Delegates search to __argsh_try_builtin() (defined in args.sh) to avoid
# duplicating the search logic. Only adds explicit-path handling.
# @arg $1 string Optional explicit path to argsh.so
# @set ARGSH_BUILTIN int 1 if builtins loaded, 0 otherwise
# @internal
# shellcheck disable=SC2120
argsh::builtin::try() {
  # If explicit path given, only try that
  if [[ -n "${1:-}" ]]; then
    [[ -f "${1}" ]] || return 1
    # shellcheck disable=SC2229
    enable -f "${1}" "${__ARGSH_BUILTINS[@]}" 2>/dev/null || return 1
    return 0
  fi
  # Search standard paths (ARGSH_BUILTIN_PATH, PATH_LIB, PATH_BIN, LD_LIBRARY_PATH, BASH_LOADABLES_PATH)
  __argsh_try_builtin
}

# @description Find the path where argsh.so is currently loaded from.
# @stdout The path to the loaded argsh.so, or "not installed"
# @internal
argsh::builtin::location() {
  local _so _d
  local -r _n="argsh.so"
  for _so in \
    "${ARGSH_BUILTIN_PATH:-}" \
    "${PATH_LIB:+${PATH_LIB}/${_n}}" \
    "${PATH_BIN:+${PATH_BIN}/${_n}}" \
  ; do
    [[ -n "${_so}" && -f "${_so}" ]] && { echo "${_so}"; return 0; }
  done
  for _d in "${LD_LIBRARY_PATH:-}" "${BASH_LOADABLES_PATH:-}"; do
    [[ -n "${_d}" ]] || continue
    local IFS=:
    for _so in ${_d}; do
      [[ -n "${_so}" && -f "${_so}/${_n}" ]] && { echo "${_so}/${_n}"; return 0; }
    done
  done
  # User-local fallback path
  [[ -f "${HOME}/.local/lib/bash/${_n}" ]] && { echo "${HOME}/.local/lib/bash/${_n}"; return 0; }
  echo "not installed"
  return 1
}

# @description Find the first writable non-sudo install directory for argsh.so.
# @stdout The writable directory path
# @exitcode 1 If no writable directory found
# @internal
argsh::builtin::install_dir() {
  local _d
  # 1. PATH_BIN (project .bin/ directory)
  if [[ -n "${PATH_BIN:-}" && -d "${PATH_BIN}" && -w "${PATH_BIN}" ]]; then
    echo "${PATH_BIN}"; return 0
  fi
  # 2. PATH_LIB
  if [[ -n "${PATH_LIB:-}" && -d "${PATH_LIB}" && -w "${PATH_LIB}" ]]; then
    echo "${PATH_LIB}"; return 0
  fi
  # 3. BASH_LOADABLES_PATH dirs
  if [[ -n "${BASH_LOADABLES_PATH:-}" ]]; then
    local IFS=:
    for _d in ${BASH_LOADABLES_PATH}; do
      [[ -d "${_d}" && -w "${_d}" ]] && { echo "${_d}"; return 0; }
    done
  fi
  # 4. User-local fallback
  _d="${HOME}/.local/lib/bash"
  if [[ -d "${_d}" && -w "${_d}" ]] || mkdir -p "${_d}" 2>/dev/null; then
    echo "${_d}"; return 0
  fi
  return 1
}

# @description Download argsh.so from the latest GitHub release.
# @arg $1 int Force download even if already installed (0|1, default 0)
# @exitcode 0 Builtin downloaded and loaded successfully
# @exitcode 1 Download failed or unsupported platform
# @internal
# shellcheck disable=SC2120
argsh::builtin::download() {
  local _force="${1:-0}" _dir _dest _tag

  # Skip if already loaded (unless force)
  if (( ! _force )) && argsh::builtin::try 2>/dev/null; then
    echo "argsh: builtins already installed" >&2
    return 0
  fi

  # Check arch (only linux/amd64 for now)
  [[ "$(uname -s)" == "Linux" && "$(uname -m)" == "x86_64" ]] || {
    echo "argsh: builtins only available for linux/amd64 (got $(uname -s)/$(uname -m))" >&2
    return 1
  }

  # Find writable install dir
  _dir="$(argsh::builtin::install_dir)" || {
    echo "argsh: no writable install path found for builtins" >&2
    echo "  Run: argsh builtins install --path /your/writable/dir" >&2
    return 1
  }
  _dest="${_dir}/argsh.so"

  command -v curl &>/dev/null || {
    echo "argsh: curl is required to download builtins" >&2
    return 1
  }

  # Get latest release tag
  _tag="$(github::latest "arg-sh/argsh")" || {
    echo "argsh: failed to get latest release from GitHub" >&2
    return 1
  }

  echo "argsh: downloading argsh.so (${_tag})..." >&2
  curl -fsSL -o "${_dest}" \
    "https://github.com/arg-sh/argsh/releases/download/${_tag}/argsh.so" || {
    echo "argsh: download failed" >&2
    rm -f "${_dest}"
    return 1
  }

  echo "argsh: installed to ${_dest}" >&2

  # Verify it actually loads
  argsh::builtin::try "${_dest}" || {
    echo "argsh: downloaded file failed to load as builtin" >&2
    rm -f "${_dest}"
    return 1
  }
  return 0
}

# @description Manage argsh native builtins (.so).
# @arg $1 string Subcommand: install, update, or empty for status
# @example
#   argsh builtins           # show current status
#   argsh builtins install   # download if not present
#   argsh builtins update    # re-download latest
argsh::builtins() {
  case "${1:-}" in
    install) shift; argsh::builtin::_install "${@}" ;;
    update)  shift; argsh::builtin::_install --force "${@}" ;;
    *)
      # Print current state
      local _loc
      _loc="$(argsh::builtin::location 2>/dev/null)" || _loc="not installed"
      echo "argsh builtins: ${_loc}"
      echo ""
      echo "Usage: argsh builtins install|update [--force] [--path DIR]"
      ;;
  esac
}

# @internal
argsh::builtin::_install() {
  local _force=0 _path=""
  while [[ "${1:-}" == --* ]]; do
    case "${1}" in
      --force) _force=1; shift ;;
      --path)  shift; _path="${1}"; shift ;;
      *) echo "argsh: unknown option: ${1}" >&2; return 1 ;;
    esac
  done

  # If --path given, override install dir logic
  if [[ -n "${_path}" ]]; then
    [[ -d "${_path}" && -w "${_path}" ]] || {
      echo "argsh: directory not writable: ${_path}" >&2
      return 1
    }
    PATH_BIN="${_path}" argsh::builtin::download "${_force}"
  else
    argsh::builtin::download "${_force}"
  fi
}

# @description Run a bash script from a shebang
# @arg $@ string Flags followed by file to run
#
# Flags (parsed before script file):
#   --builtin [path]   Load native builtins. If path given and not found, fail.
#   --no-builtin       Skip builtin loading entirely.
#   -i, --import <lib> Import additional libraries (repeatable).
#   --version          Print argsh version and exit.
#
# @exitcode 1 If the file does not exist
# @exitcode 1 If builtin path specified but not found
argsh::shebang() {
  local _argsh_builtin_mode="" _argsh_builtin_path=""
  local -a _argsh_imports=()

  # Parse argsh flags before the script file
  while [[ "${1:-}" == -* ]]; do
    case "${1}" in
      --builtin)
        _argsh_builtin_mode="required"
        shift
        # Next arg is a path if it doesn't start with - and isn't the script
        if [[ -n "${1:-}" && "${1:0:1}" != "-" && "${1}" == *.so ]]; then
          _argsh_builtin_path="${1}"
          shift
        fi
        ;;
      --no-builtin)
        _argsh_builtin_mode="disabled"
        shift
        ;;
      --import|-i)
        shift
        [[ -n "${1:-}" ]] || { echo "argsh: --import requires an argument" >&2; return 1; }
        _argsh_imports+=("${1}")
        shift
        ;;
      --version)
        echo "argsh ${ARGSH_VERSION:-unknown} (${ARGSH_COMMIT_SHA:-unknown})"
        return 0
        ;;
      --)
        shift; break
        ;;
      *)
        break
        ;;
    esac
  done

  local -r file="${1}"
  : "${ARGSH_SOURCE="${file}"}"
  export ARGSH_SOURCE

  # Handle built-in commands before file check
  case "${file}" in
    builtins) shift; argsh::builtins "${@}"; return ;;
  esac

  [[ "${BASH_SOURCE[-1]}" != "${file}" && -f "${file}" ]] || {
    binary::exists docker || {
      echo "This script requires Docker to be installed"
      return 1
    } >&2
    local tty=""
    ! tty -s || tty="-it"
    # shellcheck disable=SC2046
    docker run --rm ${tty} $(docker::user) \
      -e "BATS_LOAD" \
      -e "ARGSH_SOURCE" \
      -e "GIT_COMMIT_SHA=$(git rev-parse HEAD 2>/dev/null || :)" \
      -e "GIT_VERSION=$(git describe --tags --dirty 2>/dev/null || :)" \
      ghcr.io/arg-sh/argsh:latest "${@}"
    return 0
  }
  bash::version 4 3 0 || {
    echo "This script requires bash 4.3.0 or later"
    return 1
  } >&2

  # Load builtins based on mode
  # obfus ignore variable
  declare -gi ARGSH_BUILTIN=0
  case "${_argsh_builtin_mode}" in
    required)
      argsh::builtin::try "${_argsh_builtin_path}" || {
        # If explicit path given, don't auto-download
        if [[ -n "${_argsh_builtin_path}" ]]; then
          echo "argsh: builtin not found: ${_argsh_builtin_path}" >&2
          return 1
        fi
        # Try auto-download from latest release
        argsh::builtin::download || return 1
      }
      ARGSH_BUILTIN=1
      ;;
    disabled)
      ;;
    *)
      # Default: try silently, no failure
      # shellcheck disable=SC2034
      argsh::builtin::try && ARGSH_BUILTIN=1
      ;;
  esac

  # Import additional libraries
  local _lib
  for _lib in "${_argsh_imports[@]}"; do
    import "${_lib}"
  done

  shift
  # shellcheck source=/dev/null
  . "${file}"
}