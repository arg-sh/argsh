#!/usr/bin/env bats
# shellcheck shell=bash disable=SC1091 disable=SC2154 disable=SC2034 disable=SC2030 disable=SC2031
# vim: filetype=bash
# NOTE: do NOT add set -euo pipefail — it breaks bats internals (BATS_TEARDOWN_STARTED unbound)
# argsh disable-file=AG013
# (This file's tests deliberately use fake module names with @, ^, and ~
# prefixes to exercise import::source's error paths. They are not real
# imports — argsh-lint would flag every one with AG013.)

load ../test/helper
load_source

# Force pure-bash mode when requested (skip in minified mode).
if [[ "${ARGSH_PURE_BASH_TEST:-}" == "1" && "${BATS_LOAD:-}" != "argsh.min.sh" ]]; then
  for _b in import import::clear; do
    enable -d "${_b}" 2>/dev/null || true
  done
  # shellcheck disable=SC1091
  source "${BATS_TEST_DIRNAME}/import.sh" 2>/dev/null
fi

# Load native builtins when requested.
declare -g __BUILTIN_SKIP=""
if [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]]; then
  if [[ "$(type -t import 2>/dev/null)" == "builtin" ]]; then
    # Builtins already loaded (e.g., via ARGSH_BUILTIN_PATH in Docker)
    unset -f import import::source import::clear 2>/dev/null || true
  else
    _so="${BATS_TEST_DIRNAME}/../builtin/target/release/libargsh.so"
    [[ -f "${_so}" ]] || _so="${ARGSH_BUILTIN_PATH:-}"
    if [[ ! -f "${_so}" ]]; then
      echo "ERROR: builtin .so not found: ${_so}" >&2
      exit 1
    fi
    # shellcheck disable=SC2229
    if ! enable -f "${_so}" import import::clear; then
      echo "ERROR: builtin .so failed to load: ${_so}" >&2
      exit 1
    fi
    unset -f import import::source import::clear 2>/dev/null || true
    unset _so
  fi
fi

# -----------------------------------------------------------------------------
# Existing tests

@test "can import library" {
  # 3>&- prevents bats 1.11+ hang: forked pipeline processes (tr < /dev/urandom
  # inside string::random) inherit bats' FD3 output-capture descriptor and hold
  # it open until SIGPIPE propagates, blocking bats indefinitely.
  (
    unset ARGSH_SOURCE
    import "string"
    string::random
  ) >"${stdout}" 2>"${stderr}" 3>&- || status="${?}"

  if [[ "${BATS_LOAD}" == "argsh.min.sh" ]]; then
    # argsh.min.sh: functions already inlined, import warns but succeeds
    assert "${status}" -eq 0
    not_empty stdout
    contains "Library not found" stderr
  else
    # Both pure-bash and builtin resolve relative imports via __ARGSH_LIB_DIR
    assert "${status}" -eq 0
    is_empty stderr
    not_empty stdout
  fi
}

@test "can import from @" {
  (
    # shellcheck disable=SC2030
    export PATH_BASE="${PATH_FIXTURES}"
    import "@print_out"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^out" stdout
}

@test "can import from ~" {
  (
    # ~ resolves relative to ARGSH_SOURCE (or BASH_SOURCE[-1])
    export ARGSH_SOURCE="${PATH_FIXTURES}/entry"
    import "~print_out"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^out" stdout
}

@test "using import cache and can clear it" {
  (
    # shellcheck disable=SC2031
    export PATH_BASE="${PATH_FIXTURES}"
    import "@print_out"
    import "@print_out"
    import::clear
    import "@print_out"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^out\nout\n$" stdout
}

@test "errors when importing non-existent library" {
  (
    import "non-existent"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 1
  is_empty stdout
  contains "^Library not found" stderr
}

# -----------------------------------------------------------------------------
# Selective import tests (builtin-only features)

@test "selective: import single function" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import func_alpha "@multi_func"
    func_alpha
    declare -F func_beta && echo "LEAKED" || echo "CLEAN"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^alpha" stdout
  contains "CLEAN" stdout
}

@test "selective: import multiple functions" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import func_alpha func_gamma "@multi_func"
    func_alpha
    func_gamma
    declare -F func_beta && echo "LEAKED" || echo "CLEAN"
    declare -F func_delta && echo "LEAKED" || echo "CLEAN2"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^alpha" stdout
  contains "gamma" stdout
  contains "CLEAN" stdout
  contains "CLEAN2" stdout
}

@test "selective: error on non-existent function" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import nonexistent_func "@multi_func"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 1
  is_empty stdout
  contains "not found in module" stderr
}

@test "selective: cleans up on error" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import nonexistent_func "@multi_func" || true
    # All functions from multi_func should be removed
    declare -F func_alpha && echo "LEAKED" || echo "CLEAN"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  contains "CLEAN" stdout
}

