#!/usr/bin/env bats
# shellcheck shell=bash disable=SC2154
# vim: filetype=bash
set -euo pipefail

load ../test/helper
load_source

@test "can import library" {
  (
    import "string"
    string::random
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  if [[ "${BATS_LOAD}" == "argsh.min.sh" ]]; then
    assert "${status}" -eq 1
    is_empty stdout
    contains "^Library not found argsh.min.sh/string" stderr
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
    unset ARGSH_SOURCE
    import "~bats-format-cat"
  ) < <(echo "some input") >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  is_empty stderr   
  contains "^some input" stdout
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
