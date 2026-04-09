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
exec argsh "${@}"
