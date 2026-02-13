#!/usr/bin/env bash
# @file string
# @brief String manipulation functions
# @tags core
# @description
#   This file contains functions for manipulating strings
set -euo pipefail

# @description drops length characters from the string at index
# @arg $1 string The string to drop characters from
# @arg $2 integer The index to start dropping characters
# @arg $3 integer The number of characters to drop
# @stdout The string with characters dropped
# @example
#   string::drop-index "hello" 1 2 # hlo
string::drop-index() {
  local string="${1}"
  local index="${2}"
  local length="${3:-1}"
  echo "${string:0:index}${string:index+length}"
}

# @description Generate a random string. First character is always a letter.
# @arg $1 integer [42] The length of the string
# @arg $2 string [a-zA-Z0-9] The characters to use in the string
# @stdout The random string
# @example
#   string::random 10 # a2jKl9C4bs
#   string::random    # t4UsOP3z5Rd8sW6nX2t1C7z9L0s3R4d8cAH32ns2Ds
string::random() {
  local length="${1:-42}"
  local chars="${2:-"a-zA-Z0-9"}"
  local str

  until [[ "${str:-}" =~ ^[[:alpha:]] ]]; do
    str=$(tr -dc "${chars}" < /dev/urandom | fold -w "${length}" | head -n 1 || :)
  done
  echo "${str}"
}

# @description Left trim all lines in a string
# @arg $1 string The string to trim
# @arg $2 int [0] Indent the string by this amount
# @stdout The trimmed string
# @example
#   string::indent "  hello\n  world" # "hello\nworld"
string::indent() {
  local string="${1:-'-'}"
  local indent="${2:-0}"
  local line lines
  [[ ${string} != '-' ]] || string="$(cat)"

  mapfile -t lines < <(echo "${string}")
  for line in "${lines[@]}"; do
    line="$(string::trim-left "${line}")"
    (( indent == 0 )) || printf "%${indent}s" " " 
    echo "${line}"
  done
}

# @description Left trim a string
# @arg $1 string The string to trim
# @arg $2 string [ \n\t] The characters to trim
# @stdout The trimmed string
# @example
#   string::trim-left "  hello  " # "hello  "
string::trim-left() {
  local string="${1}"
  local chars="${2:-" "$'\n'$'\t'}"
  [[ -n ${string:-} ]] || return 0
  [[ ${string} != '-' ]] || string="$(cat)" 

  while [[ -n "${string}" ]]; do
    [[ ${chars} == *"${string:0:1}"* ]] || break
    string="${string:1}"
  done
  echo "${string}"
}

# @description Right trim a string
# @arg $1 string The string to trim
# @arg $2 string [ \n\t] The characters to trim
# @stdout The trimmed string
# @example
#   string::trim-right "  hello  " # "  hello"
string::trim-right() {
  local string="${1:-'-'}"
  local chars="${2:-" "$'\n'$'\t'}"
  [[ ${string} != '-' ]] || string="$(cat)"

  while [[ -n "${string}" ]]; do
    [[ ${chars} == *"${string: -1}"* ]] || break
    string="${string:0: -1}"
  done
  echo "${string}"
}

# @description Trim a string
# @arg $1 string The string to trim
# @arg $2 string [ \n\t] The characters to trim
# @stdout The trimmed string
# @example
#   string::trim "  hello  " # "hello"
string::trim() {
  local string="${1:-'-'}"
  local chars="${2:-" "$'\n'$'\t'}"
  [[ ${string} != '-' ]] || string="$(cat)" 

  echo "${string}" |
    string::trim-left - "${chars}" |
    string::trim-right - "${chars}"
}