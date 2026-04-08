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
# args pulls in string/fmt/is/to/error/array (needed for :usage/:args dispatch).
# Only import if not already loaded. In argsh.min.sh, args.sh is concatenated
# earlier so :usage already exists (as a function or builtin). `type -t` finds
# both forms; `declare -F` would miss builtin-loaded :usage.
[[ -n "$(type -t :usage 2>/dev/null)" ]] || import args

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
  local -a _raw_dirs=()
  _search_dirs=()
  local _d _existing _skip _rd _re
  # PATH_TEST: semicolon-separated list of directories
  if [[ -n "${PATH_TEST:-}" ]]; then
    IFS=';' read -ra _raw_dirs <<< "${PATH_TEST}"
  fi
  # Append common locations
  _raw_dirs+=(
    "${BASH_SOURCE[0]%/*}"
    "${PATH_BASE:-.}"
    "${PATH_BASE:-.}/test"
    "${PATH_BASE:-.}/tests"
    "${PATH_BASE:-.}/libraries"
  )
  # Deduplicate all entries
  for _d in "${_raw_dirs[@]}"; do
    [[ -d "${_d}" ]] || continue
    _skip=0
    _rd="$(realpath "${_d}" 2>/dev/null || echo "${_d}")"
    for _existing in "${_search_dirs[@]}"; do
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

# @description Forward a command to the argsh docker image.
# Used by handlers (test/lint/minify/coverage/docs) when the required host
# tool (bats/shellcheck/kcov/minifier/shdoc) is not available locally.
# @arg $@ string Command and arguments to forward
# @internal
argsh::_docker_forward() {
  binary::exists docker || {
    echo "argsh: this command requires either the tool installed locally or Docker" >&2
    return 1
  }
  local tty="-i"
  ! tty -s || tty="-it"
  local -r image="${ARGSH_DOCKER_IMAGE:-ghcr.io/arg-sh/argsh:latest}"
  # shellcheck disable=SC2046
  docker run --rm ${tty} $(docker::user) \
    -e "BATS_LOAD" \
    -e "ARGSH_SOURCE" \
    -e "PATH_TEST" \
    -e "GIT_COMMIT_SHA=$(git rev-parse HEAD 2>/dev/null || :)" \
    -e "GIT_VERSION=$(git describe --tags --dirty 2>/dev/null || :)" \
    "${image}" "${@}"
}

