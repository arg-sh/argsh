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
  mkdir -p "${_d}" 2>/dev/null || true
  if [[ -d "${_d}" && -w "${_d}" ]]; then
    echo "${_d}"; return 0
  fi
  return 1
}

# @description Detect the architecture for release asset naming.
# Maps uname -m to the release suffix (e.g. x86_64 → amd64, aarch64 → arm64).
# @stdout Architecture string (amd64, arm64)
# @exitcode 1 If architecture is unsupported
# @internal
argsh::builtin::arch() {
  case "$(uname -m)" in
    x86_64)  echo "amd64" ;;
    aarch64) echo "arm64" ;;
    *) return 1 ;;
  esac
}

# @description Download argsh.so from the latest GitHub release.
# @arg $1 int Force download even if already installed (0|1, default 0)
# @exitcode 0 Builtin downloaded and loaded successfully
# @exitcode 1 Download failed or unsupported platform
# @internal
# shellcheck disable=SC2120
argsh::builtin::download() {
  local _force="${1:-0}" _dir _dest _tag _arch

  # Skip if already loaded (unless force)
  if (( ! _force )) && argsh::builtin::try 2>/dev/null; then
    echo "argsh: builtins already installed" >&2
    return 0
  fi

  # Check OS (Linux only)
  [[ "$(uname -s)" == "Linux" ]] || {
    echo "argsh: builtins are only available for Linux (got $(uname -s))" >&2
    return 1
  }

  # Detect architecture
  _arch="$(argsh::builtin::arch)" || {
    echo "argsh: unsupported architecture: $(uname -m)" >&2
    echo "  Available: x86_64 (amd64), aarch64 (arm64)" >&2
    return 1
  }

  # Find writable install dir
  _dir="$(argsh::builtin::install_dir)" || {
    echo "argsh: no writable install path found for builtins" >&2
    echo "  Run: argsh builtin install --path /your/writable/dir" >&2
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

  local _asset="argsh-linux-${_arch}.so"
  echo "argsh: downloading ${_asset} (${_tag})..." >&2
  curl -fsSL -o "${_dest}" \
    "https://github.com/arg-sh/argsh/releases/download/${_tag}/${_asset}" || {
    echo "argsh: download failed" >&2
    echo "  Asset ${_asset} may not exist for ${_tag}" >&2
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
# @arg $1 string Subcommand: install, update, status, or empty for status
# @example
#   argsh builtin            # show current status
#   argsh builtin install    # download if not present
#   argsh builtin update     # re-download latest
argsh::builtin() {
  case "${1:-}" in
    install) shift; argsh::builtin::_install "${@}" ;;
    update)  shift; argsh::builtin::_install --force "${@}" ;;
    status|"")
      local _loc _arch
      _loc="$(argsh::builtin::location 2>/dev/null)" || _loc="not installed"
      _arch="$(argsh::builtin::arch 2>/dev/null)" || _arch="unsupported"
      echo "argsh builtin: ${_loc}"
      echo "  platform: $(uname -s | tr '[:upper:]' '[:lower:]')/${_arch}"
      echo "  loaded:   $(( ${ARGSH_BUILTIN:-0} )) (ARGSH_BUILTIN=${ARGSH_BUILTIN:-0})"
      echo ""
      echo "Usage: argsh builtin [install|update|status] [--force] [--path DIR]"
      echo "       Set ARGSH_BUILTIN_PATH env var to control builtin search path."
      ;;
    *)
      echo "argsh: unknown builtin subcommand: ${1}" >&2
      echo "Usage: argsh builtin [install|update|status] [--force] [--path DIR]" >&2
      return 1
      ;;
  esac
}

# @description Backward-compat alias for argsh::builtin (plural form).
# @internal
argsh::builtins() { argsh::builtin "${@}"; }

# @description Discover search directories for scripts and tests.
# Uses PATH_TEST (semicolon-separated), then common locations under PATH_BASE.
# @set _search_dirs array Directories to search (deduplicated)
# @internal
argsh::discover_dirs() {
  _search_dirs=()
  local _d _existing _skip _rd _re
  # PATH_TEST: semicolon-separated list of directories
  if [[ -n "${PATH_TEST:-}" ]]; then
    IFS=';' read -ra _search_dirs <<< "${PATH_TEST}"
  fi
  # Common locations (skip duplicates)
  for _d in \
    "${BASH_SOURCE[0]%/*}" \
    "${PATH_BASE:-.}" \
    "${PATH_BASE:-.}/test" \
    "${PATH_BASE:-.}/tests" \
    "${PATH_BASE:-.}/libraries"; do
    [[ -d "${_d}" ]] || continue
    _skip=0
    _rd="$(realpath "${_d}" 2>/dev/null || echo "${_d}")"
    for _existing in "${_search_dirs[@]}"; do
      [[ -d "${_existing}" ]] || continue
      _re="$(realpath "${_existing}" 2>/dev/null || echo "${_existing}")"
      [[ "${_rd}" != "${_re}" ]] || { _skip=1; break; }
    done
    (( _skip )) || _search_dirs+=("${_d}")
  done
}

