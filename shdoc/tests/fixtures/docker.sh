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
  local curr
  curr="$(pwd)"
  curr="${curr#"${PATH_BASE:-}"}"
  if [[ "${curr}" == "$(pwd)" ]]; then
    curr="${home}"
  else  
    curr="${home}${curr}"
  fi

  local _passwd _group
  _passwd="$(mktemp /tmp/docker_passwd.XXXXXX)"
  _group="$(mktemp /tmp/docker_group.XXXXXX)"
  echo "${user}:x:${uid}:${gid}::${home}:${shell}" > "${_passwd}"
  echo "${user}:x:${gid}:" > "${_group}"
  echo "-v ${_passwd}:/etc/passwd -v ${_group}:/etc/group"
  echo "-u ${uid}:${gid}"

  echo "-v ${PATH_BASE:-"$(pwd)"}:${home}"
  echo "-w ${curr}"
}