# @description Minify Bash files into a single script.
# @arg $@ string Files or directories, plus flags (-t, -o, -i)
argsh::minify() {
  if ! binary::exists minifier; then
    argsh::_docker_forward minify "${@}"
    return
  fi
  # obfus ignore variable
  local template out="/dev/stdout"
  # shellcheck disable=SC2034
  # obfus ignore variable
  local -a files ignore_variable args=(
    'files'              "Files to minify, can be a glob pattern"
    'template|t:~file'   "Path to a template file to use for the minified file"
    'out|o'              "Path to the output file"
    'ignore-variable|i'  "Ignores specific variable names from obfuscation"
  )
  :args "Minify Bash files" "${@}"
  ! is::uninitialized files || {
    :args::error_usage "No files to minify"
    return 1
  }
  local _content _tout
  _content="$(mktemp)"
  _tout="$(mktemp)"
  # shellcheck disable=SC2064
  trap "rm -f ${_content} ${_tout}" EXIT

  local _f _file
  local -a _glob
  for _f in "${files[@]}"; do
    if [[ -d "${_f}" ]]; then
      _glob=("${_f}"/*.{sh,bash})
    else
      # shellcheck disable=SC2206 disable=SC2128
      _glob=(${_f})
    fi
    for _file in "${_glob[@]}"; do
      [[ -e "${_file}" ]] || continue
      {
        cat "${_file}"
        echo
      } >>"${_content}"
    done
  done
  local -a _iVars=()
  if ! is::uninitialized ignore_variable && (( ${#ignore_variable[@]} )); then
    _iVars=(-I "$(array::join "," "${ignore_variable[@]}")")
  fi
  # shellcheck disable=SC2086
  minifier -i "${_content}" -o "${_tout}" -O "${_iVars[@]}"
  # obfus ignore variable
  local -r data="$(cat "${_tout}")"
  if [[ -z "${template:-}" ]]; then
    echo -n "${data}" >"${out}"
    return 0
  fi
  # obfus ignore variable
  local commit_sha="${GIT_COMMIT_SHA:-}"
  # obfus ignore variable
  local version="${GIT_VERSION:-}"
  export data commit_sha version
  # shellcheck disable=SC2016
  envsubst '$data,$commit_sha,$version' <"${template}" >"${out}"
}

# @description Lint Bash files with shellcheck.
# @arg $@ string Files or directories (optional; auto-discovered via PATH_TEST)
argsh::lint() {
  if ! binary::exists shellcheck; then
    argsh::_docker_forward lint "${@}"
    return
  fi
  # shellcheck disable=SC2034
  # obfus ignore variable
  local -a files args=(
    'files'  "Files to lint, can be a glob pattern"
  )
  :args "Lint Bash files" "${@}"
  if is::uninitialized files; then
    local -a _found_files=()
    argsh::discover_files "*.sh" "*.bash" "*.bats"
    # Also find extensionless scripts with bash/sh shebang
    local -a _search_dirs=()
    argsh::discover_dirs
    local _d _f
    for _d in "${_search_dirs[@]}"; do
      [[ -d "${_d}" ]] || continue
      for _f in "${_d}"/*; do
        [[ -f "${_f}" ]] || continue
        local _basename="${_f##*/}"
        [[ "${_basename}" != *.* ]] || continue
        # Check shebang line without a complex regex (the minifier mangles
        # single-quoted regexes containing `|`).
        local _shebang
        _shebang="$(head -1 "${_f}" 2>/dev/null || :)"
        if [[ "${_shebang}" == "#!"*bash* \
           || "${_shebang}" == "#!"*"/sh"* \
           || "${_shebang}" == "#!"*argsh* ]]; then
          _found_files+=("${_f}")
        fi
      done
    done
    if (( ${#_found_files[@]} == 0 )); then
      echo "No files to lint (set PATH_TEST or pass files as arguments)" >&2
      return 1
    fi
    # obfus ignore variable
    files=("${_found_files[@]}")
  fi

  local _file _f
  local -a _glob
  for _f in "${files[@]}"; do
    if [[ -d "${_f}" ]]; then
      _glob=("${_f}"/*.{sh,bash,bats})
    else
      # shellcheck disable=SC2206 disable=SC2128
      _glob=(${_f})
    fi
    for _file in "${_glob[@]}"; do
      [[ -e "${_file}" ]] || continue
      echo "Linting ${_file}" >&2
      shellcheck "${_file}"
    done
  done
}

# @description Run bats tests.
# @arg $@ string Paths to .bats files (optional; auto-discovered via PATH_TEST)
argsh::test() {
  if ! binary::exists bats; then
    argsh::_docker_forward test "${@}"
    return
  fi
  # shellcheck disable=SC2034
  # obfus ignore variable
  local -a path args=(
    'path'  "Path to the bats test files"
  )
  :args "Run tests" "${@}"
  if is::uninitialized path; then
    local -a _found_files=()
    argsh::discover_files "*.bats"
    if (( ${#_found_files[@]} == 0 )); then
      echo "No test files found (set PATH_TEST or pass files as arguments)" >&2
      return 1
    fi
    # obfus ignore variable
    path=("${_found_files[@]}")
  fi
  [[ -z "${BATS_LOAD:-}" ]] || echo "Running tests for ${BATS_LOAD}" >&2
  bats "${path[@]}"
}

# @description Generate coverage report for Bash scripts.
# @arg $@ string Paths to .bats files, plus flags (-o, --min)
argsh::coverage() {
  if ! binary::exists kcov; then
    argsh::_docker_forward coverage "${@}"
    return
  fi
  # obfus ignore variable
  local out="./coverage" min=75
  # shellcheck disable=SC2034
  # obfus ignore variable
  local -a tests=(".") args=(
    'tests'     "Path to the bats test files"
    'out|o'     "Path to the output directory"
    'min|:~int' "Minimum coverage required"
  )
  :args "Generate coverage report for your Bash scripts" "${@}"

  echo "Generating coverage report for: ${tests[*]}" >&2
  kcov \
    --clean \
    --bash-dont-parse-binary-dir \
    --include-pattern=.sh \
    --exclude-pattern=tests \
    --include-path=. \
    "${out}" bats "${tests[@]}" >/dev/null 2>&1 || {
      echo "Failed to generate coverage report"
      echo "Run tests with 'argsh test' to see what went wrong"
      return 1
    } >&2

  cp "${out}"/bats.*/coverage.json "${out}/coverage.json"
  # obfus ignore variable
  local coverage
  # obfus ignore variable
  coverage="$(jq -r '.percent_covered | tonumber | floor' "${out}/coverage.json")"

  echo "Coverage is ${coverage}% of required ${min}%"
  (( coverage > min )) || return 1
}

# @description Generate documentation for Bash libraries.
# @arg $@ string --in, --out, --prefix
argsh::docs() {
  if ! binary::exists shdoc; then
    argsh::_docker_forward docs "${@}"
    return
  fi
  # obfus ignore variable
  local in out prefix=""
  # shellcheck disable=SC2034
  # obfus ignore variable
  local -a args=(
    'in'      "Path to the source files to generate documentation from, can be a glob pattern"
    'out'     "Path to the output directory"
    'prefix'  "Prefix for each md file"
  )
  :args "Generate documentation" "${@}"
  [[ -d "${out}" ]] || {
    :args::error_usage "out is not a directory"
    return 1
  }
  local -a shdoc_args=(-o "${out}")
  [[ -z "${prefix}" ]] || shdoc_args+=(-p "${prefix}")
  # shellcheck disable=SC2086
  shdoc "${shdoc_args[@]}" ${in}
}

# @description Top-level argsh CLI dispatcher.
# Registers all subcommands via :usage. Called by argsh::shebang when the
# first positional argument is a subcommand (not an existing file).
# @arg $@ string Command and arguments
# @internal
argsh::main() {
  local -a usage=(
    '-'                          "Tools"
    'minify:-argsh::minify'      "Minify Bash files"
    'lint:-argsh::lint'          "Lint Bash files"
    'test:-argsh::test'          "Run tests"
    'coverage:-argsh::coverage'  "Generate coverage report for your Bash scripts"
    'docs:-argsh::docs'          "Generate documentation"
    '-'                          "Runtime"
    'builtin:-argsh::builtin'    "Manage native builtins (.so)"
    'status:-argsh::_status_cmd' "Show argsh runtime status"
  )
  :usage "Enhance your Bash scripting by promoting structure and maintainability,
          making it easier to write, understand,
          and maintain even complex scripts." "${@}"
  "${usage[@]}"
}

# @description status subcommand wrapper: loads builtins first so the
# report reflects actual runtime state. Respects --no-builtin via the
# parent's _argsh_no_builtin variable (dynamic scope from argsh::shebang).
# @internal
argsh::_status_cmd() {
  declare -gi ARGSH_BUILTIN=0
  # shellcheck disable=SC2034
  if (( ${_argsh_no_builtin:-0} == 0 )) && declare -p __ARGSH_BUILTINS &>/dev/null; then
    argsh::builtin::try && ARGSH_BUILTIN=1
  fi
  argsh::status "${@}"
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
        # :usage::help calls exit, wrap in subshell so callers (and tests)
        # don't get terminated.
        (argsh::main --help) || true
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

  # No args: show help via the :usage dispatcher (subshell — :usage::help exits)
  if [[ -z "${1:-}" ]]; then
    (argsh::main --help) || true
    return 0
  fi

  local -r file="${1}"
  : "${ARGSH_SOURCE="${file}"}"
  export ARGSH_SOURCE

  # If first arg is not an existing file, treat it as a subcommand and
  # dispatch through argsh::main. This handles minify/lint/test/coverage/
  # docs/builtin/status/builtins uniformly (plus did-you-mean suggestions).
  if [[ "${BASH_SOURCE[-1]}" == "${file}" || ! -f "${file}" ]]; then
    # Backward-compat alias: "builtins" → "builtin"
    if [[ "${file}" == "builtins" ]]; then
      shift
      argsh::builtin "${@}"
      return
    fi
    argsh::main "${@}"
    return
  fi
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