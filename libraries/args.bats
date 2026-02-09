#!/usr/bin/env bats
# shellcheck disable=SC1091 disable=SC2154 disable=SC2317 disable=SC2329 disable=SC2034
# shellcheck shell=bats
#
# Shared tests for both pure-bash and native builtin implementations.
# Set ARGSH_BUILTIN_TEST=1 to test with Rust loadable builtins.

load ../test/helper
load_source

# Load native builtins when requested.
# All builtins are loaded from the same .so to ensure consistent coverage tracking.
declare -g __BUILTIN_SKIP=""
if [[ "${ARGSH_BUILTIN_TEST:-}" == "1" ]]; then
  _so="${BATS_TEST_DIRNAME}/../builtin/target/release/libargsh.so"
  if [[ ! -f "${_so}" ]]; then
    __BUILTIN_SKIP="builtin .so not found: ${_so}"
  else
    # shellcheck disable=SC2229
    enable -f "${_so}" \
      :usage :args is::array is::uninitialized is::set is::tty \
      args::field_name to::int to::float to::boolean to::file to::string \
      import import::clear 2>/dev/null || __BUILTIN_SKIP="builtin .so failed to load"
    if [[ -z "${__BUILTIN_SKIP}" ]]; then
      unset -f :usage :args \
        is::array is::uninitialized is::set is::tty \
        to::int to::float to::boolean to::file to::string \
        args::field_name import import::source import::clear 2>/dev/null || true
    fi
  fi
  unset _so
fi

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

