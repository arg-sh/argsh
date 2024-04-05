#!/usr/bin/env bats
# shellcheck shell=bash disable=SC2154
# vim: filetype=bash
# This test file has to be run from the docker container itself
set -euo pipefail

load "/workspace/test/helper"
load_source

@test "ignore variables" {
  (
    docker-entrypoint.sh minify "${PATH_FIXTURES}/ignore_vars.sh"
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  is_empty stderr
  grep -q 'local usage' "${stdout}"
  grep -q 'local args' "${stdout}"
  grep -vq obfuscate "${stdout}"
}