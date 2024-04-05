#!/usr/bin/env bash
# shellcheck disable=SC2034
set -euo pipefail

:test::usage() {
  local config
  local -a verbose args=(
    'verbose|v:+' "Description of arg2"
    'config|f'    "Description of flag"
  )
  local -a usage=(
    'cmd1|alias'       "Description of cmd1"
    'cmd2:-main::cmd2' "Description of cmd2"
    '#cmd3'            "Description of hidden cmd3"
  )
  :usage "Simple description of the command" "${@}"
  # pre run
  "${usage[@]}"
  # post run
}

main::cmd2() {
  :args "Description of cmd2" "${@}"
  echo "cmd2"
  echo "verbose: ${verbose[*]:-}"
  echo "config: ${config:-}"
}

cmd3() {
  :args "Description of cmd3" "${@}"
  echo "cmd3"
  echo "verbose: ${verbose[*]:-}"
  echo "config: ${config:-}"
}

cmd1() {
  local command
  local -a usage=(
    'subcmd1'   "Description of subcmd1"
    'subcmd2'   "Description of subcmd2"
  )
  :usage "Subcommands of cmd1" "${@}"
  "${usage[@]}"
}

subcmd1() {
  local flag2
  args+=(
    'flag2|l'  'Description of flag2'
  )
  :args "Description of subcmd1" "${@}"

  echo "subcmd1"
  echo "verbose: ${verbose[*]:-}"
  echo "config: ${config:-}"
  echo "flag2: ${flag2:-}"
}