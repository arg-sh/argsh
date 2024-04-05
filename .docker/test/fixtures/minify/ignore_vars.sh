#!/usr/bin/env bash

main() {
  local usage="test"
  local args="test"
  local obfuscate="test"
  echo "${usage} ${args} ${obfuscate}"
}
[[ "${0}" != "${BASH_SOURCE[0]}" && -z "${ARGSH_SOURCE}" ]] || main "${@}"