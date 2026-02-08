#!/usr/bin/env bash
# @file to
# @brief Convert a value to a specific type
# @tags core, builtin
# @description
#   This file contains functions for converting a value to a specific type
set -euo pipefail

# @description Convert a value to a string
# @arg $1 any value
# @stdout The value as a string
# @example
#   to::string 1 # 1
to::string() {
  local value="${1}"
  echo "${value}"
}

# @description Convert a value to a boolean
# @arg $1 any value
# @stdout The value as a boolean
# @example
#   to::boolean "true"  # 1
#   to::boolean "false" # 0
#   to::boolean "hi"    # 1
to::boolean() {
  local value="${1}"
  case "${value}" in
    ""|"false"|"0") value="0" ;;
    *) value="1" ;;
  esac
  echo "${value}"
}

# @description Convert a value to an integer
# @arg $1 any value
# @stdout The value as an integer
# @exitcode 1 If the value is not an integer
# @example
#   to::int "1" # 1
#   to::int "a" # error
to::int() {
  local value="${1}"
  [[ ${value} =~ ^-?[0-9]+$ ]] || 
    return 1
  echo "${value}"
}

# @description Convert a value to a float
# @arg $1 any value
# @stdout The value as a float
# @exitcode 1 If the value is not a float
# @example
#   to::float "1.1" # 1.1
#   to::float "a"   # error
to::float() {
  local value="${1}"
  [[ ${value} =~ ^-?[0-9]+(\.[0-9]+)?$ ]] ||
    return 1
  echo "${value}"
}

# @description Convert the value '-' to stdin
# @arg $1 any value
# @stdout The value or stdin
# @example
#   to::stdin "a" # a
#   to::stdin "-" # (stdin)
to::stdin() {
  local value="${1}"
  [[ ${value} != "-" ]] || 
    value="$(cat)"
  echo "${value}"
}

# @description Check if a value is a file
# @arg $1 any value
# @exitcode 1 If the value is not a file
# @example
#   to::file "a" # error
#   to::file "file.txt" # file.txt
to::file() {
  local value="${1}"
  [[ -f "${value}" ]] || 
    return 1
  echo "${value}"
}