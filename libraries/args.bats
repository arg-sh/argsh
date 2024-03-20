#!/usr/bin/env bats
# shellcheck shell=bats

load ../test/helper
load_source

# -----------------------------------------------------------------------------
# First test the attributes
source "${PATH_FIXTURES}/attrs.sh"

@test "attrs: no arguments" {
  (
    :test::attrs
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
  snapshot stderr
}

@test "attrs: -h, --help" {
  (
    :test::attrs -h
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout

  (
    :test::attrs --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "attrs: positional parameters (with required arguments)" {
  :validate() {
    assert "${pos1}" = "pos1"
    assert "${pos2}" = "pos2"
    assert "${pos3}" = "3"
    assert "${arg1}" = "def"
    assert "${arg6}" = "string"
    assert "${arg8}" = "string"
    assert "${arg9}" = "1"
    assert "${arr3[*]}" = "a b"
    is::uninitialized arg2 arg3 arg4 arg5 arg7 arr1 arr2
  }
  (
    :test::attrs "pos1" "pos2" "3" --arg6 "string" --arg8 "string" --arg9
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "attrs: positional parameters (with all short options)" {
  :validate() {
    assert "${pos1}" = "pos1"
    assert "${pos2}" = "pos2"
    assert "${pos3}" = "3"
    assert "${arg1}" = "def"
    assert "${arg2}" = "str1"
    assert "${arg3}" = "1"
    assert "${arg5}" = "1"
    assert "${arg6}" = "req"
    assert "${arg7}" = "cus custom"
    assert "${arg8}" = "str1"
    assert "${arg9[*]}" = "1"
  }
  (
    :test::attrs "pos1" "pos2" "3" -2 "str1" -3 1 -5 -6 "req" -7 "cus" -8 "str1" -9 -a "arr1" -a "arr2"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "attrs: positional parameters (with all long options)" {
  :validate() {
    assert "${pos1}" = "pos1"
    assert "${pos2}" = "pos2"
    assert "${pos3}" = "3"
    assert "${arg1}" = "def"
    assert "${arg2}" = "str1"
    assert "${arg3}" = "1"
    assert "${arg5}" = "1"
    assert "${arg6}" = "req"
    assert "${arg7}" = "cus custom"
    assert "${arg8}" = "str1"
    assert "${arg9}" = "1"
    assert "${arr1[*]}" = "v1 v2"
    assert "${arr2[*]}" = "1 1 1"
    assert "${arr3[*]}" = "a b b1 b2"
  }
  (
    :test::attrs "pos1" "pos2" "3" \
      --arg2 "str1" --arg3 1 --arg5 --arg6 "req" --arg7 "cus" --arg8 "str1" --arg9 \
      --arr1 "v1" --arr1 "v2" --arr2 --arr2 --arr2 --arr3 "b1" --arr3 "b2"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "attrs: error: invalid type for positional parameter" {
  (
    :test::attrs "pos1" "pos2" "wrong" --arg6 "s" --arg8 "s" --arg9
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
  snapshot stderr
}

@test "attrs: error: invalid type for argument" {
  (
    :test::attrs "pos1" "pos2" "3" --arg6 "s" --arg8 "s" --arg9 --arg4 "wrong"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
  snapshot stderr
}

# -----------------------------------------------------------------------------
# Now test usage
source "${PATH_FIXTURES}/usage.sh"

@test "usage: no arguments, -h, --help" {
  (
    :test::usage
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout

  (
    :test::usage -h
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout

  (
    :test::usage --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "usage: calling subcommand cmd1 and alias" {
  (
    :test::usage cmd1 
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout

  (
    :test::usage alias
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "usage: calling subcommand cmd1 with flag" {
  (
    :test::usage cmd1 --config ./config.yml
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout

  (
    :test::usage --config ./config.yml cmd1
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "usage: calling sub subcommand with flags" {
  (
    :test::usage -ftest -v cmd1 -v subcmd1 -vv --flag2 juhu
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "usage: calling cmd2" {
  (
    :test::usage cmd2 -vvv --config wrong.yml
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "usage: calling hidden cmd3" {
  (
    :test::usage cmd3 -vvv --config wrong.yml
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

# -----------------------------------------------------------------------------
# Now test format stuff
source "${PATH_FIXTURES}/fmt.sh"

@test "fmt: usage: top level group" {
  (
    :test::fmt1 --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "fmt: args: top level group" {
  (
    :test::fmt1 cmd1 --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "fmt: usage: group" {
  (
    :test::fmt2 --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "fmt: args: group" {
  (
    :test::fmt2 cmd1 --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}