# -----------------------------------------------------------------------------
# Aliasing tests

@test "alias: import with alias" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import "original_func as my_func" "@alias_test"
    my_func "hello"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^original: hello" stdout
}

@test "alias: mixed selective and alias" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import "original_func as renamed" another_func "@alias_test"
    renamed "test"
    another_func "test2"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^original: test" stdout
  contains "another: test2" stdout
}

# -----------------------------------------------------------------------------
# --force flag tests

@test "force: re-imports when --force is used" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import "@print_out"
    import "@print_out"
    import --force "@print_out"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^out\nout\n$" stdout
}

# -----------------------------------------------------------------------------
# --list flag tests

@test "list: shows cached modules" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    declare -gA import_cache=()
    import "@print_out"
    import --list
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "@print_out" stdout
}

@test "list: empty when nothing imported" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    declare -gA import_cache=()
    import --list
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  is_empty stdout
}

# -----------------------------------------------------------------------------
# Cache interaction

@test "selective: second selective import bypasses cache" {
  [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]] || return 0
  (
    export PATH_BASE="${PATH_FIXTURES}"
    # First selective import: caches the module, keeps only func_alpha
    import func_alpha "@multi_func"
    func_alpha
    # Second selective import of same module with different function —
    # without cache bypass this would silently skip (func_beta missing).
    import func_beta "@multi_func"
    func_beta
    # Both should be available
    func_alpha
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "alpha" stdout
  contains "beta" stdout
}

