#!/usr/bin/env bash
# shellcheck shell=bash
# vim: filetype=bash
#
# Docker entrypoint for ghcr.io/arg-sh/argsh.
#
# argsh itself is the CLI: all subcommands (minify, lint, test, coverage,
# docs, builtin, status) are registered in argsh::main and dispatched via
# :usage. This entrypoint just forwards to argsh so the container and the
# host launcher share one source of truth.
set -euo pipefail
# Set ARGSH_SOURCE for this invocation only — don't export it into child
# processes (bats tests, user scripts) where it would interfere with
# standalone-vs-sourced detection guards.
# Use absolute path — /usr/local/sbin may shadow argsh when PATH_BIN
# is mounted there (it comes before /usr/local/bin on PATH).
ARGSH_SOURCE="${ARGSH_SOURCE:-argsh}" exec /usr/local/bin/argsh "${@}"
