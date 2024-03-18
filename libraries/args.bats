#!/usr/bin/env bats
# shellcheck shell=bats

load ../test/helper
load_source

# Test function
:main() {
  local -a args arr1 arr2 arr3=("a" "b")
  local pos1 pos2 arg1="def" arg2 arg3 arg4 arg5 arg6 arg7 arg8 arg9
  args=(
    'pos1'            "Positional parameter 1"
    'pos2'            "Positional parameter 2"
    'pos3:~int'       "Positional parameter 3 with type"
    'arg1|'           "Argument 1 with default value"
    'arg2|2'          "Argument 2 with short option"
    'arg3|3:~int'     "Argument 3 with short option and type"
    'arg4|:~float'    "Argument 4 with type"
    'arg5|5:+'        "Argument 5 with short option and no value"
    'arg6|6:!'        "Argument 6 with short option and required"
    'arg7|7:~custom'  "Argument 7 with short option and custom type"
    'arg8|8:~string!' "Argument 8 with short option and type and required"
    'arg9|9:+!'       "Argument 9 with short option and no value and required"
    'arr1|'           "Array 1 with values"
    'arr2|:+'         "Array 2 with no value"
    'arr3|a'          "Array 3 with default values"
  )
  :args "This is a test" "${@}"
  :validate >&3
  echo "■■ positional parameters"
  echo "${pos1} ${pos2} ${pos3}"
  echo "■■ arguments"
  echo "${arg1} ${arg2:-} ${arg3:-} ${arg4:-} ${arg5:-} ${arg6:-} ${arg7:-} ${arg8:-} ${arg9:-}"
  echo "${arr1[@]:-}"
  echo "${arr2[@]:-}"
  echo "${arr3[@]}"
}

to::custom() {
  local value="$1"
  echo "${value} custom"
}

declare stdout stderr status
setup() {
  :validate() { :; }
  status=0
  stdout="$(mktemp)"
  stderr="$(mktemp)"
}
teardown() {
  rm -f "${stdout}" "${stderr}"
}

@test "no arguments" {
  (
    :main
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
  snapshot stderr
}

@test "-h, --help" {
  (
    :main -h
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout

  (
    :main --help
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "positional parameters (with required arguments)" {
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
    :main "pos1" "pos2" "3" --arg6 "string" --arg8 "string" --arg9
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "positional parameters (with all short options)" {
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
    :main "pos1" "pos2" "3" -2 "str1" -3 1 -5 -6 "req" -7 "cus" -8 "str1" -9 -a "arr1" -a "arr2"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "positional parameters (with all long options)" {
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
    :main "pos1" "pos2" "3" \
      --arg2 "str1" --arg3 1 --arg5 --arg6 "req" --arg7 "cus" --arg8 "str1" --arg9 \
      --arr1 "v1" --arr1 "v2" --arr2 --arr2 --arr2 --arr3 "b1" --arr3 "b2"
  ) >"${stdout}" 2>"${stderr}" 3>&2 || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "error: invalid type for positional parameter" {
  (
    :main "pos1" "pos2" "wrong" --arg6 "s" --arg8 "s" --arg9
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
  snapshot stderr
}

@test "error: invalid type for argument" {
  (
    :main "pos1" "pos2" "3" --arg6 "s" --arg8 "s" --arg9 --arg4 "wrong"
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  is_empty stdout
  snapshot stderr
}