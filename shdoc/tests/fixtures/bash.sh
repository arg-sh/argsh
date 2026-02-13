#!/usr/bin/env bash
# @file bash
# @brief Functions around bash
# @description
#   This file contains functions around bash
set -euo pipefail

# @description Verify the version of bash
# @arg $1 int major version
# @arg $2 int minor version
# @arg $3 int patch version
# @exitcode 0 If the version is greater than or equal to the specified version
# @exitcode 1 If the version is less than the specified version
# @example
#   bash::version 4 3 0 # succeeds (returns 0)
bash::version() {
  local major="${1:-4}"
  local minor="${2:-3}"
  local patch="${3:-0}"

  if [[ "${BASH_VERSINFO[0]}" -lt "${major}" ]]; then
    return 1
  elif [[ "${BASH_VERSINFO[0]}" -gt "${major}" ]]; then
    return 0
  fi

  if [[ "${BASH_VERSINFO[1]}" -lt "${minor}" ]]; then
    return 1
  elif [[ "${BASH_VERSINFO[1]}" -gt "${minor}" ]]; then
    return 0
  fi

  if [[ "${BASH_VERSINFO[2]}" -lt "${patch}" ]]; then
    return 1
  fi

  return 0
}