@test "attrs: catch rest of positionals" {
  :validate() {
    assert "${pos1}" = "pos1"
    assert "${rest[*]}" = "a1 a2 a3 a4"
  }
  (
    :test::attrs2 "pos1" "a1" "a2" "a3" "a4"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
}

@test "attrs: catch rest of positionals is in help" {
  (
    :test::attrs2 --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
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
# Prefix resolution tests

@test "prefix: caller::func is preferred over bare func" {
  (
    :test::prefix start
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "prefix: caller::func resolved for stop" {
  (
    :test::prefix stop
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "prefix: nested caller resolution" {
  (
    :test::nested sub action
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "prefix: help still works" {
  (
    :test::prefix --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

# -----------------------------------------------------------------------------
# Now test rest of positional arguments
source "${PATH_FIXTURES}/rest.sh"

@test "rest: -h, --help" {
  for arg in -h --help; do
    (
      :test::rest "${arg}"
    ) >"${stdout}" 2>"${stderr}" || status=$?

    assert "${status}" -eq 0
    is_empty stderr
    snapshot stdout
  done
}

@test "rest: positional parameters" {
  :validate() {
    assert "${#all[@]}" -eq 5
    assert "${all[*]}" = "pos1 pos2 pos3 pos4 pos5"
  }
  (
    :test::rest "pos1" "pos2" "pos3" "pos4" "pos5"
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

# -----------------------------------------------------------------------------
# Direct to:: converter tests
# These exercise the standalone type converters (both bash and builtin modes).

@test "to: string identity" {
  result="$(to::string "hello world")"
  assert "${result}" = "hello world"
}

@test "to: int valid" {
  result="$(to::int "42")"
  assert "${result}" = "42"
  result="$(to::int "-99")"
  assert "${result}" = "-99"
  result="$(to::int "0")"
  assert "${result}" = "0"
}

@test "to: int invalid" {
  to::int "abc" >"${stdout}" 2>"${stderr}" || status=$?
  assert "${status}" -eq 1
  status=0
  to::int "12.34" >"${stdout}" 2>"${stderr}" || status=$?
  assert "${status}" -eq 1
  status=0
  to::int "" >"${stdout}" 2>"${stderr}" || status=$?
  assert "${status}" -eq 1
}

@test "to: float valid" {
  result="$(to::float "3.14")"
  assert "${result}" = "3.14"
  result="$(to::float "42")"
  assert "${result}" = "42"
  result="$(to::float "-1.5")"
  assert "${result}" = "-1.5"
  result="$(to::float "-99")"
  assert "${result}" = "-99"
}

@test "to: float invalid" {
  to::float "abc" >"${stdout}" 2>"${stderr}" || status=$?
  assert "${status}" -eq 1
  status=0
  to::float "" >"${stdout}" 2>"${stderr}" || status=$?
  assert "${status}" -eq 1
}

@test "to: boolean truthy" {
  result="$(to::boolean "true")"
  assert "${result}" = "1"
  result="$(to::boolean "yes")"
  assert "${result}" = "1"
  result="$(to::boolean "1")"
  assert "${result}" = "1"
  result="$(to::boolean "anything")"
  assert "${result}" = "1"
}

@test "to: boolean falsy" {
  result="$(to::boolean "")"
  assert "${result}" = "0"
  result="$(to::boolean "false")"
  assert "${result}" = "0"
  result="$(to::boolean "0")"
  assert "${result}" = "0"
}

@test "to: file valid" {
  local tmpfile
  tmpfile="$(mktemp)"
  result="$(to::file "${tmpfile}")"
  assert "${result}" = "${tmpfile}"
  rm -f "${tmpfile}"
}

@test "to: file invalid" {
  to::file "/nonexistent/path/file.txt" >"${stdout}" 2>"${stderr}" || status=$?
  assert "${status}" -eq 1
}

# -----------------------------------------------------------------------------
# Direct is:: introspection tests

@test "is: array true" {
  local -a myarr=("a" "b")
  is::array myarr
}

@test "is: array false for scalar" {
  local myvar="hello"
  is::array myvar && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "is: array false for nonexistent" {
  is::array _nonexistent_var_xyz_ && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "is: array empty array" {
  local -a empty_arr
  is::array empty_arr
}

@test "is: uninitialized scalar" {
  local myvar
  is::uninitialized myvar
}

@test "is: uninitialized false for set scalar" {
  local myvar="hello"
  is::uninitialized myvar && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "is: uninitialized empty array" {
  local -a myarr
  is::uninitialized myarr
}

@test "is: uninitialized false for populated array" {
  local -a myarr=("x")
  is::uninitialized myarr && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "is: set true" {
  local myvar="hello"
  is::set myvar
}

@test "is: set false for uninitialized" {
  local myvar
  is::set myvar && status=0 || status=$?
  assert "${status}" -eq 1
}

@test "is: tty false in test" {
  # stdout is redirected in tests, so is::tty should return 1
  is::tty && status=0 || status=$?
  assert "${status}" -eq 1
}

# -----------------------------------------------------------------------------
# Additional :args edge cases

@test "attrs: flag with equals syntax" {
  :validate() {
    assert "${pos1}" = "pos1"
    assert "${pos2}" = "pos2"
    assert "${pos3}" = "3"
    assert "${arg2}" = "val1"
    assert "${arg6}" = "req"
    assert "${arg8}" = "str1"
    assert "${arg9}" = "1"
  }
  (
    :test::attrs "pos1" "pos2" "3" --arg2=val1 --arg6 "req" --arg8 "str1" --arg9
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
}

@test "attrs: error: missing required flag" {
  (
    :test::attrs "pos1" "pos2" "3" --arg6 "s"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
}

@test "attrs: error: unknown flag" {
  (
    :test::attrs "pos1" "pos2" "3" --arg6 "s" --arg8 "s" --arg9 --nonexistent "x"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
}

@test "attrs: error: too many positional arguments" {
  (
    :test::attrs2 "pos1" "a1" "a2" "a3" "a4" --nonexistent "x"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
}

# -----------------------------------------------------------------------------
# Additional :usage edge cases

@test "usage: error: unknown subcommand" {
  (
    :test::usage nonexistent
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
}

# -----------------------------------------------------------------------------
# Internal type conversion via :args (exercises field::convert_type)

@test "types: float via :args" {
  :validate() {
    assert "${val}" = "3.14"
  }
  (
    :test::types_float "3.14"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
}

@test "types: negative float via :args flag" {
  :validate() {
    assert "${val}" = "-2.5"
  }
  (
    :test::types_float_flag --val "-2.5"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
}

@test "types: integer-only float via :args" {
  :validate() {
    assert "${val}" = "42"
  }
  (
    :test::types_float "42"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
}

@test "types: file via :args" {
  local tmpfile
  tmpfile="$(mktemp)"
  :validate() {
    assert "${val}" = "${tmpfile}"
  }
  (
    :test::types_file "${tmpfile}"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?
  rm -f "${tmpfile}"

  assert "${status}" -eq 0
  is_empty stderr
}

@test "types: file error via :args" {
  (
    :test::types_file "/nonexistent/file.txt"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
}

@test "types: valid float arg4 with equals" {
  :validate() {
    assert "${pos1}" = "p1"
    assert "${pos2}" = "p2"
    assert "${pos3}" = "1"
    assert "${arg4}" = "9.99"
    assert "${arg6}" = "s"
    assert "${arg8}" = "s"
    assert "${arg9}" = "1"
  }
  (
    :test::attrs "p1" "p2" "1" --arg4=9.99 --arg6 "s" --arg8 "s" --arg9
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
}

# -----------------------------------------------------------------------------
# args::field_name builtin

@test "args::field_name extracts name" {
  result="$(args::field_name "flag|f:~int!")"
  assert "${result}" = "flag"
}

@test "args::field_name with dashes" {
  result="$(args::field_name "my-flag|m")"
  assert "${result}" = "my_flag"
}

@test "args::field_name with hidden prefix" {
  result="$(args::field_name "#hidden|h")"
  assert "${result}" = "hidden"
}

# -----------------------------------------------------------------------------
# Additional :args error edge cases

@test "attrs: error: too many positionals (no rest array)" {
  (
    :test::attrs "pos1" "pos2" "3" --arg6 "s" --arg8 "s" --arg9 "extra"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
}

@test "usage: long boolean flag --verbose" {
  (
    :test::usage --verbose cmd2
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "usage: long flag with equals syntax" {
  (
    :test::usage --config=custom.yml cmd2
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "usage: short flag with separate value" {
  (
    :test::usage -f separate.yml cmd2
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "attrs: short flag with inline value" {
  :validate() {
    assert "${pos1}" = "p1"
    assert "${pos2}" = "p2"
    assert "${pos3}" = "1"
    assert "${arg2}" = "inline"
    assert "${arg6}" = "s"
    assert "${arg8}" = "s"
    assert "${arg9}" = "1"
  }
  (
    :test::attrs "p1" "p2" "1" -2inline --arg6 "s" --arg8 "s" --arg9
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
}