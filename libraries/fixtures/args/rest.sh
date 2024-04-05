#!/usr/bin/env bash
# shellcheck disable=SC2034
set -euo pipefail

:test::rest() {
  local -a all args=(
    'all' 'All arguments'
  )
  :args "Test rest parameters" "${@}"
  :validate >&3
  echo "${all[@]:-}"
}