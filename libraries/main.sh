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

  # Detect libc: musl systems (Alpine) need the musl-linked .so
  local _libc=""
  if command -v ldd &>/dev/null && ldd --version 2>&1 | grep -qi musl; then
    _libc="-musl"
  fi
  local _asset="argsh-linux${_libc}-${_arch}.so"
  # Download to a temp file alongside the destination, then atomically move
  # into place. This avoids two failure modes:
  #   1. A partially-downloaded .so being left at the destination on network
  #      failure.
  #   2. SIGSEGV when the destination is the same path as a .so already loaded
  #      into the current bash process — overwriting an mmap'd dynamic
  #      library in place can crash on subsequent dlopen of that path.
  # Use mktemp for a unique, race-safe path (avoids symlink/race attacks if
  # the install dir is shared and prevents collisions with stale leftovers).
  local _tmp
  _tmp="$(mktemp "${_dest}.download.XXXXXX")" || {
    echo "argsh: failed to create temporary download file in ${_dir}" >&2
    return 1
  }
  echo "argsh: downloading ${_asset} (${_tag})..." >&2
  curl -fsSL -o "${_tmp}" \
    "https://github.com/arg-sh/argsh/releases/download/${_tag}/${_asset}" || {
    echo "argsh: download failed" >&2
    echo "  Asset ${_asset} may not exist for ${_tag}" >&2
    rm -f "${_tmp}"
    return 1
  }

  # Verify SHA256 checksum against the release's sha256sum.txt
  local _expected_sha _actual_sha _sha_cmd
  _expected_sha="$(
    curl -fsSL "https://github.com/arg-sh/argsh/releases/download/${_tag}/sha256sum.txt" \
      | grep -F -- "${_asset}" | head -1 | cut -d' ' -f1
  )" || true
  # Validate: SHA256 must be exactly 64 hex chars
  [[ "${_expected_sha}" =~ ^[0-9a-f]{64}$ ]] || _expected_sha=""
  if [[ -n "${_expected_sha}" ]]; then
    # Find available SHA256 tool
    if command -v sha256sum &>/dev/null; then
      _sha_cmd="sha256sum"
    elif command -v shasum &>/dev/null; then
      _sha_cmd="shasum -a 256"
    else
      echo "argsh: warning: no sha256sum/shasum available — skipping checksum verification" >&2
      _expected_sha=""
    fi
  fi
  if [[ -n "${_expected_sha}" ]]; then
    _actual_sha="$(${_sha_cmd} "${_tmp}" 2>/dev/null)" || true
    _actual_sha="${_actual_sha%% *}"
    if [[ -z "${_actual_sha}" ]]; then
      echo "argsh: warning: ${_sha_cmd} failed — skipping checksum verification" >&2
    elif [[ "${_actual_sha}" != "${_expected_sha}" ]]; then
      echo "argsh: SHA256 checksum mismatch for ${_asset}" >&2
      echo "  expected: ${_expected_sha}" >&2
      echo "  actual:   ${_actual_sha}" >&2
      rm -f "${_tmp}"
      return 1
    else
      [[ "${ARGSH_DEBUG:-}" != "1" ]] || echo "argsh:debug: SHA256 verified: ${_actual_sha}" >&2
    fi
  else
    [[ "${ARGSH_DEBUG:-}" != "1" ]] || echo "argsh:debug: SHA256 verification skipped (no checksum available)" >&2
  fi

  # Verify the downloaded file loads as a builtin. Run `enable -f` in a
  # subshell so any interaction with the parent process's already-loaded
  # builtins cannot affect or crash the parent. Call `enable -f` directly
  # (not via argsh::builtin::try, which suppresses stderr with 2>/dev/null)
  # so loader diagnostics (wrong arch, missing deps, etc.) stay visible.
  local _verify_err
  _verify_err="$(
    # shellcheck disable=SC2229
    (enable -f "${_tmp}" "${__ARGSH_BUILTINS[@]}") 2>&1 1>/dev/null
  )" || {
    echo "argsh: downloaded file failed to load as builtin" >&2
    [[ -n "${_verify_err}" ]] && echo "${_verify_err}" >&2
    rm -f "${_tmp}"
    return 1
  }

  # mktemp creates files with mode 0600. Set read-only before the atomic
  # move: 0444 prevents accidental in-place overwrites (which cause segfaults
  # when the .so is already loaded). bash's enable -f only needs read access.
  local _mode="444"
  chmod "${_mode}" "${_tmp}" 2>/dev/null || true

  # Atomically replace the destination. mv on the same filesystem is atomic.
  mv -f "${_tmp}" "${_dest}" || {
    echo "argsh: failed to install to ${_dest}" >&2
    rm -f "${_tmp}"
    return 1
  }

  echo "argsh: installed to ${_dest}" >&2
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

# ── Plugin library management ──────────────────────────────────────────

