#!/usr/bin/env bash
# @file main
# @brief Main function for running a bash script
# @description
#   This file contains the main function for running a bash script
set -euo pipefail

# @internal
# shellcheck disable=SC1090
import() { declare -A _i; (( ${_i[${1}]:-} )) || { _i[${1}]=1; . "${BASH_SOURCE[0]%/*}/${1}.sh"; } }
import bash
import binary
import docker

# @description Run a bash script from a shebang
# @arg $1 string file to run
# @exitcode 1 If the file does not exist
# @exitcode 1 If the file is the same as the current file
argsh::shebang() {
  local -r file="${1}"
  : "${ARGSH_SOURCE="${file}"}"
  export ARGSH_SOURCE
  [[ "${BASH_SOURCE[-1]}" != "${file}" && -f "${file}" ]] || {
    # echo "This is intended to be used in a shebang"
    # echo "#!/usr/bin/env argsh"
    binary::exists docker || {
      echo "This script requires Docker to be installed"
      return 1
    } >&2
    local tty=""
    ! tty -s || tty="-it"
    # shellcheck disable=SC2046
    docker run --rm ${tty} $(docker::user) \
      -e "BATS_LOAD" \
      -e "ARGSH_SOURCE" \
      -e "GIT_COMMIT_SHA=$(git rev-parse HEAD 2>/dev/null || :)" \
      -e "GIT_VERSION=$(git describe --tags --dirty 2>/dev/null || :)" \
      ghcr.io/arg-sh/argsh:latest "${@}" 
    return 0
  }
  bash::version 4 3 0 || {
    echo "This script requires bash 4.3.0 or later"
    return 1
  } >&2

  shift
  # shellcheck source=/dev/null
  . "${file}"
}