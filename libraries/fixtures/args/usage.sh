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

# --- prefix resolution tests ---

# Test: caller::func is preferred over bare func
:test::prefix() {
  local -a usage=(
    'start'   "Start something"
    'stop'    "Stop something"
  )
  :usage "Prefix test" "${@}"
  "${usage[@]}"
}

# This should be picked (caller prefix match)
:test::prefix::start() {
  echo "prefix::start"
}

# This bare function should NOT be reached when :test::prefix calls :usage
start() {
  echo "bare::start"
}

# No :test::prefix::stop exists, so bare stop() should be used as fallback
:test::prefix::stop() {
  echo "prefix::stop"
}

# Test: nested prefix resolution (caller changes at each level)
:test::nested() {
  local -a usage=(
    'sub' "Enter sub"
  )
  :usage "Nested test" "${@}"
  "${usage[@]}"
}

:test::nested::sub() {
  local -a usage=(
    'action' "Do action"
  )
  :usage "Nested sub" "${@}"
  "${usage[@]}"
}

# Should resolve via :test::nested::sub -> caller::action
:test::nested::sub::action() {
  echo "nested::sub::action"
}

# --- coverage: no visible subcommands, long-only flag ---

:test::nosub() {
  local longonly
  local -a args=(
    'longonly|:~string' "A long-only flag"
  )
  local -a usage=(
    '#hidden' "Hidden command"
  )
  :usage "Multi-line description
with a blank line

and more text" "${@}"
  "${usage[@]}"
}