# @description Default OCI registry for argsh official libs.
declare -g __ARGSH_LIB_REGISTRY="${ARGSH_LIB_REGISTRY:-ghcr.io/arg-sh/libs}"

# @description Global libs directory (XDG-compliant user-wide install).
declare -gr __ARGSH_GLOBAL_LIBS="${XDG_DATA_HOME:-${HOME}/.local/share}/argsh/libs"

# @description Resolve the libs directory for the current project.
# Reads from .argsh.yaml defaults.path_libs, falls back to .argsh/libs/.
# @stdout The resolved libs directory path
# @internal
argsh::lib::dir() {
  local _dir="${PATH_BASE:-.}"
  if [[ -f "${_dir}/.argsh.yaml" ]]; then
    local _custom
    _custom="$(yq -r '.defaults.path_libs // ""' "${_dir}/.argsh.yaml" 2>/dev/null)" || _custom=""
    if [[ -n "${_custom}" ]]; then
      if [[ "${_custom:0:1}" == "/" ]]; then echo "${_custom}"; else echo "${_dir}/${_custom}"; fi
      return
    fi
  fi
  echo "${_dir}/.argsh/libs"
}

# @description Parse provider@name into registry endpoint and lib name.
# @arg $1 string Lib reference (e.g. argsh@data, myco@k8s-utils, data)
# @stdout Two lines: registry_endpoint and lib_name
# @internal
argsh::lib::resolve() {
  local _ref="${1}"
  local _provider _name _registry

  if [[ "${_ref}" == *@* ]]; then
    _provider="${_ref%%@*}"
    _name="${_ref#*@}"
  else
    _provider="argsh"
    _name="${_ref}"
  fi

  # Validate provider name (prevent yq expression injection)
  if [[ ! "${_provider}" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "argsh: invalid provider name: ${_provider}" >&2
    return 1
  fi

  if [[ "${_provider}" == "argsh" ]]; then
    _registry="${__ARGSH_LIB_REGISTRY}"
  else
    # Look up in .argsh.yaml
    local _dir="${PATH_BASE:-.}"
    if [[ -f "${_dir}/.argsh.yaml" ]]; then
      # shellcheck disable=SC2016
      _registry="$(yq -r --arg p "${_provider}" '.registries[$p].endpoint // ""' "${_dir}/.argsh.yaml" 2>/dev/null)" || _registry=""
    fi
    if [[ -z "${_registry}" ]]; then
      echo "argsh: unknown registry provider: ${_provider}" >&2
      return 1
    fi
  fi

  echo "${_registry}"
  echo "${_name}"
}

# @description Download a library via curl from GitHub releases (fallback).
# @arg $1 string Library name
# @arg $2 string Version tag
# @arg $3 string Destination directory
# @internal
argsh::lib::_curl_fallback() {
  local _name="${1}" _tag="${2}" _dest="${3}"
  local _normalized_tag="${_tag#v}"
  # Release tag format: <name>/v<version>, asset: <name>-<version>.tar.gz
  local _url="https://github.com/arg-sh/libs/releases/download/${_name}%2Fv${_normalized_tag}/${_name}-${_normalized_tag}.tar.gz"
  if [[ "${_tag}" == "latest" ]]; then
    # Per-lib releases: query GitHub API for latest release matching <name>/v*
    local _latest_tag
    # Use GITHUB_TOKEN/GH_TOKEN if available (avoids 60/hr anonymous rate limit)
    local -a _curl_args=(-fsSL)
    if [[ -n "${GITHUB_TOKEN:-${GH_TOKEN:-}}" ]]; then
      _curl_args+=(-H "Authorization: token ${GITHUB_TOKEN:-${GH_TOKEN}}")
    fi
    local _releases
    _releases="$(curl "${_curl_args[@]}" "https://api.github.com/repos/arg-sh/libs/releases?per_page=100")" || _releases=""
    _latest_tag="$(echo "${_releases}" | grep -o "\"tag_name\": *\"${_name}/v[^\"]*\"" | head -1 | cut -d'"' -f4)" || _latest_tag=""
    if [[ -z "${_latest_tag}" ]]; then
      echo "argsh: no release found for ${_name}" >&2
      return 1
    fi
    _normalized_tag="${_latest_tag#*/v}"
    _url="https://github.com/arg-sh/libs/releases/download/${_name}%2Fv${_normalized_tag}/${_name}-${_normalized_tag}.tar.gz"
  fi
  local _tmpfile; _tmpfile="$(mktemp)"
  if ! curl -fsSL "${_url}" -o "${_tmpfile}"; then
    echo "argsh: failed to download ${_name}" >&2
    rm -f "${_tmpfile}"
    return 1
  fi
  # Validate tarball: reject path traversal, symlinks, and hardlinks
  local _entry _unsafe=0
  while IFS= read -r _entry; do
    _entry="${_entry#./}"
    [[ -n "${_entry}" ]] || continue
    if [[ "${_entry}" == /* || "${_entry}" == ".." || "${_entry}" == ../* || "${_entry}" == */../* || "${_entry}" == */.. ]]; then
      _unsafe=1; break
    fi
  done < <(tar -tzf "${_tmpfile}" 2>/dev/null) || _unsafe=1
  # Reject symlinks and hardlinks
  if (( ! _unsafe )); then
    local _line
    while IFS= read -r _line; do
      case "${_line}" in
        l*|h*) _unsafe=1; break ;;
      esac
    done < <(tar -tvzf "${_tmpfile}" 2>/dev/null)
  fi
  if (( _unsafe )); then
    echo "argsh: tarball contains unsafe paths, refusing to extract" >&2
    rm -f "${_tmpfile}"
    return 1
  fi
  if ! tar xzf "${_tmpfile}" -C "${_dest}" --strip-components=1 --no-same-owner; then
    echo "argsh: failed to extract ${_name}" >&2
    rm -f "${_tmpfile}"
    return 1
  fi
  rm -f "${_tmpfile}"

  # Multi-arch .so selection: keep only matching platform, rename to canonical
  local _arch _libc="" _suffix
  _arch="$(argsh::builtin::arch 2>/dev/null)" || _arch=""
  if [[ -n "${_arch}" ]]; then
    command -v ldd &>/dev/null && ldd --version 2>&1 | grep -qi musl && _libc="-musl"
    _suffix="-linux${_libc}-${_arch}.so"
    local _so _canonical
    for _so in "${_dest}"/*-linux-*.so; do
      [[ -f "${_so}" ]] || continue
      if [[ "${_so}" == *"${_suffix}" ]]; then
        # Rename to canonical name (strip platform suffix)
        _canonical="${_so%"${_suffix}"}.so"
        mv "${_so}" "${_canonical}"
      else
        # Remove non-matching platform .so
        rm -f "${_so}"
      fi
    done
  fi
}

# @description Write or update a lockfile entry in .argsh.lock.
# Creates the lockfile with a header comment if it does not exist.
# @arg $1 string Entry key (e.g. argsh@data)
# @arg $2 string OCI reference (e.g. ghcr.io/arg-sh/libs/data:0.1.2)
# @arg $3 string Digest (e.g. sha256:abc123...)
# @internal
argsh::lib::_write_lock_entry() {
  local _key="${1}" _ref="${2}" _digest="${3}"
  if ! command -v yq &>/dev/null; then
    echo "argsh: warning: yq not found — lockfile not updated" >&2
    return 0
  fi
  local _dir="${PATH_BASE:-.}"
  local _lock="${_dir}/.argsh.lock"

  if [[ ! -f "${_lock}" ]]; then
    printf '%s\n' "# auto-generated by argsh — do not edit" "libs:" > "${_lock}"
  fi
  yq -i ".libs.\"${_key}\".ref = \"${_ref}\" | .libs.\"${_key}\".digest = \"${_digest}\"" "${_lock}"
}

# @description Remove a lockfile entry from .argsh.lock.
# @arg $1 string Entry key (e.g. argsh@data)
# @internal
argsh::lib::_remove_lock_entry() {
  local _key="${1}"
  local _dir="${PATH_BASE:-.}"
  local _lock="${_dir}/.argsh.lock"
  [[ -f "${_lock}" ]] || return 0
  command -v yq &>/dev/null || return 0
  yq -i "del(.libs.\"${_key}\")" "${_lock}"
}

# @description Add a plugin library to the project.
# @arg $1 string Lib reference (e.g. argsh@data, data, data@0.1.0)
# @arg --global Install to global libs directory instead of project-local
# @example
#   argsh lib add data
#   argsh lib add argsh@data
#   argsh lib add --global data
# @internal
argsh::lib::add() {
  local _global=0
  if [[ "${1:-}" == "--global" ]]; then
    _global=1; shift
  fi

  local _ref="${1:-}"
  [[ -n "${_ref}" ]] || { echo "argsh lib add: specify a library (e.g. argsh@data)" >&2; return 1; }

  local _version=""
  if [[ "${_ref}" == *@*@* ]]; then
    # provider@name@version
    _version="${_ref##*@}"
    _ref="${_ref%@*}"
  elif [[ "${_ref}" == *@* ]]; then
    # Could be name@version or provider@name
    local _after="${_ref#*@}"
    if [[ "${_after}" =~ ^v?[0-9] || "${_after}" == "latest" ]]; then
      # name@version (version starts with digit or v-prefix or "latest")
      _version="${_after}"
      _ref="${_ref%%@*}"
    fi
  fi

  local _registry _name
  { read -r _registry; read -r _name; } < <(argsh::lib::resolve "${_ref}") || {
    echo "argsh lib add: failed to resolve '${_ref}'" >&2; return 1
  }
  # Validate library name (prevent path traversal)
  if [[ ! "${_name}" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "argsh: invalid library name: ${_name}" >&2
    return 1
  fi

  local _tag="${_version:-latest}"
  local _oci_ref="${_registry}/${_name}:${_tag}"
  local _lib_dir
  if (( _global )); then
    _lib_dir="${__ARGSH_GLOBAL_LIBS}"
  else
    _lib_dir="$(argsh::lib::dir)"
  fi
  local _dest="${_lib_dir}/${_name}"

  echo "argsh: downloading ${_name} (${_tag})..." >&2

  mkdir -p "${_lib_dir}"
  local _tmp_dest _tmpfile=""
  _tmp_dest="$(mktemp -d "${_lib_dir}/.tmp.XXXXXX")"

  # Try OCI pull via Rust builtin (if loaded), fallback to curl/GitHub releases
  if [[ "$(type -t lib::pull 2>/dev/null)" == "builtin" ]]; then
    # Split registry into host and repo prefix (host-only if no /)
    local _host="${_registry%%/*}"
    local _repo_prefix="${_registry#*/}"
    [[ "${_host}" != "${_repo_prefix}" ]] || _repo_prefix=""
    local _repo="${_name}"
    [[ -z "${_repo_prefix}" ]] || _repo="${_repo_prefix}/${_name}"
    lib::pull "${_host}" "${_repo}" "${_tag}" "${_tmp_dest}" || {
      echo "argsh: OCI pull failed, trying GitHub releases fallback..." >&2
      rm -rf "${_tmp_dest}"; _tmp_dest="$(mktemp -d "${_lib_dir}/.tmp.XXXXXX")"
      argsh::lib::_curl_fallback "${_name}" "${_tag}" "${_tmp_dest}" || {
        rm -rf "${_tmp_dest}"; return 1
      }
    }
  else
    argsh::lib::_curl_fallback "${_name}" "${_tag}" "${_tmp_dest}" || {
      rm -rf "${_tmp_dest}"; return 1
    }
  fi

  # Atomic install: swap old aside, rename new in, then remove old
  if [[ -d "${_dest}" ]]; then
    local _old="${_dest}.old.$$"
    mv "${_dest}" "${_old}"
    if ! mv "${_tmp_dest}" "${_dest}"; then
      # Restore old on failure
      mv "${_old}" "${_dest}" 2>/dev/null || true
      rm -rf "${_tmp_dest}"
      echo "argsh: failed to install ${_name}" >&2
      return 1
    fi
    rm -rf "${_old}"
  else
    mv "${_tmp_dest}" "${_dest}"
  fi

  echo "argsh: installed ${_name} to ${_dest}" >&2

  # Skip manifest/lockfile updates for global installs
  if (( _global )); then
    return 0
  fi

  # Update .argsh.yaml if it exists
  local _dir="${PATH_BASE:-.}"
  if [[ -f "${_dir}/.argsh.yaml" ]]; then
    local _entry="${_ref}"
    [[ "${_ref}" == *@* ]] || _entry="argsh@${_ref}"
    # Add or update lib entry
    yq -i ".libs.\"${_entry}\" = \"${_version:-latest}\"" "${_dir}/.argsh.yaml" 2>/dev/null || true
  fi

  # Write lockfile entry
  local _digest="${__LIB_PULL_DIGEST:-}"
  if [[ -z "${_digest}" ]]; then
    # Compute digest from installed files (deterministic tar | sha256sum)
    local _sha_cmd="sha256sum"
    command -v sha256sum &>/dev/null || _sha_cmd="shasum -a 256"
    _digest="sha256:$(tar cf - -C "${_lib_dir}" "${_name}" 2>/dev/null | ${_sha_cmd} | cut -d' ' -f1)"
  fi
  local _lock_key="${_ref}"
  [[ "${_lock_key}" == *@* ]] || _lock_key="argsh@${_lock_key}"
  argsh::lib::_write_lock_entry "${_lock_key}" "${_oci_ref}" "${_digest}"
}

