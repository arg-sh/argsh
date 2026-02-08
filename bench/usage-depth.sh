#!/usr/bin/env bash
# Benchmark: pure-bash vs native builtin at varying subcommand depths.
# Usage: bash bench/usage-depth.sh
#
# Tests two workloads:
#   1. Subcommand dispatch: root x x x ... x -h  (N :usage levels)
#   2. Argument parsing:    cmd --flag1 v1 ... -h (N flags via :args)
set -euo pipefail

: "${PATH_BASE:="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"}"
: "${ITERATIONS:=50}"

# ── Generate N-deep subcommand tree ─────────────────────────────────────────
generate_tree() {
  local depth=$1 prefix="bench"

  for ((i = 1; i < depth; i++)); do
    eval "${prefix}() {
      local -a usage=('x' \"Level $((i + 1))\")
      :usage \"Level ${i}\" \"\${@}\"; \"\${usage[@]}\"
    }"
    prefix="${prefix}::x"
  done
  # Leaf: :args with -h triggers help + exit 0
  eval "${prefix}() { :args \"Leaf\" \"\${@}\"; }"
}

# ── Clean up generated functions ────────────────────────────────────────────
cleanup_tree() {
  local depth=$1 prefix="bench"
  for ((i = 1; i < depth; i++)); do
    unset -f "${prefix}" 2>/dev/null || true
    prefix="${prefix}::x"
  done
  unset -f "${prefix}" 2>/dev/null || true
}

# ── Generate :args benchmark function with N flags ──────────────────────────
generate_args_bench() {
  local nflags=$1
  local args_def=""
  local locals=""
  for ((i = 1; i <= nflags; i++)); do
    locals+="local flag${i}; "
    args_def+="'flag${i}|f${i}' \"Flag ${i}\" "
  done
  eval "bench_args() {
    ${locals}
    local -a args=(${args_def})
    :args \"Bench with ${nflags} flags\" \"\${@}\"
  }"
}

# ── Measure without subshell (for non-exiting commands) ─────────────────────
measure_direct() {
  local -a cmd=("${@}")
  local start end
  start=$(date +%s%N)
  for ((i = 0; i < ITERATIONS; i++)); do
    "${cmd[@]}" >/dev/null 2>&1 || true
  done
  end=$(date +%s%N)
  echo $(( (end - start) / 1000000 ))
}

# ── Measure total time for $ITERATIONS calls (ms) ──────────────────────────
measure() {
  local -a cmd=("${@}")
  local start end
  start=$(date +%s%N)
  for ((i = 0; i < ITERATIONS; i++)); do
    ( "${cmd[@]}" ) >/dev/null 2>&1 || true
  done
  end=$(date +%s%N)
  echo $(( (end - start) / 1000000 ))
}

# ── Fork-only baseline (measures subshell overhead) ─────────────────────────
measure_fork_baseline() {
  local start end
  start=$(date +%s%N)
  for ((i = 0; i < ITERATIONS; i++)); do
    ( true ) 2>/dev/null
  done
  end=$(date +%s%N)
  echo $(( (end - start) / 1000000 ))
}

# ── Disable builtins (force pure-bash path) ─────────────────────────────────
disable_builtins() {
  # shellcheck disable=SC2229
  enable -d :usage :args 2>/dev/null || true
  # Hide .so from search so re-source doesn't re-load builtins
  local saved_bin="${PATH_BIN:-}"
  unset PATH_BIN PATH_LIB ARGSH_BUILTIN_PATH 2>/dev/null || true
  ARGSH_BUILTIN=0
  # shellcheck source=/dev/null
  source "${PATH_BASE}/libraries/args.sh"
  [[ -z "${saved_bin}" ]] || PATH_BIN="${saved_bin}"
}

# ── Enable builtins ─────────────────────────────────────────────────────────
enable_builtins() {
  local so="${PATH_BASE}/builtin/target/release/libargsh.so"
  [[ -f "${so}" ]] || {
    echo "ERROR: ${so} not found. Run: cd builtin && cargo build --release" >&2
    return 1
  }
  # shellcheck disable=SC2229
  enable -f "${so}" :usage :args 2>/dev/null || {
    echo "ERROR: could not load builtins from ${so}" >&2
    return 1
  }
  # Functions shadow builtins in bash — must unset to let builtins take effect
  unset -f :usage :args 2>/dev/null || true
  ARGSH_BUILTIN=1
}

calc_speedup() {
  local bash_ms=$1 builtin_ms=$2
  if (( builtin_ms > 0 )); then
    awk "BEGIN { printf \"%.1f\", ${bash_ms} / ${builtin_ms} }"
  else
    echo "inf"
  fi
}

# ── Main ────────────────────────────────────────────────────────────────────
# shellcheck source=/dev/null
source "${PATH_BASE}/libraries/args.sh"

fork_ms=$(measure_fork_baseline)

printf "Benchmark: pure-bash vs native builtin\n"
printf "Iterations: %d | Fork baseline: %d ms\n\n" "${ITERATIONS}" "${fork_ms}"

