#!/usr/bin/env bash
# @file main
# @brief Main function for running a bash script
# @description
#   This file contains the main function for running a bash script

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
  local -a version
  read -ra version <<< "$(echo "${BASH_VERSION}" | tr '.' ' ')"

  if [[ "${version[0]}" -lt "${major}" ]]; then
    return 1
  elif [[ "${version[0]}" -gt "${major}" ]]; then
    return 0
  fi

  if [[ "${version[1]}" -lt "${minor}" ]]; then
    return 1
  elif [[ "${version[1]}" -gt "${minor}" ]]; then
    return 0
  fi

  if [[ "${version[2]}" -lt "${patch}" ]]; then
    return 1
  fi

  return 0
}

# @description Run a bash script from a shebang
# @arg $1 string file to run
# @exitcode 1 If the file does not exist
# @exitcode 1 If the file is the same as the current file
argsh::shebang() {
  local -r file="${*: -1}"
  [[ -e "${file}" && "${BASH_SOURCE[-1]}" != "${file}" ]] || {
    echo "This is intended to be used in a shebang"
    echo "#!/usr/bin/env argsh"
    return 1
  } >&2
  bash::version 4 3 0 || {
    echo "This script requires bash 4.3.0 or later"
    return 1
  } >&2
  ARGSH_SOURCE="${file}"
  export ARGSH_SOURCE

  # shellcheck source=/dev/null
  . "${file}"
}
