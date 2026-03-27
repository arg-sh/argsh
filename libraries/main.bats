#!/usr/bin/env bats
# shellcheck disable=SC1091 disable=SC2154 disable=SC2317 disable=SC2329 disable=SC2034 disable=SC2030 disable=SC2031 disable=SC2314
# shellcheck shell=bats
#
# Tests for argsh::builtin, argsh::status, and argsh::help functions.

load ../test/helper
ARGSH_SOURCE=argsh
load_source

# Ensure ARGSH_BUILTIN is defined for tests (default: not loaded)
declare -gi ARGSH_BUILTIN="${ARGSH_BUILTIN:-0}"

# ---------------------------------------------------------------------------
# argsh::builtin (no args) — shows status
# ---------------------------------------------------------------------------
@test "argsh::builtin: no args shows status" {
  ARGSH_BUILTIN=0 argsh::builtin >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh builtin:" stdout
  contains "platform:" stdout
  contains "loaded:" stdout
  contains "Usage:" stdout
}

# ---------------------------------------------------------------------------
# argsh::builtin status — same output as no args
# ---------------------------------------------------------------------------
@test "argsh::builtin status: shows status" {
  ARGSH_BUILTIN=0 argsh::builtin status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh builtin:" stdout
  contains "platform:" stdout
  contains "loaded:" stdout
}

# ---------------------------------------------------------------------------
# argsh::builtin unknowncmd — returns error
# ---------------------------------------------------------------------------
@test "argsh::builtin: unknown subcommand returns error" {
  argsh::builtin unknowncmd >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 1
  is_empty stdout
  contains "unknown builtin subcommand: unknowncmd" stderr
  contains "Usage:" stderr
}

# ---------------------------------------------------------------------------
# argsh::builtins (plural alias) — delegates to singular
# ---------------------------------------------------------------------------
@test "argsh::builtins: plural alias delegates to argsh::builtin" {
  ARGSH_BUILTIN=0 argsh::builtins >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh builtin:" stdout
}

# ---------------------------------------------------------------------------
# argsh::status — shows version, builtin section, shell section, features
# ---------------------------------------------------------------------------
@test "argsh::status: shows all sections" {
  ARGSH_BUILTIN=0 argsh::status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh " stdout
  contains "Builtin" stdout
  contains "status:" stdout
  contains "Shell:" stdout
  contains "bash:" stdout
  contains "Features:" stdout
}

# ---------------------------------------------------------------------------
# argsh::status with ARGSH_BUILTIN=1 shows "available"
# ---------------------------------------------------------------------------
@test "argsh::status: ARGSH_BUILTIN=1 shows available features" {
  ARGSH_BUILTIN=1 argsh::status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "loaded" stdout
  contains "available \\(builtin\\)" stdout
}

# ---------------------------------------------------------------------------
# argsh::status with ARGSH_BUILTIN=0 shows "requires builtin"
# ---------------------------------------------------------------------------
@test "argsh::status: ARGSH_BUILTIN=0 shows requires builtin" {
  ARGSH_BUILTIN=0 argsh::status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "not loaded" stdout
  contains "requires builtin" stdout
}

# ---------------------------------------------------------------------------
# argsh::help — shows usage with builtin and status commands
# ---------------------------------------------------------------------------
@test "argsh::help: shows usage information" {
  argsh::help >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "Usage:" stdout
  contains "builtin" stdout
  contains "status" stdout
  contains "Commands:" stdout
  contains "Flags:" stdout
  contains "Environment:" stdout
}
