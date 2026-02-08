#!/usr/bin/env bats
# shellcheck shell=bash disable=SC2154
# vim: filetype=bash
# NOTE: do NOT add set -euo pipefail — it breaks bats internals (BATS_TEARDOWN_STARTED unbound)

load ../test/helper
load_source

# Load native builtins when requested.
if [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]]; then
  # shellcheck disable=SC2229
  enable -f "${BATS_TEST_DIRNAME}/../builtin/target/release/libargsh.so" \
    import import::clear
  unset -f import import::source import::clear 2>/dev/null || true
fi

# -----------------------------------------------------------------------------
# Existing tests

@test "can import library" {
  (
    unset ARGSH_SOURCE
    import "string"
    string::random
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  if [[ "${ARGSH_BUILTIN_TEST:-}" == "1" || "${BATS_LOAD}" == "argsh.min.sh" ]]; then
    # Builtin: BASH_SOURCE[0] is the caller, not import.sh, so relative resolve fails.
    # argsh.min.sh: stripped bundle can't find separate library files.
    assert "${status}" -ne 0
    is_empty stdout
    contains "Library not found" stderr
  else
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
