#!/usr/bin/env bash
# @file args.utils
# @brief Functions for working with arguments
# @description
#   This file contains functions for working with arguments 
set -euo pipefail

# @description
#   Run a function if a flag is set
# @arg $1 any if empty will run all functions
# @arg $2 boolean run the following function
# @arg $3 string the function name to run
# @example
#   args:run "" 1 test::argsh 1 test::docs
args::run() {
  local all="${1}"; shift
  for (( i=0; i<${#}; i++ )); do
    local run="${1}"; shift
    local func="${1}"; shift

    if ! (( all )) || (( run )); then
      "${func}"
    fi
  done
}
