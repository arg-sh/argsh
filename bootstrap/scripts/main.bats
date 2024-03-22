#!/usr/bin/env bats
# shellcheck shell=bash

load ../test/helper
# This is a helper function to loads the source file to be tested
# Usually, it is the same filename as the test file, but with a .sh extension
# But if BATS_LOAD is set, it will use that instead
load_source

@test "no argument, -h, --help" {
  # Note that we are running the main function within a subshell
  # This is because if the main function were to exit, it would exit the test
  (
    main
  ) >"${stdout}" 2>"${stderr}" || status=$?

  # You could also use [[ ${status} -eq 0 ]] but `assert` will give you a better error message
  assert "${status}" -eq 0
  # stderr and stdout are temporary files created before the test
  is_empty stderr
  # Have a look into ./test/fixtures/snapshots/
  snapshot stdout

  # Lets make sure that all other ways to print help output the same thing
  for flag in -h --help; do
    (
      main "${flag}"
    ) >"${stdout}" 2>"${stderr}" || status=$?

    assert "${status}" -eq 0
    is_empty stderr
    snapshot stdout
  done
}

@test "v, version" {
  for command in v version; do
    (
      main "${command}"
    ) >"${stdout}" 2>"${stderr}" || status=$?

    assert "${status}" -eq 0
    is_empty stderr
    # This would fail if the version changes
    snapshot stdout
  done
}

@test "short version" {
  (
    main version -s
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "hello" {
  (
    main hello
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  snapshot stdout
}

@test "hello with all subcommands" {
  for subcommand in github lint test coverage docs styleguide minify; do
    (
      main hello "${subcommand}"
    ) >"${stdout}" 2>"${stderr}" || status=$?

    assert "${status}" -eq 0
    is_empty stderr
    snapshot stdout
  done
}