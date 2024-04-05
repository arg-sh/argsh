#!/usr/bin/env bash

: "${BATS_LOAD:=""}"
: "${PATH_FIXTURES:=""}"

declare stdout stderr status
setup() {
  # shellcheck disable=SC2317
  :validate() { :; }
  status=0
  stdout="$(mktemp)"
  stderr="$(mktemp)"
}
teardown() {
  rm -f "${stdout}" "${stderr}"
}

load_source() {
  local file
  if [[ -n "${BATS_LOAD}" ]]; then
    file="${BATS_LOAD}"
  else
    file="${BATS_TEST_FILENAME/%.bats/.sh}"
  fi
  : "${PATH_FIXTURES:="$(
    realpath "$(dirname "${BATS_TEST_FILENAME}")/fixtures/$(basename "${BATS_TEST_FILENAME%.*}")"
  )"}"
  : "${PATH_SNAPSHOTS="${PATH_FIXTURES}/snapshots"}"
  mkdir -p "${PATH_SNAPSHOTS}"

  [[ -f "${file}" ]] ||
    return 0

  # shellcheck disable=SC1090
  source "${file}"
}

snapshot() {
  local file name snap
  for file in "${@}"; do
    name="${BATS_TEST_NAME}"
    snap="${PATH_SNAPSHOTS}/${name}.${file}.snap"
    [[ -f "${snap}" ]] || {
      cat "${!file}" >"${snap}"
    }
    [[ "$(cat "${!file}")" == "$(cat "${snap}")" ]] || {
      echo "■■ Snapshot ${name}.${file} does not match"
      diff -u "${snap}" "${!file}"
      return 1
    } 
  done
}

assert() {
  local args=("${@}")
  test "${args[@]}" || {
    echo "■■ with [[ ${args[*]} ]]"
    [[ -z "${stdout:-}" ]] || echo -e "■■ stdout >>>\n$(cat "${stdout}")\n<<< stdout"
    [[ -z "${stderr:-}" ]] || echo -e "■■ stderr >>>\n$(cat "${stderr}")\n<<< stderr"
    return 1
  }
}

is_empty() {
  local check="${1}"
  [[ -n "${!check}" ]] || return 0
  [[ -s "${!check}" ]] || return 0

  echo "■■ ${check} is not empty"
  [[ ! -f "${!check}" ]] || echo -e "■■ >>>\n$(cat "${!check}")\n<<<"
  return 1
}

not_empty() {
  local check="${1}"
  [[ -n "${!check}" ]] || return 1
  [[ -s "${!check}" ]] || return 1

  return 0
}

contains() {
  local check="${1}"
  local -n file="${2}"
  grep -qzP "${check}" "${file}" || {
    echo "■■ ${file} does not contain ${check}"
    cat "${file}"
    return 1
  }
}

is::uninitialized() {
  local var
  for var in "${@}"; do
    if is::array "${var}"; then
      [[ $(declare -p "${var}") == "declare -a ${var}" ]]
    else
      [[ ${!var+x} ]]
    fi
  done
}

filter_control_sequences() {
  "${@}" 2>&1 | sed $'s,\x1b\\[[0-9;]*[a-zA-Z],,g'
  exit "${PIPESTATUS[0]}"
}

# shellcheck disable=SC2154
log_on_failure() {
  echo Failed with status "${status}" and output:
  echo "${output}"
}

declare -p grep 2>/dev/null || {
  grep="$(command -v grep)"
  readonly grep
}
grep() {
  $grep "${@}" || {
    local status="${?}"
    echo "■■ grep failed with status ${status}"
    if [[ -f "${*: -1}" ]]; then
      echo "■■ >>>"
      cat "${*: -1}"
      echo "<<<"
    fi
    return "${status}"
  }
}