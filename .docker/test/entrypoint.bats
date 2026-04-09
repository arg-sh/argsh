#!/usr/bin/env bats
# shellcheck shell=bash disable=SC2154
# vim: filetype=bash
#
# Regression tests for .docker/docker-entrypoint.sh.
#
# Guards against:
#   - "argsh::discover_files: command not found" when running
#     `docker-entrypoint.sh test` / `lint` / `coverage` with no args.
#   - Entrypoint subcommand list drifting from the launcher's argsh::main.
#
# This file must be run from inside the argsh docker container.
set -euo pipefail

load "/workspace/test/helper"
load_source

@test "entrypoint: --help lists all subcommands" {
  (
    docker-entrypoint.sh --help
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  contains "minify" stdout
  contains "lint" stdout
  contains "test" stdout
  contains "coverage" stdout
  contains "docs" stdout
  contains "builtin" stdout
  contains "status" stdout
}

@test "entrypoint: unknown command suggests closest match" {
  (
    docker-entrypoint.sh tests
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -ne 0
  contains "test" stderr
}

@test "entrypoint: test with no args discovers .bats files via PATH_TEST" {
  local _tmp
  _tmp="$(mktemp -d)"
  cat >"${_tmp}/sample.bats" <<'EOF'
#!/usr/bin/env bats
@test "sample" { true; }
EOF
  (
    PATH_TEST="${_tmp}" docker-entrypoint.sh test
  ) >"${stdout}" 2>"${stderr}" || status="${?}"
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "sample" stdout
}

@test "entrypoint: test -h shows clean subcommand usage (no 'test test')" {
  (
    docker-entrypoint.sh test -h
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  # Must NOT contain the old awkward 'test test ...tests' rendering.
  ! grep -q "test test" "${stdout}"
  # Should advertise the renamed positional.
  contains "path" stdout
}

@test "entrypoint: status reports runtime info" {
  (
    docker-entrypoint.sh status
  ) >"${stdout}" 2>"${stderr}" || status="${?}"

  assert "${status}" -eq 0
  contains "Shell:" stdout
  contains "Features:" stdout
}