@test "can import from ^" {
  (
    export PATH_SCRIPTS="${PATH_FIXTURES}/scripts"
    import "^caret_lib"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  contains "^caret" stdout
}

@test "cache prevents re-sourcing" {
  (
    export PATH_BASE="${PATH_FIXTURES}"
    import "@print_out"
    import "@print_out"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr
  # Should only print "out" once (second import is cached)
  contains "^out\n$" stdout
}

# -----------------------------------------------------------------------------
# ARGSH_DEBUG tests

@test "debug: import shows trace when ARGSH_DEBUG=1" {
  # Debug output is stripped in minified mode
  [[ "${BATS_LOAD}" != "argsh.min.sh" ]] || return 0
  (
    unset ARGSH_SOURCE
    export ARGSH_DEBUG=1
    import "string"
  ) >"${stdout}" 2>"${stderr}" 3>&- || status="${?}"

  assert "${status}" -eq 0
  contains "argsh:debug:" stderr
}

@test "debug: import silent when ARGSH_DEBUG unset" {
  # Minified mode: all libs are inlined, no standalone files to import
  [[ "${BATS_LOAD}" != "argsh.min.sh" ]] || return 0
  (
    unset ARGSH_SOURCE
    import "string"
  ) >"${stdout}" 2>"${stderr}" 3>&- || status="${?}"

  assert "${status}" -eq 0
  ! command grep -q "argsh:debug:" "${stderr}" || {
    echo "Debug output should not appear without ARGSH_DEBUG=1"
    return 1
  }
}

# ── @ fallback to git root ──────────────────────────────

@test "import: @ falls back to git root when PATH_BASE unset" {
  if [[ -n "${__BUILTIN_SKIP}" ]]; then skip "${__BUILTIN_SKIP}"; fi
  command -v git &>/dev/null || skip "git not available"
  local _tmp
  _tmp="$(mktemp -d)"
  mkdir -p "${_tmp}/libs"
  echo 'test_at_fallback() { echo "at-fallback-ok"; }' > "${_tmp}/libs/helper.sh"
  git -C "${_tmp}" init -q 2>"${stderr}" || {
    echo "git init failed:" >&2; cat "${stderr}" >&2; return 1
  }
  # Mark temp dir as safe (required for git 2.35.2+ in containers)
  git config --global --add safe.directory "${_tmp}" 2>/dev/null || true
  cat > "${_tmp}/run.sh" <<'SCRIPT'
#!/usr/bin/env bash
import @libs/helper
test_at_fallback
SCRIPT
  chmod +x "${_tmp}/run.sh"

  (
    unset PATH_BASE 2>/dev/null || true
    cd "${_tmp}" || exit 1
    echo "bash-PATH_BASE=[${PATH_BASE:-UNSET}]" >&2
    echo "bash-env-PATH_BASE=[$(env | grep ^PATH_BASE= || echo UNSET)]" >&2
    ARGSH_SOURCE="${_tmp}/run.sh" source "${_tmp}/run.sh"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  if [[ "${status:-0}" -ne 0 ]]; then
    cat "${stderr}" >&2
  fi
  assert "${status}" -eq 0
  contains "at-fallback-ok" stdout

  rm -rf "${_tmp}"
}

# ── ^ walk-up fallback ──────────────────────────────────

@test "import: ^ walks up from script dir when PATH_SCRIPTS unset" {
  if [[ -n "${__BUILTIN_SKIP}" ]]; then skip "${__BUILTIN_SKIP}"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  # Create: root/utils/verbose.sh and root/sub/deep/script.sh
  mkdir -p "${_tmp}/utils" "${_tmp}/sub/deep"
  # Init git repo to bound walk-up (prevents escaping to host filesystem)
  command -v git &>/dev/null && git -C "${_tmp}" init -q 2>/dev/null || true
  echo 'test_walkup() { echo "walkup-ok"; }' > "${_tmp}/utils/verbose.sh"
  cat > "${_tmp}/sub/deep/run.sh" <<'SCRIPT'
#!/usr/bin/env bash
import ^utils/verbose
test_walkup
SCRIPT
  chmod +x "${_tmp}/sub/deep/run.sh"

  (
    unset PATH_SCRIPTS 2>/dev/null || true
    cd "${_tmp}/sub/deep" || exit 1
    ARGSH_SOURCE="${_tmp}/sub/deep/run.sh" source "${_tmp}/sub/deep/run.sh"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "walkup-ok" stdout

  rm -rf "${_tmp}"
}

# ── # argsh source= directive ──────────────────────────

@test "import: ^ uses # argsh source= directive" {
  if [[ -n "${__BUILTIN_SKIP}" ]]; then skip "${__BUILTIN_SKIP}"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  mkdir -p "${_tmp}/libs/utils"
  echo 'test_directive() { echo "directive-ok"; }' > "${_tmp}/libs/utils/verbose.sh"
  # Script with directive pointing to libs/
  cat > "${_tmp}/run.sh" <<SCRIPT
#!/usr/bin/env bash
# argsh source=${_tmp}/libs
import ^utils/verbose
test_directive
SCRIPT
  chmod +x "${_tmp}/run.sh"

  (
    unset PATH_SCRIPTS 2>/dev/null || true
    ARGSH_SOURCE="${_tmp}/run.sh" source "${_tmp}/run.sh"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "directive-ok" stdout

  rm -rf "${_tmp}"
}

