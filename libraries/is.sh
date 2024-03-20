#!/usr/bin/env bash
# @file is
# @brief Functions for checking types
# @description
#   This file contains functions for checking types

set -euo pipefail

# @description Check if terminal is a tty
# @exitcode 0 If the terminal is a tty
# @exitcode 1 If the terminal is not a tty
# @example
#   is::tty # succeeds (returns 0)
is::tty() {
  [[ -t 1 ]]
}

# @description Check if a variable is an array
# @arg $1 string variable name
# @exitcode 0 If the variable is an array
# @exitcode 1 If the variable is not an array
# @example
#   local -a arr=("a" "b" "c" "d" "e")
#   is::array arr # succeeds (returns 0)
#   is::array str # fails (returns 1)
is::array() {
  declare -p "${1}" &>/dev/null && [[ $(declare -p "${1}") == "declare -a"* ]]
}

# @description Check if a variable is uninitialized
# @arg $1 string variable name
# @exitcode 0 If the variable is uninitialized
# @exitcode 1 If the variable is initialized
# @example
#   local -a arr=("a" "b" "c" "d" "e")
#   is::uninitialized arr # fails (returns 1)
#   local -a str
#   is::uninitialized str # succeeds (returns 0)
is::uninitialized() {
  local var="${1}"
  if is::array "${var}"; then
    [[ $(declare -p "${var}") == "declare -a ${var}" ]]
  else
    [[ ${!var+x} ]]
  fi
}

# @description Check if a variable is set (initialized)
# @arg $1 string variable name
# @exitcode 0 If the variable is set
# @exitcode 1 If the variable is not set
# @example
#   local -a arr=("a" "b" "c" "d" "e")
#   is::set arr # succeeds (returns 0)
is::set() {
  ! is::uninitialized "${1}"
}