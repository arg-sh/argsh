#!/usr/bin/env bats
# shellcheck shell=bash disable=SC2154 disable=SC2034 disable=SC2030 disable=SC2031
# vim: filetype=bash
# NOTE: do NOT add set -euo pipefail — it breaks bats internals (BATS_TEARDOWN_STARTED unbound)

load ../test/helper
load_source

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
      __BUILTIN_SKIP="builtin .so not found"
    else
      # shellcheck disable=SC2229
      enable -f "${_so}" import import::clear 2>/dev/null || __BUILTIN_SKIP="builtin .so failed to load"
      if [[ -z "${__BUILTIN_SKIP}" ]]; then
        unset -f import import::source import::clear 2>/dev/null || true
      fi
    fi
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
