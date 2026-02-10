#!/usr/bin/env bats
# shellcheck shell=bash disable=SC2154
# vim: filetype=bash
set -euo pipefail

load ../test/helper
load_source

@test "trim-left" {
  result="$(string::trim-left "  foo")"
  assert "foo" = "${result}"
}

@test "trim-left with custom character" {
  result="$(string::trim-left " x  foo" "x ")"
  assert "foo" = "${result}"
}

@test "trim-right" {
  result="$(string::trim-right "foo  ")"
  assert "foo" = "${result}"
}

@test "trim-right with custom character" {
  result="$(string::trim-right "foo x " "x ")"
  assert "foo" = "${result}"
}

@test "trim" {
  result="$(string::trim "  foo  ")"
  assert "foo" = "${result}"
}

@test "trim with custom character" {
  result="$(string::trim " x  foo x " "x ")"
  assert "foo" = "${result}"
}

# 3>&- on /dev/urandom pipelines: prevents bats 1.11+ hang where forked
# pipeline processes inherit bats' FD3 output-capture descriptor.

@test "random" {
  result="$(string::random 3>&-)"
  assert "${#result}" -eq 42
  # Must start with a letter (not a digit)
  [[ "${result}" =~ ^[[:alpha:]] ]]
}

@test "random with length" {
  result="$(string::random 10 3>&-)"
  assert "${#result}" -eq 10
  [[ "${result}" =~ ^[[:alpha:]] ]]
}

@test "random with length and characters" {
  result="$(string::random 10 "abc" 3>&-)"
  assert "${#result}" -eq 10
  assert "" = "${result//[abc]/}"
}

@test "drop-index" {
  result="$(string::drop-index "hello" 1 2)"
  assert "hlo" = "${result}"
}

@test "drop-index default length" {
  result="$(string::drop-index "hello" 2)"
  assert "helo" = "${result}"
}

@test "trim-left empty string" {
  result="$(string::trim-left "")"
  assert "" = "${result}"
}

@test "trim all whitespace" {
  result="$(string::trim "   ")"
  assert "" = "${result}"
}

@test "indent with indentation" {
  result="$(string::indent "  hello" 4)"
  assert "    hello" = "${result}"
}

# -----------------------------------------------------------------------------
# array:: function tests
# shellcheck disable=SC1091
source "${BATS_TEST_DIRNAME}/array.sh"

@test "array: contains found" {
  local -a arr=("a" "b" "c")
  array::contains "b" "${arr[@]}"
}

@test "array: contains not found" {
  local -a arr=("a" "b" "c")
  array::contains "z" "${arr[@]}" && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "array: contains empty array" {
  array::contains "a" && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "array: join" {
  local -a arr=("a" "b" "c")
  result="$(array::join "," "${arr[@]}")"
  assert "a,b,c" = "${result}"
}

@test "array: join single element" {
  result="$(array::join "," "only")"
  assert "only" = "${result}"
}

@test "array: nth" {
  local -a arr=("a" "b" "c" "d" "e" "f") result_arr=()
  array::nth result_arr 2 "${arr[@]}"
  assert "b d f" = "${result_arr[*]}"
}

# -----------------------------------------------------------------------------
# bash::version tests
# shellcheck disable=SC1091
source "${BATS_TEST_DIRNAME}/bash.sh"

@test "bash: version current passes" {
  bash::version "${BASH_VERSINFO[0]}" "${BASH_VERSINFO[1]}" "${BASH_VERSINFO[2]}"
}

@test "bash: version higher major fails" {
  bash::version 99 0 0 && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "bash: version lower major passes" {
  bash::version 1 0 0
}