# @description List installed plugin libraries.
# @arg --global List from global libs directory
# @internal
argsh::lib::list() {
  local _lib_dir
  if [[ "${1:-}" == "--global" ]]; then
    _lib_dir="${__ARGSH_GLOBAL_LIBS}"
  else
    _lib_dir="$(argsh::lib::dir)"
  fi

  if [[ ! -d "${_lib_dir}" ]]; then
    echo "No libraries installed (${_lib_dir} not found)"
    return
  fi

  local _d
  for _d in "${_lib_dir}"/*/; do
    [[ -d "${_d}" ]] || continue
    local _name _version="?"
    _name="$(basename "${_d}")"
    if [[ -f "${_d}/argsh-plugin.yml" ]]; then
      _version="$(yq -r '.version // "?"' "${_d}/argsh-plugin.yml" 2>/dev/null)" || _version="?"
    fi
    echo "${_name} (${_version})"
  done
}

# @description Remove a plugin library.
# @arg $1 string Library name
# @arg --global Remove from global libs directory
# @internal
argsh::lib::remove() {
  local _global=0
  if [[ "${1:-}" == "--global" ]]; then
    _global=1; shift
  fi

  local _name="${1:-}"
  [[ -n "${_name}" ]] || { echo "argsh lib remove: specify a library name" >&2; return 1; }
  # Validate name (prevent path traversal via rm -rf)
  if [[ ! "${_name}" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "argsh: invalid library name: ${_name}" >&2
    return 1
  fi

  local _lib_dir
  if (( _global )); then
    _lib_dir="${__ARGSH_GLOBAL_LIBS}"
  else
    _lib_dir="$(argsh::lib::dir)"
  fi
  local _dest="${_lib_dir}/${_name}"

  if [[ ! -d "${_dest}" ]]; then
    echo "argsh: library '${_name}' not found in ${_lib_dir}" >&2
    return 1
  fi

  rm -rf "${_dest}"
  echo "argsh: removed ${_name}"

  # Remove lockfile entry (skip for global installs)
  if (( ! _global )); then
    # Try all common key patterns
    argsh::lib::_remove_lock_entry "${_name}"
    argsh::lib::_remove_lock_entry "argsh@${_name}"
  fi
}

# @description Install all libraries from .argsh.lock (exact refs) or .argsh.yaml.
# When a lockfile exists, each library is installed at the exact OCI ref recorded
# in the lock. Otherwise falls back to .argsh.yaml version ranges.
# @internal
argsh::lib::install() {
  local _dir="${PATH_BASE:-.}"

  command -v yq &>/dev/null || {
    echo "argsh: yq is required for lib install (https://github.com/mikefarah/yq)" >&2
    return 1
  }

  # Prefer lockfile for reproducible installs
  if [[ -f "${_dir}/.argsh.lock" ]]; then
    local _lib _oci_ref _failed=0
    while IFS='=' read -r _lib _oci_ref; do
      [[ -n "${_lib}" ]] || continue
      _oci_ref="${_oci_ref//\"/}"
      echo "Installing ${_lib} (locked: ${_oci_ref})..."
      # Extract version tag from OCI ref (registry/name:tag → tag)
      local _tag="${_oci_ref##*:}"
      local _ref="${_lib}"
      [[ -z "${_tag}" || "${_tag}" == "${_oci_ref}" ]] || _ref="${_lib}@${_tag}"
      argsh::lib::add "${_ref}" || { echo "argsh: failed to install ${_lib}" >&2; _failed=1; }
    done < <(yq -r '.libs // {} | to_entries[] | .key + "=" + .value.ref' "${_dir}/.argsh.lock" 2>/dev/null)
    return "${_failed}"
  fi

  if [[ ! -f "${_dir}/.argsh.yaml" ]]; then
    echo "argsh: no .argsh.yaml found" >&2
    return 1
  fi

  command -v yq &>/dev/null || {
    echo "argsh: yq is required for lib install (https://github.com/mikefarah/yq)" >&2
    return 1
  }

  local _lib _version _ref _failed=0
  while IFS='=' read -r _lib _version; do
    [[ -n "${_lib}" ]] || continue
    _version="${_version//\"/}"
    _version="${_version#^}"  # strip semver range prefix (v1: exact match only)
    _version="${_version#~}"
    # v1: semver ranges not resolved — uses stripped version as exact tag
    echo "Installing ${_lib} (${_version})..." >&2
    # Append version if available and not a range
    _ref="${_lib}"
    [[ -z "${_version}" || "${_version}" == "latest" ]] || _ref="${_lib}@${_version}"
    argsh::lib::add "${_ref}" || { echo "argsh: failed to install ${_lib}" >&2; _failed=1; }
  done < <(yq -r '.libs // {} | to_entries[] | .key + "=" + (.value | tostring)' "${_dir}/.argsh.yaml" 2>/dev/null)
  return "${_failed}"
}

# @description Update all libraries to their latest versions.
# Reads libs entries from .argsh.yaml and re-adds each without a pinned version,
# which fetches the latest tag. The lockfile is updated as a side-effect.
# @internal
argsh::lib::update() {
  local _dir="${PATH_BASE:-.}"
  if [[ ! -f "${_dir}/.argsh.yaml" ]]; then
    echo "argsh: no .argsh.yaml found" >&2
    return 1
  fi

  local _lib _failed=0
  while IFS='=' read -r _lib _; do
    [[ -n "${_lib}" ]] || continue
    echo "Updating ${_lib}..."
    argsh::lib::add "${_lib}" || { echo "argsh: failed to update ${_lib}" >&2; _failed=1; }
  done < <(yq -r '.libs // {} | to_entries[] | .key + "=" + .value' "${_dir}/.argsh.yaml" 2>/dev/null)
  return "${_failed}"
}

# @description Publish a plugin library to an OCI registry.
# Must be run from a directory containing argsh-plugin.yml.
# Requires the Rust builtin (lib::push) for OCI push support.
# @arg --registry string Override registry (default: from .argsh.yaml or ghcr.io/arg-sh/libs)
# @internal
argsh::lib::publish() {
  local _registry=""
  while [[ "${1:-}" == --* ]]; do
    case "${1}" in
      --registry) shift; _registry="${1:-}"; shift ;;
      *) echo "argsh lib publish: unknown option: ${1}" >&2; return 1 ;;
    esac
  done

  # Read argsh-plugin.yml
  if [[ ! -f "argsh-plugin.yml" ]]; then
    echo "argsh lib publish: no argsh-plugin.yml in current directory" >&2
    return 1
  fi

  local _name _version
  _name="$(yq -r '.name // ""' argsh-plugin.yml)"
  _version="$(yq -r '.version // ""' argsh-plugin.yml)"
  if [[ -z "${_name}" || -z "${_version}" ]]; then
    echo "argsh lib publish: argsh-plugin.yml must have 'name' and 'version' fields" >&2
    return 1
  fi

  # Validate name
  if [[ ! "${_name}" =~ ^[a-zA-Z0-9_-]+$ ]]; then
    echo "argsh: invalid library name: ${_name}" >&2
    return 1
  fi

  # Determine registry
  if [[ -z "${_registry}" ]]; then
    _registry="${__ARGSH_LIB_REGISTRY}"
  fi

  # Require builtin for OCI push
  if [[ "$(type -t lib::push 2>/dev/null)" != "builtin" ]]; then
    echo "argsh lib publish: requires argsh builtin (.so) with OCI push support" >&2
    echo "  Run: argsh builtin install" >&2
    return 1
  fi

  echo "argsh: publishing ${_name} v${_version} to ${_registry}..." >&2
  lib::push "${_registry}" "${_name}" "${_version}" "." || {
    echo "argsh lib publish: push failed" >&2
    return 1
  }

  echo "argsh: published ${_registry}/${_name}:${_version}" >&2
  echo "  digest: ${__LIB_PUSH_DIGEST:-unknown}" >&2
}

# @description Manage plugin libraries.
# @arg $1 string Subcommand: add, list, remove, install, update, publish
argsh::lib() {
  case "${1:-}" in
    add)     shift; argsh::lib::add "${@}" ;;
    list|ls) shift; argsh::lib::list "${@}" ;;
    remove)  shift; argsh::lib::remove "${@}" ;;
    install) shift; argsh::lib::install ;;
    update)  shift; argsh::lib::update ;;
    publish) shift; argsh::lib::publish "${@}" ;;
    "")
      argsh::lib::list
      echo ""
      echo "Usage: argsh lib [add|list|remove|install|update|publish] [--global]"
      ;;
    *)
      echo "argsh: unknown lib subcommand: ${1}" >&2
      echo "Usage: argsh lib [add|list|remove|install|update|publish] [--global]" >&2
      return 1
      ;;
  esac
}

# @description Discover search directories for scripts and tests.
# Uses PATH_TESTS (semicolon-separated), then common locations under PATH_BASE.
# @set _search_dirs array Directories to search (deduplicated)
# @internal
argsh::discover_dirs() {
  local -a _raw_dirs=()
  _search_dirs=()
  local _d _existing _skip _rd _re
  # PATH_TESTS: semicolon-separated list of directories
  if [[ -n "${PATH_TESTS:-}" ]]; then
    IFS=';' read -ra _raw_dirs <<< "${PATH_TESTS}"
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
  if (( ${ARGSH_BUILTIN:-0} )); then
    echo "  version:      ${__ARGSH_BUILTIN_VERSION:-unknown}"
    echo "  commit:       ${__ARGSH_BUILTIN_COMMIT:-unknown}"
  fi
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
  # Suppress binary::exists's own "docker is required" message — we print
  # a more contextual one below.
  binary::exists docker 2>/dev/null || {
    echo "argsh: this command requires either the tool installed locally or Docker" >&2
    return 1
  }
  # Only attach stdin (-i) when it's connected (TTY or pipe).
  # In non-TTY contexts (MCP subprocesses, some CI) stdin may be
  # /dev/null — passing -i there makes Docker hang waiting for EOF.
  local tty=""
  if [[ -t 0 ]]; then
    tty="-it"
  elif [[ -p /dev/stdin ]]; then
    tty="-i"
  fi
  local -r image="${ARGSH_DOCKER_IMAGE:-ghcr.io/arg-sh/argsh:latest}"
  # Collect user-defined env vars to forward: any exported ARGSH_ENV_FOO=bar
  # on the host is passed as FOO=bar inside the container.
  # Uses compgen -e (exported only) so unexported locals aren't leaked.
  # The docker run is wrapped in a subshell so that:
  #   - exports of the stripped names don't pollute the parent process env
  #   - read-only/special vars (UID, BASHOPTS, etc.) are safely skipped
  #   - secrets stay out of the process argv (passed via env, not -e NAME=value)
  # Collect candidate env vars to forward (name=value pairs).
  local -a _env_candidates=()
  local _var _name
  while IFS='=' read -r _var _; do
    _name="${_var#ARGSH_ENV_}"
    [[ -n "${_name}" ]] || continue
    [[ "${_name}" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
    _env_candidates+=("${_name}=${!_var}")
  done < <(compgen -e ARGSH_ENV_ 2>/dev/null || :)
  # Run docker in a subshell so exports don't pollute the parent env.
  # The -e flags are built AFTER export succeeds — if a name is
  # read-only (UID, BASHOPTS), the export fails silently and no -e
  # flag is added, so docker never receives a stale/empty value.
  # shellcheck disable=SC2046 disable=SC2030
  (
    local -a _docker_env_flags=()
    local _kv
    for _kv in "${_env_candidates[@]}"; do
      # shellcheck disable=SC2163
      if export "${_kv}" 2>/dev/null; then
        _docker_env_flags+=(-e "${_kv%%=*}")
      fi
    done
    docker run --rm ${tty} $(docker::user) \
      -e "BATS_LOAD" \
      -e "ARGSH_SOURCE" \
      -e "PATH_TESTS" \
      -e "PATH_SCRIPTS" \
      -e "GIT_COMMIT_SHA=$(git rev-parse HEAD 2>/dev/null || :)" \
      -e "GIT_VERSION=$(git describe --tags --dirty 2>/dev/null || :)" \
      "${_docker_env_flags[@]}" \
      "${image}" "${@}"
  )
}

# @description Minify Bash files into a single script.
# @arg $@ string Files or directories, plus flags (-t, -o, -i)
argsh::minify() {
  if ! binary::exists minifier 2>/dev/null; then
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
  # Run the rest in a subshell so the EXIT trap that cleans up temp files
  # is scoped to this function's invocation and does not clobber any
  # caller-installed EXIT trap. :args-set vars are inherited by the subshell.
  (
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
      exit 0
    fi
    binary::exists envsubst 2>/dev/null || {
      echo "argsh: envsubst is required for -t/--template (install gettext)" >&2
      exit 1
    }
    # obfus ignore variable
    local commit_sha="${GIT_COMMIT_SHA:-}"
    # obfus ignore variable
    local version="${GIT_VERSION:-}"
    export data commit_sha version
    # shellcheck disable=SC2016
    envsubst '$data,$commit_sha,$version' <"${template}" >"${out}"
  )
}

# @description Lint Bash files with shellcheck and argsh-lint.
#
# Runs both linters by default. Use --only-argsh to skip shellcheck and
# --only-shellcheck to skip argsh-lint. Exit code is 1 if any linter
# reports diagnostics.
#
# @arg $@ string Files or directories (optional; auto-discovered via PATH_TESTS)
argsh::lint() {
  # obfus ignore variable
  local only_argsh=0
  # obfus ignore variable
  local only_shellcheck=0
  # shellcheck disable=SC2034
  # obfus ignore variable
  local -a files args=(
    'only-argsh|:+'      "Skip shellcheck, run only argsh-lint"
    'only-shellcheck|:+' "Skip argsh-lint, run only shellcheck"
    'files'              "Files to lint, can be a glob pattern"
  )
  :args "Lint Bash files" "${@}"
  if (( only_argsh )) && (( only_shellcheck )); then
    echo "argsh lint: --only-argsh and --only-shellcheck are mutually exclusive" >&2
    return 2
  fi
  # Require shellcheck only when we actually need it — otherwise a caller
  # that runs `argsh lint --only-argsh` must not be forced to install it or
  # fall back to Docker.
  if (( ! only_argsh )) && ! binary::exists shellcheck 2>/dev/null; then
    argsh::_docker_forward lint "${@}"
    return
  fi
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
        # single-quoted regexes containing `|`). Covers direct paths
        # (/bin/bash, /bin/sh), env-based (env bash, env sh, env -S bash,
        # env -S sh), and argsh. Uses `case` to avoid the substring-match
        # pitfall where `*"/sh"*` misses `env sh` (no `/` before `sh`).
        local _shebang _is_shell=0
        _shebang="$(head -1 "${_f}" 2>/dev/null || :)"
        # Strip the "#!" prefix + optional "/usr/bin/env " + optional "-S ",
        # then the leading interpreter path/name should begin with bash/sh/argsh.
        local _interp="${_shebang#\#!}"
        _interp="${_interp# }"
        # If env-style, drop through "env" and optional "-S"
        case "${_interp}" in
          */env|*/env\ *)
            _interp="${_interp#*env}"
            _interp="${_interp# }"
            [[ "${_interp}" != -S* ]] || { _interp="${_interp#-S}"; _interp="${_interp# }"; }
            ;;
        esac
        # Now _interp should start with the shell name (possibly with path).
        # Take the basename (last path component) of the first whitespace-delimited token.
        local _first="${_interp%% *}"
        _first="${_first##*/}"
        case "${_first}" in
          bash|sh|argsh) _is_shell=1 ;;
        esac
        (( _is_shell )) && _found_files+=("${_f}")
      done
    done
    if (( ${#_found_files[@]} == 0 )); then
      echo "No files to lint (set PATH_TESTS or pass files as arguments)" >&2
      return 1
    fi
    # obfus ignore variable
    files=("${_found_files[@]}")
  fi

  # Expand globs/directories into a flat list of files to lint.
  local _file _f
  local -a _glob _expanded=()
  for _f in "${files[@]}"; do
    if [[ -d "${_f}" ]]; then
      _glob=("${_f}"/*.{sh,bash,bats})
    else
      # shellcheck disable=SC2206 disable=SC2128
      _glob=(${_f})
    fi
    for _file in "${_glob[@]}"; do
      [[ -e "${_file}" ]] || continue
      _expanded+=("${_file}")
    done
  done

  local _rc=0
  # Run shellcheck (skipped with --only-argsh).
  if (( ! only_argsh )); then
    for _file in "${_expanded[@]}"; do
      echo "Linting ${_file}" >&2
      shellcheck "${_file}" || _rc=1
    done
  fi
  # argsh-lint — static analysis of argsh-specific constructs (AG001-AG013).
  # If the binary isn't on PATH we skip silently unless the user explicitly
  # asked for it, matching the shellcheck auto-install behavior used above.
  if (( ! only_shellcheck )); then
    if binary::exists argsh-lint 2>/dev/null; then
      for _file in "${_expanded[@]}"; do
        echo "argsh-lint ${_file}" >&2
        argsh-lint "${_file}" || _rc=1
      done
    elif (( only_argsh )); then
      echo "argsh lint: argsh-lint binary not found on PATH" >&2
      return 1
    fi
  fi
  return "${_rc}"
}

# @description Run bats tests.
# @arg $@ string Paths to .bats files (optional; auto-discovered via PATH_TESTS)
argsh::test() {
  if ! binary::exists bats 2>/dev/null; then
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
      echo "No test files found (set PATH_TESTS or pass files as arguments)" >&2
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
  # Both kcov (run) and jq (report parse) are required locally; otherwise
  # forward to docker which has both.
  if ! binary::exists kcov 2>/dev/null || ! binary::exists jq 2>/dev/null; then
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
  (( coverage >= min )) || return 1
}

# @description Generate documentation for Bash libraries.
# @arg $1 string in — source files (glob ok)
# @arg $2 string out — output directory
# @arg $3 string prefix — optional prefix for each md file
argsh::docs() {
  if ! binary::exists shdoc 2>/dev/null; then
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
    'lib:-argsh::lib'            "Manage plugin libraries"
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

  # If the first arg looks like a script path (contains a slash or has a
  # shell extension) but the file doesn't exist, fail with a clear "file
  # not found" error instead of dispatching it as a subcommand (which
  # would give a confusing "Invalid command" / did-you-mean error).
  if [[ "${BASH_SOURCE[-1]}" != "${file}" && ! -f "${file}" ]]; then
    case "${file}" in
      */*|*.sh|*.bash)
        echo "argsh: file not found: ${file}" >&2
        return 1
        ;;
    esac
  fi

  # If first arg is not an existing file, treat it as a subcommand and
  # dispatch through argsh::main. This handles minify/lint/test/coverage/
  # docs/builtin/status/builtins uniformly (plus did-you-mean suggestions).
  #
  # Important: do NOT set ARGSH_SOURCE=<subcommand> here. It would propagate
  # through argsh::_docker_forward and cause the container's COMMANDNAME to
  # start as the subcommand (e.g. "test"), re-introducing the "test test ..."
  # usage rendering regression. Only set ARGSH_SOURCE when running a script
  # file.
  if [[ "${BASH_SOURCE[-1]}" == "${file}" || ! -f "${file}" ]]; then
    : "${ARGSH_SOURCE:=argsh}"
    export ARGSH_SOURCE
    # Backward-compat alias: "builtins" → "builtin"
    if [[ "${file}" == "builtins" ]]; then
      shift
      argsh::builtin "${@}"
      return
    fi
    argsh::main "${@}"
    return
  fi
  : "${ARGSH_SOURCE="${file}"}"
  export ARGSH_SOURCE
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
    # argsh disable=AG013
    import "${_lib}"
  done

  shift
  # shellcheck source=/dev/null
  . "${file}"
}