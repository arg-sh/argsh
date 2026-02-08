#!/usr/bin/env bash
set -euo pipefail

:test::attrs() {
  local -a args arr1 arr2 arr3=("a" "b")
  local pos1 pos2 pos3 arg1="def" arg2 arg3 arg4 arg5 arg6 arg7 arg8 arg9
  # shellcheck disable=SC2034
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

:test::types_float() {
  local val
  # shellcheck disable=SC2034
  local -a args=(
    'val:~float' "A float value"
  )
  :args "Float type test" "${@}"
  :validate >&3
}

:test::types_float_flag() {
  local val
  # shellcheck disable=SC2034
  local -a args=(
    'val|:~float' "A float value flag"
  )
  :args "Float flag type test" "${@}"
  :validate >&3
}

:test::types_file() {
  local val
  # shellcheck disable=SC2034
  local -a args=(
    'val:~file' "A file path"
  )
  :args "File type test" "${@}"
  :validate >&3
}

:test::attrs2() {
  local pos1
  # shellcheck disable=SC2034
  local -a rest args=(
    'pos1'    "Positional parameter 1"
    'rest'    "Rest of the parameters"
  )
  :args "Description of cmd4" "${@}"
  echo "cmd4"
  echo "pos1: ${pos1:-}"
  echo "rest: ${rest[*]:-}"
}