#!/usr/bin/env bash
# @file binary
# @brief Functions for working with binaries
# @description
#   This file contains functions for working with binaries
set -euo pipefail

# @description Check if a binary exists
# @arg $1 string binary name
# @stderr The binary is required to run this script
# @exitcode 0 If the binary exists
# @exitcode 1 If the binary does not exist
# @example
#   binary::exists "curl" # succeeds (returns 0)
#   binary::exists "zcurl" # fails (returns 1)
binary::exists() {
  local binary="${1}"
  command -v "${binary}" &> /dev/null || {
    echo "${binary} is required to run this script" >&2
    return 1
  }
}

# @description Download a binary from github
# @arg $1 string path to binary
# @arg $2 string GitHub repository
# @arg $3 string file to download
# @arg $4 string [opt] tar file to extract
# @example
#   # https://github.com/cli/cli/releases/download/v2.45.0/gh_2.45.0_linux_amd64.tar.gz
#   latest="$(github::latest "cli/cli")"
#   binary::github "./bin/gh" "cli/cli" "${latest}/gh_${latest:1}_$(uname -s)_$(uname -m).tar.gz" "gh_${latest:-1}_$(uname -s)_$(uname -m)/bin/gh"
binary::github() {
  local path="${1}"
  local repo="${2}"
  local file="${3}"
  local tar="${4:-}"
  curl -Lso /dev/stdout "https://github.com/${repo}/releases/download/${file}" | {
    if [[ -n "${tar}" ]]; then
      tar -xz -C "$(dirname "${path}")" "${tar}"
    else
      tee "${path}" &> /dev/null
    fi
    chmod +x "${path}"
  }
}

# @description Get the architecture of the system
# @stdout The architecture of the system
# @example
#   binary::arch # amd64
binary::arch() {
  local short="${1:-0}"
  local -r arch="$(uname -m)"
  case "${arch}" in
    x86_64|amd64) if (( short )); then echo "64-bit"; else echo "amd64"; fi ;;
    armv7l) echo "arm" ;;
    aarch64) echo "arm64" ;;
    *) echo "${arch}" ;;
  esac
}

# @description Download the jq binary into $PATH_BIN if it does not exist
# @example
#   binary::jq
# https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64
binary::jq() {
  binary::exists "jq" 2>/dev/null || {
    local -r latest="$(github::latest "stedolan/jq")" system="$(uname -s)"
    binary::github "${PATH_BIN?}/jq" "stedolan/jq" "${latest}/jq-${system,,}-$(binary::arch)"
  }
}

# @description Download the vale binary into $PATH_BIN if it does not exist
# @example
#   binary::vale
# https://github.com/errata-ai/vale/releases/download/v2.28.0/vale_2.28.0_Linux_64-bit.tar.gz
binary::vale() {
  binary::exists "vale" 2>/dev/null || {
    local -r latest="$(github::latest "errata-ai/vale")" system="$(uname -s)"
    binary::github "${PATH_BIN?}/vale" "errata-ai/vale" "${latest}/vale_${latest:1}_$(uname -s)_$(binary::arch 1).tar.gz" "vale"
  }
}