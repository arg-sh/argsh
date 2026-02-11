#!/usr/bin/env bash
set -euo pipefail

original_func() {
  echo "original: $*"
}

another_func() {
  echo "another: $*"
}
