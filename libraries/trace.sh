#!/usr/bin/env bash
# @file trace
# @brief Process trace via ARGSH_TRACE env var
# @description
#   When ARGSH_TRACE is set to a file path, writes a structured markdown
#   trace of the script's execution. Tracks function entry/exit with timing,
#   variable state after :args calls, and command dispatch via :usage.
#
#   Activation: export ARGSH_TRACE=/tmp/trace.md
#   The trace file is written incrementally during execution.
set -euo pipefail

# Guard: only define trace functions when ARGSH_TRACE is set to a non-empty
# file path. When tracing is off, define no-op stubs so call sites don't
# need conditionals.
if [[ -z "${ARGSH_TRACE:-}" ]]; then
  # No-op stubs — zero overhead when tracing is disabled.
  __argsh_trace_init()  { :; }
  __argsh_trace_args()  { :; }
  __argsh_trace_usage() { :; }
  return 0 2>/dev/null || exit 0
fi

# ── Internal state ──────────────────────────────────────────────────────
# obfus ignore variable
declare -g  __ARGSH_TRACE_FILE="${ARGSH_TRACE}"
# obfus ignore variable
declare -gi __ARGSH_TRACE_START=0
# obfus ignore variable
declare -gi __ARGSH_TRACE_STEP=0
# obfus ignore variable
declare -gi __ARGSH_TRACE_DEPTH=0
# obfus ignore variable
declare -ga __ARGSH_TRACE_STACK=()

# ── Helpers ─────────────────────────────────────────────────────────────

