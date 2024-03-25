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

@test "random" {
  result="$(string::random)"
  assert "${#result}" -eq 42
}

@test "random with length" {
  result="$(string::random 10)"
  assert "${#result}" -eq 10
}

@test "random with length and characters" {
  result="$(string::random 10 "abc")"
  assert "${#result}" -eq 10
  assert "" = "${result//[abc]/}"
}

@test "drop-index" {
  result="$(string::drop-index "hello" 1 2)"
  assert "hlo" = "${result}"
}