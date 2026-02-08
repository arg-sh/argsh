#!/usr/bin/env bash
# @file array
# @brief Array manipulation functions
# @tags core
# @description
#   This file contains functions for manipulating arrays

set -euo pipefail

# @description Check if an array contains a value
# @arg $1 string needle
# @arg $@ array elements to search
# @exitcode 0 If the array contains the needle
# @exitcode 1 If the array does not contain the needle
# @example
#   local -a arr=("a" "b" "c" "d" "e")
#   array::contains "a" "${arr[@]}" # succeeds (returns 0)
#   array::contains "z" "${arr[@]}" # fails (returns 1)
array::contains() {
  local -r needle="${1}"; shift
  for element in "${@}"; do
    [[ "${element}" != "${needle}" ]] || return 0
  done
  return 1
}

# @description Join an array with a delimiter
# @arg $1 string delimiter
# @arg $@ array elements to join
# @stdout The joined array
# @example
#   local -a arr=("a" "b" "c" "d" "e")
#   array::join "," "${arr[@]}" # a,b,c,d,e
array::join() {
  local -r delimiter="${1}"; shift
  local result
  printf -v result "${delimiter}%s" "${@}"
  echo "${result:${#delimiter}}"
}

# @description Get the nth element of an array
# @arg $1 string new array name
# @arg $2 integer nth element
# @arg $@ array elements to get nth element from
# @example
#   local -a new_arr arr=("a" "b" "c" "d" "e")
#   array::nth new_arr 2 "${arr[@]}"
#   echo "${new_arr[@]}" # b d
array::nth() {
  local -n out="${1}"
  local -r nth="${2}"
  shift 2

  for (( i=1; i<=${#}; i++ )); do
    (( i % nth )) || out+=("${!i}")
  done
}