# @description Find files matching a pattern across discovered directories.
# Caller must declare: local -a _found_files=()
# @arg $@ string Glob patterns to search (e.g. "*.sh" "*.bats")
# @set _found_files array Matching files (appended to caller's array)
# @internal
argsh::discover_files() {
  local -a _search_dirs=()
  argsh::discover_dirs
  local _d _f _pattern
  for _d in "${_search_dirs[@]}"; do
    [[ -d "${_d}" ]] || continue
    for _pattern in "${@}"; do
      for _f in "${_d}"/${_pattern}; do
        [[ -f "${_f}" ]] || continue
        _found_files+=("${_f}")
      done
    done
  done
}

# @description Show comprehensive argsh runtime status.
# @stdout Multi-line status report
# @example
#   argsh status
argsh::status() {
  # Version + identity
  echo "argsh ${ARGSH_VERSION:-unknown} (${ARGSH_COMMIT_SHA:-unknown})"
  echo "  script: $(realpath "${BASH_SOURCE[0]}" 2>/dev/null || echo "${BASH_SOURCE[0]}")"
  echo ""

  # Builtin (.so) status
  local _loc _arch _so_status
  _loc="$(argsh::builtin::location 2>/dev/null)" || _loc="not installed"
  _arch="$(argsh::builtin::arch 2>/dev/null)" || _arch="unsupported"
  if (( ${ARGSH_BUILTIN:-0} )); then
    _so_status="loaded"
  else
    _so_status="not loaded"
  fi
  echo "Builtin (.so):"
  echo "  status:       ${_so_status}"
  echo "  path:         ${_loc}"
  echo "  architecture: $(uname -s | tr '[:upper:]' '[:lower:]')/${_arch}"
  echo ""

  # Shell
  echo "Shell:"
  echo "  bash: ${BASH_VERSION:-unknown}"
  echo ""

  # Features
  echo "Features:"
  if (( ${ARGSH_BUILTIN:-0} )); then
    echo "  mcp:        available (builtin)"
    echo "  completion: available (builtin)"
    echo "  docgen:     available (builtin)"
  else
    echo "  mcp:        requires builtin"
    echo "  completion: requires builtin"
    echo "  docgen:     requires builtin"
  fi
  echo ""

  # Tests
  local -a _found_files=()
  argsh::discover_files "*.bats"
  if (( ${#_found_files[@]} > 0 )); then
    echo "Tests: ${#_found_files[@]} .bats file(s)"
    local _f
    for _f in "${_found_files[@]}"; do
      echo "  ${_f}"
    done
  else
    echo "Tests: none found"
  fi

  # Coverage — search for coverage.json under PATH_BASE
  local -a _cov_files=()
  local _d _cov_file
  for _d in "${PATH_BASE:-.}" "${PATH_BASE:-.}"/*/; do
    _d="${_d%/}"
    [[ -f "${_d}/coverage.json" ]] && _cov_files+=("${_d}/coverage.json")
  done
  if (( ${#_cov_files[@]} > 0 )); then
    echo "Coverage:"
    for _cov_file in "${_cov_files[@]}"; do
      local _pct="?" _date="?"
      _pct="$(grep -o '"percent_covered"[^"]*"[^"]*"' "${_cov_file}" | tail -1)" && [[ "${_pct}" =~ \"([0-9.]+)\"$ ]] && _pct="${BASH_REMATCH[1]}"
      _date="$(grep -o '"date"[^"]*"[^"]*"' "${_cov_file}")" && [[ "${_date}" =~ \"([^\"]+)\"$ ]] && _date="${BASH_REMATCH[1]}"
      echo "  ${_cov_file##"${PATH_BASE:-.}"/}: ${_pct}% (${_date})"
    done
  else
    echo "Coverage: no coverage.json found"
  fi
}

# @description Print argsh help/usage information.
# @internal
argsh::help() {
  echo "argsh ${ARGSH_VERSION:-unknown}"
  echo ""
  echo "Usage: argsh [flags] <script> [script-args...]"
  echo "       argsh <command> [args...]"
  echo ""
  echo "Commands:"
  echo "  builtin [install|update|status]  Manage native builtins (.so)"
  echo "  status                           Show argsh runtime status"
  echo ""
  echo "Flags:"
  echo "  --version          Print version and exit"
  echo "  --help, -h         Show this help and exit"
  echo "  -i, --import LIB   Import library before running script"
  echo "  --no-builtin       Skip builtin loading and auto-download"
  echo ""
  echo "Environment:"
  echo "  ARGSH_BUILTIN_PATH       Path to argsh.so (overrides auto-search)"
  echo "  ARGSH_NO_AUTO_DOWNLOAD   Set to 1 to skip auto-download of builtins"
  echo "  ARGSH_DEBUG              Set to 1 to enable debug trace output"
  echo "  PATH_TEST                Semicolon-separated dirs for test/script discovery"
}

# @internal
argsh::builtin::_install() {
  local _force=0 _dest_dir=""
  while [[ "${1:-}" == --* ]]; do
    case "${1}" in
      --force) _force=1; shift ;;
      --path)
        shift
        if [[ -z "${1:-}" || "${1}" == --* ]]; then
          echo "argsh: --path requires a directory argument" >&2
          return 1
        fi
        _dest_dir="${1}"; shift
        ;;
      *) echo "argsh: unknown option: ${1}" >&2; return 1 ;;
    esac
  done

  # If --path given, override install dir logic
  if [[ -n "${_dest_dir}" ]]; then
    [[ -d "${_dest_dir}" && -w "${_dest_dir}" ]] || {
      echo "argsh: directory not writable: ${_dest_dir}" >&2
      return 1
    }
    PATH_BIN="${_dest_dir}" argsh::builtin::download "${_force}"
  else
    argsh::builtin::download "${_force}"
  fi
}

# @description Run a bash script from a shebang or as a CLI.
# @arg $@ string Flags followed by file to run
#
# Commands (when first arg is a keyword):
#   builtin [install|update|status]  Manage native builtins (.so)
#   builtins ...                     Alias for builtin (backward compat)
#   status                           Show argsh runtime status
#
# Flags (parsed before script file):
#   -i, --import <lib>  Import additional libraries (repeatable).
#   --no-builtin        Skip builtin loading and auto-download.
#   --version           Print argsh version and exit.
#   --help, -h          Show usage information.
#
# Builtins are loaded by default and auto-downloaded if missing.
# Use --no-builtin to disable. Control install path via
# ARGSH_BUILTIN_PATH env var or: argsh builtin install --path /your/dir
#
# @exitcode 1 If the file does not exist
argsh::shebang() {
  local -a _argsh_imports=()
  local _argsh_no_builtin=0

  # Parse argsh flags before the script file
  while [[ "${1:-}" == -* ]]; do
    case "${1}" in
      --help|-h)
        argsh::help
        return 0
        ;;
      --import|-i)
        shift
        [[ -n "${1:-}" ]] || { echo "argsh: --import requires an argument" >&2; return 1; }
        _argsh_imports+=("${1}")
        shift
        ;;
      --no-builtin)
        _argsh_no_builtin=1
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

  # No args: show help
  if [[ -z "${1:-}" ]]; then
    argsh::help
    return 0
  fi

  local -r file="${1}"
  : "${ARGSH_SOURCE="${file}"}"
  export ARGSH_SOURCE

  # Handle commands before file/docker check
  case "${file}" in
    builtin)  shift; argsh::builtin "${@}";  return ;;
    builtins) shift; argsh::builtins "${@}"; return ;;
    status)
      # Try loading builtins first so status reports accurate state
      # obfus ignore variable
      declare -gi ARGSH_BUILTIN=0
      # shellcheck disable=SC2034
      if (( ! _argsh_no_builtin )) && declare -p __ARGSH_BUILTINS &>/dev/null; then
        argsh::builtin::try && ARGSH_BUILTIN=1
      fi
      shift; argsh::status "${@}"; return ;;
  esac

  [[ "${BASH_SOURCE[-1]}" != "${file}" && -f "${file}" ]] || {
    binary::exists docker || {
      echo "This script requires Docker to be installed"
      return 1
    } >&2
    local tty="-i"
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

  # Load builtins: try loading, auto-download if missing (unless --no-builtin)
  # obfus ignore variable
  declare -gi ARGSH_BUILTIN=0
  # shellcheck disable=SC2034
  if (( ! _argsh_no_builtin )); then
    [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: searching for argsh.so..." >&2
    if argsh::builtin::try; then
      ARGSH_BUILTIN=1
      [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: loaded builtins from $(argsh::builtin::location 2>/dev/null || echo 'unknown')" >&2
    else
      [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: builtins not found locally" >&2
      # Auto-download from latest release (unless ARGSH_NO_AUTO_DOWNLOAD=1)
      if [[ "${ARGSH_NO_AUTO_DOWNLOAD:-}" != "1" ]]; then
        [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: attempting auto-download of builtins" >&2
        argsh::builtin::download 0 && argsh::builtin::try && ARGSH_BUILTIN=1
      else
        [[ "${ARGSH_DEBUG:-}" == "1" ]] && echo "argsh:debug: auto-download disabled (ARGSH_NO_AUTO_DOWNLOAD=1)" >&2
      fi
    fi
  fi

  # Import additional libraries
  local _lib
  for _lib in "${_argsh_imports[@]}"; do
    import "${_lib}"
  done

  shift
  # shellcheck source=/dev/null
  . "${file}"
}