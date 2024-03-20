#!/usr/bin/env bash
# shellcheck disable=SC2034
set -euo pipefail

fmt::args1() {
  local pos1 flag1 flag2
  local -a args=(
    'pos1'    "Positional parameter 1"
    -         "Group Flags 1"
    'flag1|f' "Description of flag1"
    -         "Group Flags 2"
    'flag2|l' "Description of flag2"
  )
  :args "This is a test" "${@}"
  echo "pos1: ${pos1:-}"
  echo "flag1: ${flag1:-}"
  echo "flag2: ${flag2:-}"
}

:test::fmt1() {
  local -a usage=(
    '-'                "Group 1"
    'cmd1:-fmt::args1' "Description of cmd1"
    '-'                "Group 2"
    'noop'             "Description of noop"
  )
  :usage "Simple description of the command" "${@}"
  "${usage[@]}"
}

fmt::args2() {
  local pos1 flag1 flag2
  local -a args=(
    'pos1'    "Positional parameter 1"
    'flag1|f' "Description of flag1"
    -         "Group Flags"
    'flag2|l' "Description of flag2"
  )
  :args "This is a test" "${@}"
  echo "pos1: ${pos1:-}"
  echo "flag1: ${flag1:-}"
  echo "flag2: ${flag2:-}"
}

:test::fmt2() {
  local -a usage=(
    'cmd1:-fmt::args2' "Description of cmd1"
    -                  "Group"
    'noop'             "Description of noop"
  )
  :usage "Simple description of the command" "${@}"
  "${usage[@]}"
}