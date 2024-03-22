#!/usr/bin/env argsh
# shellcheck shell=bash
# vim: filetype=bash
# @file main
# @brief Main script for the project
# @description
#   This file contains the main script for the project
set -euo pipefail

hello::github() {
  :args "Get started with argsh on GitHub" "${@}"

  echo "
    If you explore your .github/workflows directory, you will find a argsh.yaml file.
    This file contains a GitHub Actions workflow that runs argsh on your project.

    You can customize this file to fit your needs.
  " | string::indent - | fmt::tty
}

hello::lint() {
  :args "Lint your Bash scripts" "${@}"

  echo "
    To lint your Bash scripts, you can use ShellCheck.
    ShellCheck is a static analysis tool that gives you warnings and suggestions for bash/sh shell scripts.
    Learn more at https://www.shellcheck.net/

    Check if we have any issues in the main script:

    argsh lint scripts
  " | string::indent - | fmt::tty
}

hello::test() {
  :args "How to get started with testing" "${@}"

  echo "
    To get started with testing, you can use Bats.
    Bats is a TAP-compliant testing framework for Bash.
    Learn more at https://bats-core.readthedocs.io/en/stable/

    We encourage you to write a *.bats file beside your Bash script.
    There is a test file for this script located at scripts/main.bats.

    To run the tests, execute the following command:

    argsh test scripts
  " | string::indent - | fmt::tty
}

hello::coverage() {
  :args "Generate a coverage report" "${@}"

  echo "
    To generate a coverage report, we use a combination of kcov and Bats.
    kcov is a code coverage tool for compiled programs, Python, and Bash.
    Learn more at https://simonkagstrom.github.io/kcov/

    To generate a coverage report, execute the following command:

    argsh coverage scripts coverage --min 100

    Tip: Show your coverage in the README.md file. It's free.
  " | string::indent - | fmt::tty
}

hello::docs() {
  :args "Want to document your Bash scripts?" "${@}"

  echo "
    To generate markdown documentation for your Bash scripts, you can use shdoc.
    Learn more at https://github.com/reconquest/shdoc

    To generate documentation, execute the following command:

    mkdir -p docs/scripts
    argsh docs scripts docs/scripts
  " | string::indent - | fmt::tty
}

hello::styleguide() {
  :args "Need a style guide for your Bash scripts?" "${@}"

  echo "
    To maintain a consistent style in your Bash scripts, use our style guide.
    Learn more at https://arg.sh/styleguide

    This style guide is based on Google's Shell Style Guide but it is hardned.
  " | string::indent - | fmt::tty
}

hello::minify() {
  :args "Feeling experimental? Try minifying your Bash scripts" "${@}"

  echo "
    This is quite experimental.
    But we use it for our own scripts.
    We encourage you to to have a good test coverage before you minify your scripts.
    Learn more at https://arg.sh/minify

    To minify your script, execute the following command:

    argsh minify scripts > main.min.sh
    export BATS_LOAD=\"main.min.sh\"
    argsh test scripts

    Note: BATS_LOAD is an environment variable that tells Bats to load a specific file
    instead of the default test file.
  " | string::indent - | fmt::tty
}

version() {
  local short
  # shellcheck disable=SC2034
  local -a args=(
    'short|s' "Print the short version"
  )
  if (( short )); then
    echo "${ARGSH_VERSION:-unknown}"
  else
    echo "https://arg.sh ${ARGSH_COMMIT_SHA:-unknown} ${ARGSH_VERSION:-unknown}"
  fi
}

hello() {
  local -a usage=(
    'github:-hello::github'         "Get started with argsh on GitHub"
    'lint:-hello::lint'             "Lint your Bash scripts"
    'test:-hello::test'             "How to get started with testing"
    'coverage:-hello::coverage'     "Generate a coverage report"
    'docs:-hello::docs'             "Want to document your Bash scripts?"
    'styleguide:-hello::styleguide' "Need a style guide for your Bash scripts?"
    'minify:-hello::minify'         "Feeling experimental? Try minifying your Bash scripts"
  )
  :usage "Nice to have you here!
          There is a lot to ensure good code quality.
          So we have a few options for you to make it easier at least for your Bash scripts."  "${@}"
  "${usage[@]}"
}

main() {
  # shellcheck disable=SC2034
  local -a usage=(
    'hello'     "Getting started with argsh"
    'version|v' "Print the version of argsh"
  )
  :usage "Hello, argsh!
          This is a simple example of how to use argsh.
          To get started, run the following command:

          ./scripts/main.sh hello" "${@}"
  
  # Here you prerun before the command is executed
  "${usage[@]}"
  # Here you postrun after the command is executed
  # assumed that the command did not exit
  # in this case, you could define a `trap` in prerun to catch it
}

# Only run the main function if this script is executed not sourced
[[ "${BASH_SOURCE[0]}" != "${0}" ]] || main "${@}"