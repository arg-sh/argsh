#!/usr/bin/bash
# @file fmt
# @brief Functions for formatting text
# @tags core
# @description
#   This file contains functions for formatting text
set -euo pipefail

# @description Format text to the width of the terminal
# @arg $1 string|stdin text to format. If not provided, will read from stdin
# @stdout The formatted text
# @example
#   fmt::tty "This is a long line that should be wrapped to the width of the terminal"
#   fmt::tty < file.txt
#   cat file.txt | fmt::tty
fmt::tty() {
  local str="${1:-"$(cat)"}"
  if ! command -v fmt &>/dev/null || [[ ! -t 1 ]]; then
    echo "${str}"
    return 0
  fi

  local cols
  cols="$(tput cols)"
  echo "${str}" | fmt -w "${cols}"
}
