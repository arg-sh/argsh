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
: "${ARGSH_SOURCE:="argsh"}"
export ARGSH_SOURCE
# Use absolute path — /usr/local/sbin may shadow argsh when PATH_BIN
# is mounted there (it comes before /usr/local/bin on PATH).
exec /usr/local/bin/argsh "${@}"
