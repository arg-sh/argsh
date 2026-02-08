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

# @description Try loading argsh native builtins (.so).
# @arg $1 string Optional explicit path to argsh.so
# @set ARGSH_BUILTIN int 1 if builtins loaded, 0 otherwise
# @internal
# shellcheck disable=SC2120
argsh::try_builtin() {
  local _so _d
  local -r _n="argsh.so"
  # shellcheck disable=SC2034
  local -ra _builtins=(:usage :args
    is::array is::uninitialized is::set is::tty
    args::field_name to::int to::float to::boolean to::file to::string
    import import::clear)
  # If explicit path given, only try that
  if [[ -n "${1:-}" ]]; then
    [[ -f "${1}" ]] || return 1
    # shellcheck disable=SC2229
    enable -f "${1}" "${_builtins[@]}" 2>/dev/null || return 1
    return 0
  fi
  # Search order: ARGSH_BUILTIN_PATH, PATH_LIB, PATH_BIN, LD_LIBRARY_PATH, BASH_LOADABLES_PATH
  for _so in \
    "${ARGSH_BUILTIN_PATH:-}" \
    "${PATH_LIB:+${PATH_LIB}/${_n}}" \
    "${PATH_BIN:+${PATH_BIN}/${_n}}" \
  ; do
    [[ -n "${_so}" && -f "${_so}" ]] || continue
    # shellcheck disable=SC2229
    enable -f "${_so}" "${_builtins[@]}" 2>/dev/null || continue
    return 0
  done
  for _d in "${LD_LIBRARY_PATH:-}" "${BASH_LOADABLES_PATH:-}"; do
    [[ -n "${_d}" ]] || continue
    local IFS=:
    for _so in ${_d}; do
      [[ -n "${_so}" && -f "${_so}/${_n}" ]] || continue
      # shellcheck disable=SC2229
      enable -f "${_so}/${_n}" "${_builtins[@]}" 2>/dev/null || continue
      return 0
    done
  done
  return 1
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
      argsh::try_builtin "${_argsh_builtin_path}" || {
        echo "argsh: failed to load builtins${_argsh_builtin_path:+: ${_argsh_builtin_path}}" >&2
        return 1
      }
      ARGSH_BUILTIN=1
      ;;
    disabled)
      ;;
    *)
      # Default: try silently, no failure
      # shellcheck disable=SC2034
      argsh::try_builtin && ARGSH_BUILTIN=1
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