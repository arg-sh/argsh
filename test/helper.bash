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
  [[ -z "${__BUILTIN_SKIP:-}" ]] || skip "${__BUILTIN_SKIP}"
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

  # A missing file means two different things depending on how it was chosen,
  # and they must not share a code path.
  #
  # DEFAULT (no BATS_LOAD): the `.sh` beside the `.bats` is a convention, not a
  # requirement — suites legitimately have no matching source file. Skipping is
  # correct, and so is the silence.
  #
  # EXPLICIT (BATS_LOAD set): the caller named a file. Returning 0 sources
  # NOTHING and runs the suite against whatever the environment already
  # provides, so `BATS_LOAD=argsh.min.sh` from the wrong directory reports on a
  # bundle it never read. Measured: a misdirected BATS_LOAD turns 370 passing
  # tests into 136 failures that say `command not found` and never mention
  # BATS_LOAD — and any test file that does not happen to touch the source
  # still PASSES. An explicit request that cannot be honoured is an error.
  if [[ ! -f "${file}" ]]; then
    [[ -z "${BATS_LOAD}" ]] || {
      echo "load_source: BATS_LOAD=${BATS_LOAD} does not exist (cwd: ${PWD})" >&2
      echo "  Refusing to run — the suite would test whatever is already loaded," >&2
      echo "  not the file you named." >&2
      return 1
    }
    return 0
  fi

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
      [[ $(declare -p "${var}") == "declare -a ${var}" ]] || return 1
    else
      [[ ! ${!var+x} ]] || return 1
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