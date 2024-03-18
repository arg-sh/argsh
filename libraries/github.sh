#!/usr/bin/env bash
# @file github
# @brief Functions for working with GitHub
# @description
#   This file contains functions for working with GitHub
set -euo pipefail

# @description Get the latest release version from a GitHub repository
# @arg $1 string GitHub repository
# @stdout The latest release version
# @example
#   github::latest "koalaman/shellcheck" # v0.10.0
github::latest() {
  local repo="${1}"
  curl -fsSLI -o /dev/null -w "%{url_effective}" \
    "https://github.com/${repo}/releases/latest" |
    rev | cut -d'/' -f1 | rev
}