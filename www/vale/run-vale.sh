#!/usr/bin/env bash
set -euo pipefail

:add_to_list() {
  declare -p list || return 1
  local -r file="${1}"

  if [ ${#list} -gt 0 ]; then
    list+=' '
  fi
  list+="www/${file#../}"
}

main() {
  local list=""
  local -r path="../apps/${1}/${2}"
  local -r alert_level="${3:-"suggestion"}"

  # get directories in content other than reference
  find "${path}" -type d -maxdepth 1 -not -path "${path}" -exec add_to_list {} \;
  #get files in content (not nested)
  find "${path}" -type f -maxdepth 1 -exec add_to_list {} \;

  cd ../..
  exec vale "${list}" --minAlertLevel "${alert_level}"
}

[[ "${BASH_SOURCE[0]}" != "${0}" ]] || main "${@}"