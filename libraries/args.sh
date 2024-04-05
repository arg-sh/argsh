#!/usr/bin/env bash
# @file args
# @brief Functions for working with arguments
# @description
#   This file contains functions for working with arguments
set -euo pipefail

: "${ARGSH_VERSION:=unknown}"
: "${ARGSH_COMMIT_SHA:=unknown}"
: "${ARGSH_FIELD_WIDTH:=24}"
# obfus ignore variable
COMMANDNAME=("$(s="${ARGSH_SOURCE:-"${0}"}"; echo "${s##*/}")")

# @internal
# shellcheck disable=SC1090
import() { declare -A _i; (( ${_i[${1}]:-} )) || { _i[${1}]=1; . "${BASH_SOURCE[0]%/*}/${1}.sh"; } }
import string
import fmt
import is
import to
import error
import array

# @description Print usage information
# @arg $1 string The title of the usage
# @arg $@ array User arguments
# @set usage array Usage information for the command
# @exitcode 0 If user arguments are correct
# @exitcode 2 If user arguments are incorrect
# @example
#   local -a usage
#   usage=(
#     command "Description of command"
#     [...]
#   )
#  :usage "Title" "${@}"
:usage() {
  local title="${1}"; shift
  declare -p usage &>/dev/null || local -a usage=()
  declare -p args &>/dev/null || local -a args=()
  [[ $(( ${#usage[@]} % 2 )) -eq 0 ]] ||
    :args::_error "usage must be an associative array"
  [[ $(( ${#usage[@]} % 2 )) -eq 0 ]] ||
    :args::_error "usage must be an associative array"

  if [[ -z ${1:-} || ${1} == "-h" || ${1} == "--help" ]]; then
    :usage::text "${title}"
    exit 0
  fi
  if ! (( ${#COMMANDNAME[@]} )) && [[ ${1:-} == "--argsh" ]]; then
    echo "https://arg.sh ${ARGSH_COMMIT_SHA:-} ${ARGSH_VERSION:-}"
    exit 0
  fi

  local -A match=()
  local -a cli=("${@}")
  local cmd field=""
  
  while (( ${#cli[@]} )); do
    # command
    if [[ ${cli[0]:0:1} != "-" ]]; then
      [[ -z "${cmd:-}" ]] || break
      cmd="${cli[0]}"
      cli=("${cli[@]:1}")
      continue
    fi

    :args::parse_flag || break
    match["${field}"]=1
  done
  :args::check_required_flags

  local func
  for (( i=0; i < ${#usage[@]}; i+=2 )); do
    for alias in $(echo "${usage[i]/:*}" | tr '|' "\n"); do
      alias="${alias#\#}"
      [[ "${cmd}" == "${alias}" ]] || continue
      field="${usage[i]#\#}"

      func="${usage[i]/*:-}"
      func="${func#\#}"
      [[ "${func}" == "${usage[i]}" ]] || break 2
      
      func="${func/|*}"
      break 2
    done
  done

  [[ -n "${func:-}" ]] ||
    :args::error_usage "Invalid command: ${cmd}"

  # obfus ignore variable
  COMMANDNAME+=("${field/[|:]*}")
  # obfus ignore variable
  usage=("${func}" "${cli[@]}")
}

# @description Print usage information
# @arg $1 string The title of the usage
# @set usage array Usage information for the command
# @internal
:usage::text() {
  local title="${1:-}"
  string::indent "${title}"
  echo
  echo "Usage: ${COMMANDNAME[*]} <command> [args]"
  [[ ${usage[0]:-} == '-' ]] ||
    echo -e "\nAvailable Commands:"
  for (( i=0; i < ${#usage[@]}; i+=2 )); do
    [[ "${usage[i]:0:1}" != "#" ]] || continue
    [[ "${usage[i]}" != "-" ]] || {
      echo
      echo "${usage[i+1]}"
      continue
    }
    printf "  %-${ARGSH_FIELD_WIDTH}s %s\n" "${usage[i]/[:|]*}" "${usage[i+1]}"
  done
  :args::text_flags
  echo
  echo "Use \"${COMMANDNAME[*]} <command> --help\" for more information about a command."
}

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

  if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
    :args::text
    exit 0
  fi

  local first=0 field="" i positional_index=1
  local -A match=()
  local -a cli=("${@}")
  
  while (( ${#cli[@]} )); do
    # positional
    if [[ ${cli[0]:0:1} != "-" ]]; then
      local name value
      i="$(:args::field_positional "${positional_index}")" ||
        :args::error_usage "too many arguments: ${cli[0]}"
      
      field="${args[i]}"
      name="$(args::field_name "${field}")"
      value="$(:args::field_value "${cli[0]}")" || exit "${?}"
    
      # shellcheck disable=SC2155
      local -n ref="${name}"
      if is::array "${name}"; then
        (( first )) || {
          ref=()
          first=1
        }
        ref+=("${value}")
      else
        # shellcheck disable=SC2178
        ref="${value}"
      fi
      cli=("${cli[@]:1}")
      (( ++positional_index ))
      continue
    fi

    :args::parse_flag || 
      :args::error_usage "unknown flag: ${cli[0]}"
    match["${field}"]=1
  done
  
  if i="$(:args::field_positional "${positional_index}")"; then
    field="$(args::field_name "${args[i]}")"
    if is::uninitialized "${field}" && ! is::array "${field}"; then
      :args::error_usage "missing required argument: ${field}"
    fi
  fi

  :args::check_required_flags
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
  declare -p args &>/dev/null || return 0
  local -a positional=() params=()
  :args::positional

  string::indent "${title}"
  echo
  echo "Usage:"
  echo "  ${COMMANDNAME[*]} ${params[*]}"

  (( ${#positional[@]} == 0 )) || {
    echo
    echo "Arguments:"
    for i in "${positional[@]}"; do
      [[ ${args[i]} != "-" ]] || continue
      desc="$(
        printf "   %-${ARGSH_FIELD_WIDTH}s%s" " " "${args[i+1]}" | fmt::tty
      )"
      printf "   %-${ARGSH_FIELD_WIDTH}s%s\n" \
        "$(:args::fieldf "${args[i]}")" \
        "$(string::trim-left "${desc}")"
    done
  }
  :args::text_flags
  echo
}

:args::text_flags() {
  # we make a copy here as we add --help to the flags
  # obfus ignore variable
  local -a args=("${args[@]}")
  local -a flags=()
  array::contains 'help|h:+' "${args[@]}" || args+=('help|h:+' "Show this help message")
  :args::flags
  (( ${#flags[@]} )) || return 0

  [[ "${args[${flags[0]}]}" == "-" ]] ||
    echo -e "\nOptions:"
  for i in "${flags[@]}"; do
    [[ "${args[i]:0:1}" != "#" ]] || continue
    [[ "${args[i]}" != "-" ]] || {
      echo
      echo "${args[i+1]}"
      continue
    }

    :args::fieldf "${args[i]}"
    {
      echo -n "           "
      echo -e "${args[i+1]}\n"
    } | fmt::tty
  done
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
    if [[ ${args[i]} == *"|"* || ${args[i]} == '-' ]]; then
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
  local ref

  for (( i=0; i < "${#args[@]}"; i+=2 )); do
    [[ ${args[i]} != *"|"* && ${args[i]} != '-' ]] || continue
    ref="$(args::field_name "${args[i]}")"
    
    positional+=("${i}")
    if is::array "${ref}"; then
      params+=("...${ref}")
      continue
    fi
    if ! is::uninitialized "${ref}"; then
      params+=("[${ref}]")
      continue
    fi
    params+=("<${ref}>")
  done
}

:args::parse_flag() {
  declare -p cli field &>/dev/null || return 1
  local flag="${cli[0]/=*}"
  # -- long flag
  if [[ ${flag:0:2} == "--" ]]; then
    field="$(:args::field_lookup "${flag:2}")" || return "${?}"
  # - short flag
  elif [[ ${flag:0:1} == "-" ]]; then
    flag="${flag:0:2}"
    field="$(:args::field_lookup "${flag:1}")" || return "${?}"
  fi

  :args::field_set_flag "${field}"
}

# @description
#   Check for required flags
#   Set boolean flags to false if not set
# @set match array [get] The matched flags
# @set args array [get] The arguments to parse
# @internal
:args::check_required_flags() {
  declare -p match args &>/dev/null || return 1
  local field
  local -a attrs

  for (( i=0; i < ${#args[@]}; i+=2 )); do
    field="${args[i]}"
    :args::field_attrs "${field}"

    # set boolean to false if not set
    if (( attrs[2] )) && ! (( attrs[4] )); then
      local -n ref="${attrs[0]}"
      ref=0
    fi

    # is it required? was it matched?
    if (( attrs[6] )) && [[ -z ${match[${args[i]}]:-} ]]; then
      :args::error_usage "missing required flag: ${args[i]/|*}"
    fi
  done
}

# @description
#   Set the flag value
# @arg $1 string The field to set
# @set cli array [get] The command line arguments
# @set flag string The flag value (cli)
# @internal
# shellcheck disable=SC2034
:args::field_set_flag() {
  local field="${1}"
  declare -p cli flag &>/dev/null || return 1

  local -a attrs
  :args::field_attrs "${field}"

  local -n ref="${attrs[0]}"
  local set_value cli_value

  # is it no-value?
  if (( attrs[2] )); then
    set_value=1

    if [[ ${flag:0:2} == "--" ]]; then
      cli=("${cli[@]:1}")
    else
      cli[0]="-${cli[0]:2}"
      [[ ${cli[0]} != "-" ]] || cli=("${cli[@]:1}")
    fi
  fi

  # is it a type?
  [[ -n ${set_value:-} ]] || {
    cli_value="${cli[0]/${flag}}"
    if [[ ${cli_value} == "" ]]; then
      (( ${#cli[@]} )) ||
        :args::error "missing value for flag: ${attrs[0]}"
      
      set_value="${cli[1]}"
      cli=("${cli[@]:1}")
    else
      [[ "${cli_value:0:1}" != "=" ]] ||
        cli_value="${cli_value:1}"
      set_value="${cli_value}"
    fi
    set_value="$(:args::field_value "${set_value}")" || exit "${?}"
    cli=("${cli[@]:1}")
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
:args::field_value() {
  local value="${1}"
  declare -p field &>/dev/null || return 1
  declare -p attrs &>/dev/null || {
    local -a attrs
    :args::field_attrs "${field}"
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
:args::field_lookup() {
  local field="${1}"
  declare -p args &>/dev/null || return 1

  for (( i=0; i < ${#args[@]}; i+=2 )); do
    if [[ ${args[i]} =~ (^${field}\||\|${field}:|\|${field}$) ]]; then
      echo "${args[i]}"
      return 0
    fi
  done
  return 1
}

# @description
#   Lookup nth positional field.
#   If a field is an array, it will end and return the index.
# @arg $1 int The position
# @set args array [get] The arguments to parse
# @stdout The field position
# @exitcode 0 If the field is found
# @exitcode 1 If the field is not found
# @internal
:args::field_positional() {
  local position="${1:-1}"
  declare -p args &>/dev/null || return 1

  for (( i=0; i < ${#args[@]}; i+=2 )); do
    if [[ ${args[i]} != *"|"* && ${args[i]} != '-' ]]; then
      if is::array "$(args::field_name "${args[i]}")" || (( --position == 0 )); then
        echo "${i}"
        return 0
      fi
    fi
  done 
  return 1
}

# @description
#   Get the field variable reference name
# @arg $1 string The field to parse
# @stdout The field variable reference name
# @internal
args::field_name() {
  local field="${1}"
  local asref="${2:-1}"
  field="${field/[|:]*}"
  field="${field#\#}"
  if (( asref )); then
    field="${field//-/_}"
  fi
  echo "${field}"
}

# @description
#   Get/Parse the attributes of a field
# @arg $1 string The field to parse
# @set attrs array [get] The field attributes
# @internal
:args::field_attrs() {
  local field="${1}"
  declare -p attrs &>/dev/null || local -a attrs
  attrs=(
    ""  # 0 name
    ""  # 1 short
    0   # 2 boolean
    ""  # 3 type
    0   # 4 has value
    0   # 5 multiple
    0   # 6 required
    0   # 7 hidden
    ""  # 8 display name
  )

  local seps="+~!"
  local mods="${field#*[:]}"
  [ "${mods}" != "${field}" ] || mods=""
  # set name
  attrs[0]="$(args::field_name "${field}")"
  # display name
  attrs[8]="$(args::field_name "${field}" 0)"
  # hidden
  [[ ${attrs[0]:0:1} != "#" ]] || {
    attrs[7]=1
  }
  # shellcheck disable=SC2178
  local -n ref="${attrs[0]}"
  local -a flags
  mapfile -t flags < <(echo "${field/[:]*}" | tr '|' '\n')
  [[ ${#flags[@]} -eq 1 ]] || {
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
  elif ! is::uninitialized "${attrs[0]}"; then
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
    :args::field_attrs "${field}"
  }

  [[ ${field} == *"|"* ]] || {
    echo "${attrs[8]} ${attrs[3]}"
    return 0
  }

  # shellcheck disable=SC2178
  local -n ref="${attrs[0]}"

  format="   "
  # required
  ! (( attrs[6] )) ||
    format=" ! "

  if [[ -n ${attrs[1]} ]]; then
    format+="-${attrs[1]}, --${attrs[8]}"
  else
    format+="    --${attrs[8]}"
  fi
  
  format+=" "
  # multiple
  ! (( attrs[5] )) ||
    format+="..."
  # type
  format+="${attrs[3]}"
  # default value
  if (( attrs[4] )) && ! (( attrs[2] )); then
    format+=" (default: ${ref[*]})"
  fi
  
  echo "${format}"
}
