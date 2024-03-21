#!/usr/bin/env argsh
# shellcheck shell=bash
# vim: filetype=bash
set -euo pipefail

: "${ARGSH_SOURCE:="argsh"}"
export ARGSH_SOURCE

argsh::minify() {
  local template
  # shellcheck disable=SC2034
  local -a files args=(
    'files'            "Files to minify, can be a glob pattern"
    'template|t:~file' "Path to a template file to use for the minified file"
  )
  :args "Minify Bash files" "${@}"
  ! is::uninitialized files || {
    args::error_usage "No files to minify"
    return 1
  }
  local content out
  content="$(mktemp)"
  out="$(mktemp)"
  # shellcheck disable=SC2064
  trap "rm -f ${content} ${out}" EXIT

  local f file
  local -a glob
  for f in "${files[@]}"; do
    if [[ -d "${f}" ]]; then
      glob=("${f}"/*.{sh,bash})
    else
      # shellcheck disable=SC2206 disable=SC2128
      glob=(${glob})
    fi
    for file in "${glob[@]}"; do
      [[ -e "${file}" ]] || continue
      {
        cat "${file}"
        echo
      } >>"${content}"
    done
  done
  
  obfus -i "${content}" -o "${out}" -A
  local -r data="$(cat "${out}")"
  if [[ -z "${template}" ]]; then
    echo -n "${data}"
    return 0
  fi

  export data
    # shellcheck disable=SC2016 disable=SC2094
  envsubst '$data' <"${template}" >"${template%.*}.sh"
}

argsh::lint() {
  # shellcheck disable=SC2034
  local -a files args=(
    'files' "Files to lint, can be a glob pattern"
  )
  :args "Lint Bash files" "${@}"
  ! is::uninitialized files || {
    :args::error_usage "No files to lint"
    return 1
  }

  local file f
  local -a glob
  for f in "${files[@]}"; do
    if [[ -d "${f}" ]]; then
      glob=("${f}"/*.{sh,bash,bats})
    else
      # shellcheck disable=SC2206 disable=SC2128
      glob=(${glob})
    fi
    for file in "${glob[@]}"; do
      [[ -e "${file}" ]] || continue
      echo "Linting ${file}" >&2
      shellcheck "${file}"
    done
  done
}

argsh::test() {
  local tests="."
  # shellcheck disable=SC2034
  local -a args=(
    'tests'    "Path to the bats test files"
  )
  :args "Run tests" "${@}"
  [[ -z "${BATS_LOAD:-}" ]] || {
    echo "Running tests for ${BATS_LOAD}" >&2
  }
  bats "${tests}"
}

argsh::coverage() {
  local out tests="." min=75
  # shellcheck disable=SC2034
  local -a args=(
    'out'       "Path to the output directory"
    'tests'     "Path to the bats test files"
    'min|:~int' "Minimum coverage required"
  )
  :args "Generate coverage report for your Bash scripts" "${@}"

  echo "Generating coverage report" >&2
  kcov \
    --clean \
    --bash-dont-parse-binary-dir \
    --include-pattern=.sh \
    --exclude-pattern=tests \
    --include-path=. \
    "${out}" bats "${tests}" >/dev/null 2>&1 || {
      echo "Failed to generate coverage report"
      echo "Run tests with 'argsh test' to see what went wrong"
      exit 1
    } >&2

  cp "${out}"/bats.*/coverage.json "${out}/coverage.json"
  local coverage
  coverage="$(jq -r '.percent_covered | tonumber | floor' "${out}/coverage.json")"

  echo "Coverage is ${coverage}% of required ${min}%"
  (( coverage > min )) || exit 1
}

argsh::docs() {
  local in out prefix=""
  # shellcheck disable=SC2034
  local -a args=(
    'in'     "Path to the source files to generate documentation from, can be a glob pattern"
    'out'    "Path to the output directory"
    'prefix' "Prefix for each md file"
  )
  :args "Generate documentation" "${@}"
  [[ -d "${out}" ]] || {
    :args::error_usage "out is not a directory"
    exit 1
  }

  if [[ -f "${prefix}" ]]; then
    prefix="$(cat "${prefix}")"
  elif [[ -d "${prefix}" || -f "${prefix}/_prefix.mdx" ]]; then
    prefix="$(cat "${prefix}/_prefix.mdx")"
  elif [[ -f "${out}/_prefix.mdx" ]]; then
    prefix="$(cat "${out}/_prefix.mdx")"
  fi

  local -a glob
  if [[ -d "${in}" ]]; then
    glob=("${in}"/*.{sh,bash,bats})
  else
    # shellcheck disable=SC2206 disable=SC2128
    glob=(${in})
  fi
  
  local file f name to
  for file in "${glob[@]}"; do
    [[ -e "${file}" ]] || continue

    name="${file##*/}"
    name="${name%.sh}"
    export name

    to="${out}/${name}.mdx"
    # shellcheck disable=SC2016
    echo "${prefix}" | envsubst '$name' >"${to}"
    shdoc <"${file}" >>"${to}"
  done
}

argsh::main() {
  local -a usage=(
    'minify:-argsh::minify'     "Minify Bash files"
    'lint:-argsh::lint'         "Lint Bash files"
    'test:-argsh::test'         "Run tests"
    'coverage:-argsh::coverage' "Generate coverage report for your Bash scripts"
    'docs:-argsh::docs'         "Generate documentation"
  )
  :usage "Enhance your Bash scripting by promoting structure and maintainability,
          making it easier to write, understand, 
          and maintain even complex scripts." "${@}"
  "${usage[@]}"
}

argsh::main "${@}"