# Milliseconds since epoch (bash 5+ has EPOCHREALTIME; fall back to date).
__argsh_trace_ms() {
  if [[ -n "${EPOCHREALTIME:-}" ]]; then
    local s="${EPOCHREALTIME%.*}"
    local f="${EPOCHREALTIME#*.}"
    # EPOCHREALTIME has 6 fractional digits; take first 3 for ms.
    echo $(( s * 1000 + 10#${f:0:3} ))
  else
    date +%s%3N 2>/dev/null || echo $(( $(date +%s) * 1000 ))
  fi
}

# Write a line to the trace file.
__argsh_trace_write() {
  echo "${*}" >> "${__ARGSH_TRACE_FILE}"
}

# ── Initialization ──────────────────────────────────────────────────────

# @description Initialize the trace file with a header section.
# Called once from main.sh after ARGSH_SOURCE is set.
# @arg $@ string Original script arguments
__argsh_trace_init() {
  __ARGSH_TRACE_START="$(__argsh_trace_ms)"
  __ARGSH_TRACE_STEP=0
  __ARGSH_TRACE_DEPTH=0
  __ARGSH_TRACE_STACK=()

  # Truncate and write header
  {
    echo "# Process Trace"
    echo ""
    echo "- **Script**: ${ARGSH_SOURCE:-unknown}"
    echo "- **Date**: $(date -Iseconds 2>/dev/null || date)"
    echo "- **Args**: \`${*}\`"
    echo "- **PID**: $$"
    echo ""
    echo "## Execution"
    echo ""
  } > "${__ARGSH_TRACE_FILE}"

  # Set up the DEBUG trap for function entry/exit tracking.
  # The trap fires before every simple command; we filter to only
  # record function call/return boundaries.
  trap '__argsh_trace_debug_hook' DEBUG

  # Set up EXIT trap to write the summary section.
  trap '__argsh_trace_exit_hook $?' EXIT
}

# ── DEBUG trap ──────────────────────────────────────────────────────────

__argsh_trace_debug_hook() {
  # Only trace at function boundaries: FUNCNAME[0] is this hook,
  # FUNCNAME[1] is the calling context. Track depth changes.
  local _depth=$(( ${#FUNCNAME[@]} - 2 ))  # subtract hook + main
  (( _depth >= 0 )) || _depth=0

  # Skip internal trace functions to avoid recursion.
  case "${FUNCNAME[1]:-}" in
    __argsh_trace_*) return 0 ;;
  esac

  local _func="${FUNCNAME[1]:-main}"
  local _now

  if (( _depth > __ARGSH_TRACE_DEPTH )); then
    # Function entry
    _now="$(__argsh_trace_ms)"
    __ARGSH_TRACE_STACK+=("${_func}:${_now}")
    __ARGSH_TRACE_DEPTH=${_depth}
    (( ++__ARGSH_TRACE_STEP ))
    local _indent=""
    local _i
    for (( _i=0; _i < _depth; _i++ )); do _indent+="  "; done
    __argsh_trace_write "${_indent}- **[${__ARGSH_TRACE_STEP}]** \`${_func}\` enter (+$(( _now - __ARGSH_TRACE_START ))ms)"
  elif (( _depth < __ARGSH_TRACE_DEPTH )); then
    # Function exit — pop stack entries until we match depth
    _now="$(__argsh_trace_ms)"
    while (( ${#__ARGSH_TRACE_STACK[@]} > _depth )); do
      local _top="${__ARGSH_TRACE_STACK[-1]}"
      unset '__ARGSH_TRACE_STACK[-1]'
      local _top_func="${_top%%:*}"
      local _top_start="${_top##*:}"
      local _elapsed=$(( _now - _top_start ))
      local _indent=""
      local _d=$(( ${#__ARGSH_TRACE_STACK[@]} + 1 ))
      local _i
      for (( _i=0; _i < _d; _i++ )); do _indent+="  "; done
      __argsh_trace_write "${_indent}- \`${_top_func}\` exit (${_elapsed}ms)"
    done
    __ARGSH_TRACE_DEPTH=${_depth}
  fi
}

# ── :args integration ───────────────────────────────────────────────────

# @description Write variable state after a :args call to the trace file.
# Called from :args (pure bash) after parsing completes.
# @arg $1 string The title passed to :args
# Reads `args` array from caller scope.
__argsh_trace_args() {
  local _title="${1:-}"
  local _now
  _now="$(__argsh_trace_ms)"
  local _indent=""
  local _i
  for (( _i=0; _i <= __ARGSH_TRACE_DEPTH; _i++ )); do _indent+="  "; done

  __argsh_trace_write ""
  __argsh_trace_write "${_indent}<details><summary>Variables after <code>:args \"${_title}\"</code> (+$(( _now - __ARGSH_TRACE_START ))ms)</summary>"
  __argsh_trace_write ""
  __argsh_trace_write "${_indent}\`\`\`"

  # Dump each variable defined in the args array.
  # The args array has pairs: 'field_spec' 'description'.
  # We extract the field name and print its current value.
  declare -p args &>/dev/null 2>&1 || { __argsh_trace_write "${_indent}\`\`\`"; __argsh_trace_write "${_indent}</details>"; __argsh_trace_write ""; return 0; }
  local _fi _fname _fval
  for (( _fi=0; _fi < ${#args[@]}; _fi+=2 )); do
    [[ "${args[_fi]}" != "-" ]] || continue
    _fname="${args[_fi]}"
    _fname="${_fname/[|:]*}"
    _fname="${_fname#\#}"
    _fname="${_fname//-/_}"
    [[ -n "${_fname}" ]] || continue
    # Use declare -p to get the value representation safely.
    _fval="$(declare -p "${_fname}" 2>/dev/null)" || _fval="${_fname}=(unset)"
    # Strip the declare prefix to just show name=value.
    _fval="${_fval#declare -* }"
    __argsh_trace_write "${_indent}${_fval}"
  done

  __argsh_trace_write "${_indent}\`\`\`"
  __argsh_trace_write "${_indent}</details>"
  __argsh_trace_write ""
}

# ── :usage integration ──────────────────────────────────────────────────

# @description Record the command path after :usage dispatches.
# Called from :usage after resolving the subcommand.
# @arg $1 string The resolved function name
# @arg $2 string The command name the user typed
__argsh_trace_usage() {
  local _func="${1:-}" _cmd="${2:-}"
  local _now
  _now="$(__argsh_trace_ms)"
  local _indent=""
  local _i
  for (( _i=0; _i <= __ARGSH_TRACE_DEPTH; _i++ )); do _indent+="  "; done
  __argsh_trace_write "${_indent}- dispatch: \`${_cmd}\` -> \`${_func}\` (+$(( _now - __ARGSH_TRACE_START ))ms)"
}

# ── EXIT trap ───────────────────────────────────────────────────────────

__argsh_trace_exit_hook() {
  local _exit_code="${1:-$?}"
  local _now
  _now="$(__argsh_trace_ms)"
  local _elapsed=$(( _now - __ARGSH_TRACE_START ))

  {
    echo ""
    echo "## Summary"
    echo ""
    echo "- **Steps**: ${__ARGSH_TRACE_STEP}"
    echo "- **Duration**: ${_elapsed}ms"
    echo "- **Exit code**: ${_exit_code}"
  } >> "${__ARGSH_TRACE_FILE}"
}
