#!/usr/bin/env bash
set -euo pipefail

# This script is used to prevent Vercel from deploying branches that are not allowed to be deployed
main() {
  local current_branch
  current_branch=$(git branch --show-current)

  if [[ "${VERCEL_DEPLOY_BRANCHES}" == *${current_branch}* ]]; then
    echo "Branch allowed to deploy"
    exit 1
  fi

  echo "Branch not allowed to deploy"
}

[[ "${BASH_SOURCE[0]}" != "${0}" ]] || main "${@}"