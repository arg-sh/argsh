#!/usr/bin/env bash
# @file docker
# @brief Functions for working with Docker
# @description
#   This file contains functions for working with Docker
set -euo pipefail

# @description
#   Prepare a Docker container for running as current or specified user.
#   This function creates a temporary passwd and group file to run the container.
#   Env $PATH_BASE is mounted to the container's $HOME.
# @arg $1 string user id
# @arg $2 string group id
# @arg $3 string user name
# @arg $4 string home directory
# @arg $5 string shell
# @stdout The Docker run options for running as the specified user
# @example
#   # docker::user "$(id -u)" "$(id -g)" "$(whoami)" "/workspace" "/bin/sh"
#   flags=$(docker::user)
#   docker run ${flags} image
docker::user() {
  local uid="${1:-"$(id -u)"}"
  local gid="${2:-"$(id -g)"}"
  local user="${3:-"$(whoami)"}"
  local home="${4:-"/workspace"}"
  local shell="${5:-"/bin/sh"}"

  echo "${user}:x:${uid}:${gid}::${home}:${shell}" > /tmp/docker_passwd
  echo "${user}:x:${gid}:" > /tmp/docker_group
  
  echo -v "${PATH_BASE:-.}:${home}"
  echo -v /tmp/docker_passwd:/etc/passwd -v /tmp/docker_group:/etc/group
  echo -u "${uid}:${gid}"
  echo -w "${home}"
}