#!/usr/bin/bash
# @file fmt
# @brief Functions for formatting text
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
  command -v fmt &>/dev/null || {
    echo "${str}"
    return 0
  }

  local cols
  cols="$(tput cols)"
  echo "${str}" | fmt -w "${cols}"
}