# ── Part 1: :usage subcommand dispatch depth ────────────────────────────────
printf "### Subcommand dispatch (\`cmd x x ... x -h\`)\n\n"
printf "| %-5s | %12s | %12s | %7s |\n" "Depth" "Pure Bash" "Builtin" "Speedup"
printf "|-------|--------------|--------------|---------|"

for depth in 10 25 50; do
  cmd=(bench)
  for ((j = 1; j < depth; j++)); do cmd+=(x); done
  cmd+=(-h)

  disable_builtins
  generate_tree "${depth}"
  raw_bash=$(measure "${cmd[@]}")
  ms_bash=$(( raw_bash - fork_ms ))
  (( ms_bash > 0 )) || ms_bash=1
  cleanup_tree "${depth}"

  enable_builtins
  generate_tree "${depth}"
  raw_builtin=$(measure "${cmd[@]}")
  ms_builtin=$(( raw_builtin - fork_ms ))
  (( ms_builtin > 0 )) || ms_builtin=1
  cleanup_tree "${depth}"

  printf "\n| %-5d | %9d ms | %9d ms | %5sx |" \
    "${depth}" "${ms_bash}" "${ms_builtin}" "$(calc_speedup "${ms_bash}" "${ms_builtin}")"
done

printf "\n\n"

# ── Part 2: :args flag parsing (actual values, no subshell) ─────────────────
printf "### Argument parsing (\`cmd --flag1 v1 ... --flagN vN\`)\n\n"
printf "| %-5s | %12s | %12s | %7s |\n" "Flags" "Pure Bash" "Builtin" "Speedup"
printf "|-------|--------------|--------------|---------|"

for nflags in 10 25 50; do
  # Build flag args: --flag1 val1 --flag2 val2 ...
  flag_args=()
  for ((j = 1; j <= nflags; j++)); do
    flag_args+=("--flag${j}" "val${j}")
  done

  disable_builtins
  generate_args_bench "${nflags}"
  ms_bash=$(measure_direct bench_args "${flag_args[@]}")
  (( ms_bash > 0 )) || ms_bash=1

  enable_builtins
  generate_args_bench "${nflags}"
  ms_builtin=$(measure_direct bench_args "${flag_args[@]}")
  (( ms_builtin > 0 )) || ms_builtin=1

  printf "\n| %-5d | %9d ms | %9d ms | %5sx |" \
    "${nflags}" "${ms_bash}" "${ms_builtin}" "$(calc_speedup "${ms_bash}" "${ms_builtin}")"
done

printf "\n\n"

# ── Part 3: Real-world — :usage + :args at every level ─────────────────────
# Each level: :usage dispatches 'x', plus 2 flags parsed per level.
# Call: bench --f1 v1 --f2 v2 x --f1 v1 --f2 v2 x ... -h

generate_real_tree() {
  local depth=$1 prefix="bench_real"

  for ((i = 1; i < depth; i++)); do
    eval "${prefix}() {
      local f1 f2
      local -a args=('f1' \"Flag 1\" 'f2' \"Flag 2\")
      local -a usage=('x' \"Level $((i + 1))\")
      :usage \"Level ${i}\" \"\${@}\"; \"\${usage[@]}\"
    }"
    prefix="${prefix}::x"
  done
  # Leaf: :args with flags + -h triggers help
  eval "${prefix}() {
    local f1 f2
    local -a args=('f1' \"Flag 1\" 'f2' \"Flag 2\")
    :args \"Leaf\" \"\${@}\"
  }"
}

cleanup_real_tree() {
  local depth=$1 prefix="bench_real"
  for ((i = 1; i < depth; i++)); do
    unset -f "${prefix}" 2>/dev/null || true
    prefix="${prefix}::x"
  done
  unset -f "${prefix}" 2>/dev/null || true
}

printf "### Real-world (\`:usage\` + \`:args\` at every level, depth 10)\n\n"
printf "| %-10s | %12s | %12s | %7s |\n" "Scenario" "Pure Bash" "Builtin" "Speedup"
printf "|------------|--------------|--------------|---------|"

depth=10

# Build args: --f1 v1 --f2 v2 x --f1 v1 --f2 v2 x ... -h
real_cmd=(bench_real)
for ((j = 1; j < depth; j++)); do
  real_cmd+=(--f1 "v1" --f2 "v2" x)
done
real_cmd+=(-h)

disable_builtins
generate_real_tree "${depth}"
raw_bash=$(measure "${real_cmd[@]}")
ms_bash=$(( raw_bash - fork_ms ))
(( ms_bash > 0 )) || ms_bash=1
cleanup_real_tree "${depth}"

enable_builtins
generate_real_tree "${depth}"
raw_builtin=$(measure "${real_cmd[@]}")
ms_builtin=$(( raw_builtin - fork_ms ))
(( ms_builtin > 0 )) || ms_builtin=1
cleanup_real_tree "${depth}"

printf "\n| %-10s | %9d ms | %9d ms | %5sx |" \
  "10 levels" "${ms_bash}" "${ms_builtin}" "$(calc_speedup "${ms_bash}" "${ms_builtin}")"

printf "\n\n"
