#!/usr/bin/env bash
# @file args
# @brief Functions for working with arguments
# @description
#   This file contains functions for working with arguments
set -euo pipefail

: "${ARGSH_FIELD_WIDTH:=24}"
: "${ARGSH_PATH_IMPORT:=${BASH_SOURCE[0]%/*}}"

# @internal
# shellcheck disable=SC1090
import() { declare -A _i; (( ${_i[${1}]:-} )) || { _i[${1}]=1; . "${ARGSH_PATH_IMPORT}/${1}.sh"; } }
import string
import fmt
import is
import to
import error

# @brief
#   Parse command line arguments.
# @description
#   This function will parse the command line arguments and set the variables
#   for the user to use. It will also print the usage information if the user
#   passes the `-h` or `--help` flag.
# @arg $1 string The title of the usage
# @arg $@ array User arguments
# @set args array [get] The arguments to parse
# @exitcode 0 If user arguments are correct
# @exitcode 2 If user arguments are incorrect
# @example
#   local arg flag flag1 flag2 flag3 flag4 flag5 flag6="default"
#   local -a args
#   args=(
#     'arg'             "positional argument"       # required
#     'flag|'           "flag with value"
#     'flag1|f'         "flag with value and short"
#     'flag2|l:+'       "flag without value"
#     'flag3|a:!'       "required flag"             # required
#     'flag4|b:~float'  "flag with type"
#     'flag5|c:~int!'   "required flag with type"   # required
#     'flag6|d'         "flag with default"
#   )
#   :args "Title" "${@}"
#
#  echo "arg: ${arg}"
#  echo "flags: ${flag} ${flag1} ${flag2} ${flag3} ${flag4} ${flag5} ${flag6}"
:args() {
  local title="${1}"; shift
  declare -p args &>/dev/null || local -a args=()
  [[ $(( ${#args[@]} % 2 )) -eq 0 ]] ||
    :args::_error "args must be an associative array"

  args+=('help|h:+' "Show this help message")
  if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
    :args::text
    exit 0
  fi

  local field flag
  local positional_index=0
  local -A match=()
  local -a cli=("${@}")
  
  while (( ${#cli[@]} )); do
    # positional
    if [[ ${cli[0]:0:1} != "-" ]]; then
      field="${args[${positional_index}]}"
      [[ ${field} != *"|" ]] ||
        :args::error_usage "too many arguments: ${cli[0]}"

      local -n ref="${field/:*}"
      ref="$(:args::field-value "${cli[0]}")" || exit "${?}"
      unset 'cli[0]'; cli=("${cli[@]}")
      (( positional_index += 2 ))
      continue
    fi

    flag="${cli[0]/=*}"
    # -- long flag
    if [[ ${flag:0:2} == "--" ]]; then
      field="$(:args::field-lookup "${flag:2}" "${positional_index}")"
    # - short flag
    elif [[ ${flag:0:1} == "-" ]]; then
      flag="${flag:0:2}"
      field="$(:args::field-lookup "${flag:1}" "${positional_index}")"
    fi

    :args::field-set-flag "${field}"
    match["${field}"]=1
  done
  
  field="${args[${positional_index}]:-}"
  if [[ -n "${field}" && "${field}" != *"|"* ]]; then
    is::uninitialized "${field/:*}" ||
      :args::error_usage "missing required argument: ${args[${positional_index}]/:*}"
  fi

  for (( i=0; i < ${#args[@]}; i+=2 )); do
    [[ ${args[i]: -1} == "!" ]] || continue

    if [[ -z ${match[${args[i]}]:-} ]]; then
      :args::error_usage "missing required flag: ${args[i]/|*}"
    fi
  done

  [[ ${#cli[@]} -eq 0 ]] || 
    :args::error_usage "too many arguments: ${cli[*]}"
}

# @description
#   Print arguments usage information
# @set title string [get] The title of the usage
# @set positional array The positional arguments
# @set flags array The flags
# @internal
:args::text() {
  local -a flags=() positional=() params=()
  :args::positional
  :args::flags

  local base="${0##*/}"
  echo "${title}"
  echo
  echo "Usage:"
  echo "  ${base} ${FUNCNAME[2]/::*} ${params[*]}"

  (( ${#positional[@]} == 0 )) || {
    echo
    echo "Arguments:"
    for i in "${positional[@]}"; do
      desc="$(
        printf "   %-${ARGSH_FIELD_WIDTH}s%s" " " "${args[i+1]}" | fmt::tty
      )"
      printf "   %-${ARGSH_FIELD_WIDTH}s%s\n" \
        "$(:args::fieldf "${args[i]}")" \
        "$(string::trim-left "${desc}")"
    done
  }
  echo
  echo "Options:"
  (( ${#flags[@]} == 0 )) || {
    for i in "${flags[@]}"; do
      :args::fieldf "${args[i]}"
      {
        echo -n "           "
        echo -e "${args[i+1]}\n"
      } | fmt::tty
    done
  }
  echo
}

# @description
#   Set the flags
# @set args array [get] The arguments to parse
# @set flags array The flags
# @internal
:args::flags() {
  declare -p args &>/dev/null || local -a args
  declare -p flags &>/dev/null || local -a flags

  for (( i=0; i < "${#args[@]}"; i+=2 )); do
    if [[ ${args[i]} == *"|"* ]]; then
      flags+=("${i}")
    fi
  done
}

# @description
#   Set the positional arguments
# @set args array [get] The arguments to parse
# @set positional array The positional arguments
# @set params array The parameters
# @internal
:args::positional() {
  declare -p args &>/dev/null || local -a args
  declare -p positional &>/dev/null || local -a positional
  declare -p params &>/dev/null || local -a params

  for (( i=0; i < "${#args[@]}"; i+=2 )); do
    [[ ${args[i]} != *"|"* ]] || continue

    positional+=("${i}")
    if is::uninitialized "${args[i]/:*}"; then
      params+=("[${args[i]/:*}]")
      continue
    fi
    params+=("<${args[i]/:*}>")
  done
}

# @description
#   Set the flag value
# @arg $1 string The field to set
# @set cli array [get] The command line arguments
# @set flag string The flag value (cli)
# @internal
# shellcheck disable=SC2034
:args::field-set-flag() {
  local field="${1}"
  declare -p cli flag &>/dev/null || return 1

  local -a attrs
  :args::field-attrs "${field}"

  local -n ref="${attrs[0]}"
  local set_value cli_value

  # is it no-value?
  if (( attrs[2] )); then
    set_value=1

    if [[ ${flag:0:2} == "--" ]]; then
      unset 'cli[0]'; cli=("${cli[@]}")
    else
      cli[0]="-${cli[0]:2}"
      [[ ${cli[0]} != "-" ]] || { unset 'cli[0]'; cli=("${cli[@]}"); }
    fi
  fi

  # is it a type?
  [[ -n ${set_value:-} ]] || {
    cli_value="${cli[0]/${flag}}"
    if [[ ${cli_value} == "" ]]; then
      (( ${#cli[@]} )) ||
        :args::error "missing value for flag: ${attrs[0]}"
      
      set_value="${cli[1]}"
      unset 'cli[1]'; cli=("${cli[@]}")
    else
      [[ "${cli_value:0:1}" != "=" ]] ||
        cli_value="${cli_value:1}"
      set_value="${cli_value}"
    fi
    set_value="$(:args::field-value "${set_value}")" || exit "${?}"
    unset 'cli[0]'; cli=("${cli[@]}")
  }

  if (( attrs[5] )); then
    ref+=("${set_value}")
  else
    # shellcheck disable=SC2178
    ref="${set_value}"
  fi
}

# @description
#   Transform the value to the correct type
# @arg $1 string The value to transform
# @set field string [get] The field to transform
# @set attrs array [get] The field attributes
# @stdout The transformed value
# @exitcode 0 If the value is transformed
# @exitcode 1 If the type is unknown
# @internal
:args::field-value() {
  local value="${1}"
  declare -p field &>/dev/null || return 1
  declare -p attrs &>/dev/null || {
    local -a attrs
    :args::field-attrs "${field}"
  }
  declare -f "to::${attrs[3]}" &>/dev/null ||
    :args::_error "unknown type: ${attrs[3]}"

  "to::${attrs[3]}" "${value}" "${attrs[0]}" ||
    :args::error_usage "invalid type (${attrs[3]}): ${value}"
}

# @description
#   Lookup a field in the args array
# @arg $1 string The field to lookup
# @arg $2 int The start index
# @set args array [get] The arguments to parse
# @stdout The field value
# @exitcode 0 If the field is found
# @exitcode 1 If the field is not found
# @internal
:args::field-lookup() {
  local field="${1}"
  local start="${2:-0}"
  declare -p args &>/dev/null || return 1

  for (( i=start; i < ${#args[@]}; i+=2 )); do
    if [[ ${args[i]} =~ (^${field}\||\|${field}:|\|${field}$) ]]; then
      echo "${args[i]}"
      return 0
    fi
  done
  :args::error_usage "unknown flag"
}

# @description
#   Get/Parse the attributes of a field
# @arg $1 string The field to parse
# @set attrs array [get] The field attributes
# @internal
:args::field-attrs() {
  local field="${1}"
  declare -p attrs &>/dev/null || local -a attrs
  attrs=(
    ""  # 0 name
    ""  # 1 short
    0   # 2 boolean
    ""  # 3 type
    0   # 4 has default
    0   # 5 multiple
    0   # 6 required
  )

  local seps="+~!"
  local mods="${field#*[:]}"
  [ "${mods}" != "${field}" ] || mods=""
  # set name
  attrs[0]="${field/[|:]*}"
  # shellcheck disable=SC2178
  local -n ref="${attrs[0]}"
  local -a flags
  mapfile -t flags < <(echo "${field/[:]*}" | tr '|' '\n')
  [[ ${#flags[@]} -eq 1 ]] || {
    attrs[0]="${flags[0]}"
    attrs[1]="${flags[1]}"
  }
  # multiple
  if is::array "${attrs[0]}"; then
    attrs[5]=1
    ! is::uninitialized "${attrs[0]}" ||
      ref=()

    # default
    ! (( ${#ref[@]} )) || 
      attrs[4]=1
  elif is::uninitialized "${attrs[0]}"; then
      attrs[4]=1
  fi
  
  # loop through modifiers
  while (( ${#mods} > 0 )); do
    # is boolean - no value
    if [[ ${mods:0:1} == "+" ]]; then
      [[ -z ${attrs[3]} ]] ||
        :args::_error "cannot have multiple types: ${attrs[3]} and boolean"

      attrs[2]=1
      mods="${mods:1}"
      continue
    fi
    # type
    if [[ ${mods:0:1} == "~" ]]; then
      ! (( attrs[2] )) ||
        :args::_error "already flagged as boolean"

      mods="${mods:1}"
      attrs[3]="${mods/[$seps]*}"
      mods="${mods:${#attrs[3]}}"
      continue
    fi
    # required
    if [[ ${mods:0:1} == "!" ]]; then
      ! (( attrs[4] )) ||
        :args::_error "cannot be required with default value"
      ! (( attrs[6] )) ||
        :args::_error "field already flagged as required"

      attrs[6]=1
      mods="${mods:1}"
      continue
    fi
    echo ":args error: unknown modifier: ${mods:0:1}" >&2
    exit 2
  done
  if [[ -z ${attrs[3]} && ${attrs[2]} -eq 0 ]]; then 
    attrs[3]="string"
  fi
}

# @description
#   Print the field formated
# @arg $1 string The field to format
# @set attrs array [get] The field attributes
# @stdout The field formated
# @internal
:args::fieldf() {
  local field="${1}"
  declare -p attrs &>/dev/null || {
    local -a attrs
    :args::field-attrs "${field}"
  }

  [[ ${field} == *"|"* ]] || {
    echo "${field/:*} ${attrs[3]}"
    return 0
  }

  # shellcheck disable=SC2178
  local -n ref="${attrs[0]}"

  format="   "
  # required
  ! (( attrs[6] )) ||
    format=" ! "

  if [[ -n ${attrs[1]} ]]; then
    format+="-${attrs[1]}, --${attrs[0]}"
  else
    format+="    --${attrs[0]}"
  fi
  
  format+=" "
  # multiple
  ! (( attrs[5] )) ||
    format+="..."
  # type
  format+="${attrs[3]}"
  # default value
  ! (( attrs[4] )) ||
    format+=" (default: ${ref[*]})"
  
  echo "${format}"
}
