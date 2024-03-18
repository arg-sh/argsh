#!/usr/bin/env bash

: "${BATS_LOAD:=""}"
: "${PATH_FIXTURES:=""}"

load_source() {
  local file
  if [[ -n "${BATS_LOAD}" ]]; then
    file="${BATS_LOAD}"
  else
    file="${BATS_TEST_FILENAME/%.bats/.sh}"
  fi
  PATH_FIXTURES="$(
    realpath "$(dirname "${BATS_TEST_FILENAME}")/../test/fixtures/$(basename "${BATS_TEST_FILENAME%.*}")"
  )"
  # shellcheck disable=SC1090
  source "${file}"
}

snapshot() {
  local file name snap
  for file in "${@}"; do
    name="${BATS_TEST_NAME}"
    snap="${PATH_FIXTURES}/${name}.${file}.snap"
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