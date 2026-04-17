#!/usr/bin/env bats
# shellcheck disable=SC1091 disable=SC2154 disable=SC2317 disable=SC2329 disable=SC2034 disable=SC2030 disable=SC2031 disable=SC2314
# shellcheck shell=bats
#
# Tests for argsh::builtin, argsh::status, argsh::main dispatcher,
# and subcommand handlers (test/lint/minify/coverage/docs).

load ../test/helper
ARGSH_SOURCE=argsh
load_source

# Ensure ARGSH_BUILTIN is defined for tests (default: not loaded)
declare -gi ARGSH_BUILTIN="${ARGSH_BUILTIN:-0}"

# ---------------------------------------------------------------------------
# argsh::builtin (no args) — shows status
# ---------------------------------------------------------------------------
@test "argsh::builtin: no args shows status" {
  ARGSH_BUILTIN=0 argsh::builtin >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh builtin:" stdout
  contains "platform:" stdout
  contains "loaded:" stdout
  contains "Usage:" stdout
}

# ---------------------------------------------------------------------------
# argsh::builtin status — same output as no args
# ---------------------------------------------------------------------------
@test "argsh::builtin status: shows status" {
  ARGSH_BUILTIN=0 argsh::builtin status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh builtin:" stdout
  contains "platform:" stdout
  contains "loaded:" stdout
}

# ---------------------------------------------------------------------------
# argsh::builtin unknowncmd — returns error
# ---------------------------------------------------------------------------
@test "argsh::builtin: unknown subcommand returns error" {
  argsh::builtin unknowncmd >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 1
  is_empty stdout
  contains "unknown builtin subcommand: unknowncmd" stderr
  contains "Usage:" stderr
}

# ---------------------------------------------------------------------------
# argsh::builtins (plural alias) — delegates to singular
# ---------------------------------------------------------------------------
@test "argsh::builtins: plural alias delegates to argsh::builtin" {
  ARGSH_BUILTIN=0 argsh::builtins >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh builtin:" stdout
}

# ---------------------------------------------------------------------------
# argsh::status — shows version, builtin section, shell section, features
# ---------------------------------------------------------------------------
@test "argsh::status: shows all sections" {
  ARGSH_BUILTIN=0 argsh::status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "argsh " stdout
  contains "Builtin" stdout
  contains "status:" stdout
  contains "Shell:" stdout
  contains "bash:" stdout
  contains "Features:" stdout
}

# ---------------------------------------------------------------------------
# argsh::status with ARGSH_BUILTIN=1 shows "available"
# ---------------------------------------------------------------------------
@test "argsh::status: ARGSH_BUILTIN=1 shows available features" {
  ARGSH_BUILTIN=1 argsh::status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "loaded" stdout
  contains "available \\(builtin\\)" stdout
}

# ---------------------------------------------------------------------------
# argsh::status with ARGSH_BUILTIN=0 shows "requires builtin"
# ---------------------------------------------------------------------------
@test "argsh::status: ARGSH_BUILTIN=0 shows requires builtin" {
  ARGSH_BUILTIN=0 argsh::status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  is_empty stderr
  contains "not loaded" stdout
  contains "requires builtin" stdout
}

# ---------------------------------------------------------------------------
# argsh::main — registers all subcommands and shows them in help
# ---------------------------------------------------------------------------
@test "argsh::main --help lists all subcommands" {
  (argsh::main --help) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "minify" stdout
  contains "lint" stdout
  contains "test" stdout
  contains "coverage" stdout
  contains "docs" stdout
  contains "builtin" stdout
  contains "status" stdout
}

@test "argsh::main unknown command suggests closest match" {
  (argsh::main tests) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  # :usage suggests the closest known command
  contains "test" stderr
}

# ---------------------------------------------------------------------------
# argsh::shebang dispatch tests
# ---------------------------------------------------------------------------
@test "shebang: no args shows help" {
  argsh::shebang >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "minify" stdout
  contains "test" stdout
}

@test "shebang: --help shows help" {
  argsh::shebang --help >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "builtin" stdout
  contains "status" stdout
  contains "test" stdout
}

@test "shebang: -h shows help" {
  argsh::shebang -h >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "minify" stdout
}

@test "shebang: builtin command dispatches" {
  ARGSH_BUILTIN=0 argsh::shebang builtin >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "argsh builtin:" stdout
}

@test "shebang: status command dispatches" {
  ARGSH_BUILTIN=0 argsh::shebang status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "Shell:" stdout
  contains "Features:" stdout
}

# -----------------------------------------------------------------------------
# Discovery tests

@test "status: discovers bats files" {
  ARGSH_BUILTIN=0 argsh::shebang status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "\.bats file" stdout
}

@test "status: PATH_TESTS adds custom directory" {
  local _tmp
  _tmp="$(mktemp -d)"
  touch "${_tmp}/custom.bats"
  (
    PATH_TESTS="${_tmp}" ARGSH_BUILTIN=0 argsh::shebang status
  ) >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "custom.bats" stdout
}

@test "status: deduplicates overlapping dirs" {
  (
    # PATH_BASE/libraries is already in defaults, adding it via PATH_TESTS shouldn't double-count
    export PATH_TESTS="${PATH_BASE:-${BATS_TEST_DIRNAME}/..}/libraries"
    ARGSH_BUILTIN=0 argsh::status
  ) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "\.bats file" stdout
  # Count .bats occurrences — each file should appear only once
  local _count
  _count=$(command grep -c "\.bats$" "${stdout}" || echo 0)
  # libraries has 4 .bats files, shouldn't be doubled
  assert "${_count}" -le 6
}

@test "status: discovers coverage.json" {
  ARGSH_BUILTIN=0 argsh::shebang status >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "coverage.json" stdout
}

@test "shebang: --version shows version" {
  ARGSH_VERSION="test-ver" argsh::shebang --version >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "test-ver" stdout
}

# ---------------------------------------------------------------------------
# Subcommand handler tests — verify dispatch reaches the handler and that
# discover_files is available (regression: https://... was
# "argsh::discover_files: command not found" from docker-entrypoint.sh)
# ---------------------------------------------------------------------------

@test "argsh::test: no args discovers .bats files via PATH_TESTS" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  touch "${_tmp}/sample.bats"
  # Stub bats so the handler does not actually execute bats — just echo the args.
  bats() { echo "bats called with: $*"; }
  export -f bats
  # Stub binary::exists so argsh::test takes the local path (not docker forward).
  binary::exists() { [[ "${1}" == "bats" ]] || command -v "${1}" &>/dev/null; }

  # shellcheck disable=SC2119
  (PATH_TESTS="${_tmp}" argsh::test) >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "sample.bats" stdout
}

@test "argsh::test: errors cleanly when no files discovered" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  bats() { echo "bats called"; }
  export -f bats
  binary::exists() { [[ "${1}" == "bats" ]] || command -v "${1}" &>/dev/null; }
  # Stub discover_files to return nothing, simulating an empty project.
  argsh::discover_files() { :; }

  # shellcheck disable=SC2119
  (argsh::test) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "No test files found" stderr
}

# ---------------------------------------------------------------------------
# argsh::builtin::download — atomic install via temp file
#
# Regression: `argsh builtin update` segfaulted when overwriting the .so file
# already loaded into the current bash process. Fix downloads to a temp path
# and atomically `mv`s into place.
# ---------------------------------------------------------------------------

@test "builtin::download: writes to temp path then atomically moves" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  # Stub the network and verification — we want to assert on filesystem behavior.
  github::latest() { echo "v0.0.0-test"; }
  curl() {
    # Find -o argument and write a fake payload there. Verify it is NOT the
    # final destination path (regression: previous code wrote directly to dest).
    local _out=""
    while [[ $# -gt 0 ]]; do
      [[ "${1}" == "-o" ]] && { _out="${2}"; shift 2; continue; }
      shift
    done
    [[ -n "${_out}" ]] || return 1
    [[ "${_out}" != "${_tmp}/argsh.so" ]] || {
      echo "regression: curl wrote directly to destination, not temp" >&2
      return 1
    }
    echo "fake-so-content" > "${_out}"
  }
  enable() { :; }  # stub `enable -f` verification
  export -f github::latest curl enable

  PATH_BIN="${_tmp}" argsh::builtin::download 1 >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  # Final destination exists with the downloaded content
  assert -f "${_tmp}/argsh.so"
  assert "$(cat "${_tmp}/argsh.so")" = "fake-so-content"
  # No leftover temp files
  local _leftover
  _leftover=$(command ls "${_tmp}"/*.download.* 2>/dev/null || true)
  assert "${_leftover}" = ""

  rm -rf "${_tmp}"
}

@test "builtin::download: cleans up temp file on download failure" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  github::latest() { echo "v0.0.0-test"; }
  curl() { return 1; }  # simulate download failure
  export -f github::latest curl

  PATH_BIN="${_tmp}" argsh::builtin::download 1 >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "download failed" stderr
  # No leftover files at all
  assert ! -f "${_tmp}/argsh.so"
  local _leftover
  _leftover=$(command ls "${_tmp}"/*.download.* 2>/dev/null || true)
  assert "${_leftover}" = ""

  rm -rf "${_tmp}"
}

@test "builtin::download: cleans up temp file on verify failure" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  github::latest() { echo "v0.0.0-test"; }
  curl() {
    local _out=""
    while [[ $# -gt 0 ]]; do
      [[ "${1}" == "-o" ]] && { _out="${2}"; shift 2; continue; }
      shift
    done
    echo "bad" > "${_out}"
  }
  enable() { return 1; }  # simulate failed load at `enable -f`
  export -f github::latest curl enable

  PATH_BIN="${_tmp}" argsh::builtin::download 1 >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "failed to load as builtin" stderr
  # No final file written, no temp leftovers
  assert ! -f "${_tmp}/argsh.so"
  local _leftover
  _leftover=$(command ls "${_tmp}"/*.download.* 2>/dev/null || true)
  assert "${_leftover}" = ""

  rm -rf "${_tmp}"
}

@test "builtin::download: surfaces enable -f stderr on failure" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  github::latest() { echo "v0.0.0-test"; }
  curl() {
    local _out=""
    while [[ $# -gt 0 ]]; do
      [[ "${1}" == "-o" ]] && { _out="${2}"; shift 2; continue; }
      shift
    done
    echo "bad" > "${_out}"
  }
  # Stub `enable` itself — this is the real code path now, so loader
  # diagnostics must propagate end-to-end without being swallowed.
  enable() {
    echo "cannot open shared object: wrong ELF class" >&2
    return 1
  }
  export -f github::latest curl enable

  PATH_BIN="${_tmp}" argsh::builtin::download 1 >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "failed to load as builtin" stderr
  # The underlying loader diagnostic must reach the user, not get swallowed.
  contains "wrong ELF class" stderr

  rm -rf "${_tmp}"
}

@test "builtin::download: overwriting existing .so does not crash and replaces content" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  # Pre-existing .so with old content (simulates already-installed builtin).
  echo "old-content" > "${_tmp}/argsh.so"
  github::latest() { echo "v0.0.0-test"; }
  curl() {
    local _out=""
    while [[ $# -gt 0 ]]; do
      [[ "${1}" == "-o" ]] && { _out="${2}"; shift 2; continue; }
      shift
    done
    echo "new-content" > "${_out}"
  }
  enable() { :; }
  export -f github::latest curl enable

  PATH_BIN="${_tmp}" argsh::builtin::download 1 >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  assert "$(cat "${_tmp}/argsh.so")" = "new-content"

  rm -rf "${_tmp}"
}

@test "builtin::download: fresh install uses 0644 mode (not mktemp's 0600)" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  github::latest() { echo "v0.0.0-test"; }
  curl() {
    local _out=""
    while [[ $# -gt 0 ]]; do
      [[ "${1}" == "-o" ]] && { _out="${2}"; shift 2; continue; }
      shift
    done
    echo "new-content" > "${_out}"
  }
  enable() { :; }
  export -f github::latest curl enable

  PATH_BIN="${_tmp}" argsh::builtin::download 1 >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  # Must NOT be 0600 (mktemp default) — that would break shared install dirs.
  local _mode
  _mode="$(stat -c '%a' "${_tmp}/argsh.so")"
  assert "${_mode}" = "644"

  rm -rf "${_tmp}"
}

@test "builtin::download: preserves existing .so mode on update" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  # Pre-existing .so with a custom (non-default) mode set by the operator.
  echo "old-content" > "${_tmp}/argsh.so"
  chmod 0755 "${_tmp}/argsh.so"
  github::latest() { echo "v0.0.0-test"; }
  curl() {
    local _out=""
    while [[ $# -gt 0 ]]; do
      [[ "${1}" == "-o" ]] && { _out="${2}"; shift 2; continue; }
      shift
    done
    echo "new-content" > "${_out}"
  }
  enable() { :; }
  export -f github::latest curl enable

  PATH_BIN="${_tmp}" argsh::builtin::download 1 >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  # Mode must be preserved across updates so operator customizations stick.
  local _mode
  _mode="$(stat -c '%a' "${_tmp}/argsh.so")"
  assert "${_mode}" = "755"

  rm -rf "${_tmp}"
}

@test "argsh::lint: errors cleanly when no files discovered" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  shellcheck() { echo "shellcheck called"; }
  export -f shellcheck
  binary::exists() { [[ "${1}" == "shellcheck" ]] || command -v "${1}" &>/dev/null; }
  argsh::discover_files() { :; }
  argsh::discover_dirs() { _search_dirs=(); }

  # shellcheck disable=SC2119
  (argsh::lint) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "No files to lint" stderr
}

@test "argsh::lint: --only-argsh skips shellcheck" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  shellcheck() { echo "SHELLCHECK: $*"; }
  argsh-lint() { echo "ARGSH_LINT: $*"; }
  export -f shellcheck argsh-lint
  binary::exists() { case "${1}" in shellcheck|argsh-lint) return 0 ;; *) command -v "${1}" &>/dev/null ;; esac; }

  (argsh::lint --only-argsh "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  # Must NOT contain shellcheck stub output (only argsh-lint should run).
  # Use `command grep` — bats overrides `grep` to print failure diagnostics
  # on non-zero exit, which would be noise here (we expect no match).
  ! command grep -q "SHELLCHECK:" "${stdout}"
  # MUST contain argsh-lint stub output.
  contains "ARGSH_LINT:" stdout
}

@test "argsh::lint: --only-shellcheck skips argsh-lint" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  shellcheck() { echo "SHELLCHECK: $*"; }
  argsh-lint() { echo "ARGSH_LINT: $*"; }
  export -f shellcheck argsh-lint
  binary::exists() { case "${1}" in shellcheck|argsh-lint) return 0 ;; *) command -v "${1}" &>/dev/null ;; esac; }

  (argsh::lint --only-shellcheck "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "SHELLCHECK:" stdout
  # Use `command grep` for the negative case to silence bats' helper noise.
  ! command grep -q "ARGSH_LINT:" "${stdout}"
}

@test "argsh::lint: both --only-* flags together is a usage error" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  shellcheck() { :; }
  export -f shellcheck
  binary::exists() { [[ "${1}" == "shellcheck" ]] || command -v "${1}" &>/dev/null; }

  (argsh::lint --only-argsh --only-shellcheck foo) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 2
  contains "mutually exclusive" stderr
}

@test "argsh::lint: default runs both shellcheck and argsh-lint" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  shellcheck() { echo "SHELLCHECK: $*"; }
  argsh-lint() { echo "ARGSH_LINT: $*"; }
  export -f shellcheck argsh-lint
  binary::exists() { case "${1}" in shellcheck|argsh-lint) return 0 ;; *) command -v "${1}" &>/dev/null ;; esac; }

  (argsh::lint "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "SHELLCHECK:" stdout
  contains "ARGSH_LINT:" stdout
}

@test "argsh::lint: --only-argsh does not require shellcheck locally" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  argsh-lint() { echo "ARGSH_LINT: $*"; }
  export -f argsh-lint
  # Simulate shellcheck *missing* but argsh-lint available — docker-forward
  # must NOT be invoked because the user explicitly said --only-argsh.
  binary::exists() { [[ "${1}" == "argsh-lint" ]]; }
  argsh::_docker_forward() { echo "DOCKER_FORWARD: $*"; return 0; }

  (argsh::lint --only-argsh "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "ARGSH_LINT:" stdout
  # Must NOT have fallen back to Docker just because shellcheck is missing.
  ! command grep -q "DOCKER_FORWARD:" "${stdout}"
}

@test "argsh::lint: missing argsh-lint with --only-argsh errors" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  shellcheck() { :; }
  export -f shellcheck
  # Simulate shellcheck installed, argsh-lint not present.
  binary::exists() { [[ "${1}" == "shellcheck" ]]; }

  (argsh::lint --only-argsh "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -ne 0
  contains "argsh-lint binary not found" stderr
}

@test "argsh::lint: missing argsh-lint without flags silently skips it" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  shellcheck() { echo "SHELLCHECK: $*"; }
  export -f shellcheck
  binary::exists() { [[ "${1}" == "shellcheck" ]]; }

  (argsh::lint "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "SHELLCHECK:" stdout
}

@test "argsh::lint: shellcheck failure propagates non-zero exit" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  shellcheck() { return 1; }
  argsh-lint() { :; }
  export -f shellcheck argsh-lint
  binary::exists() { case "${1}" in shellcheck|argsh-lint) return 0 ;; *) command -v "${1}" &>/dev/null ;; esac; }

  (argsh::lint "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 1
}

@test "argsh::lint: argsh-lint failure propagates non-zero exit" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  echo '#!/usr/bin/env argsh' >"${_tmp}/a.sh"

  shellcheck() { :; }
  argsh-lint() { return 1; }
  export -f shellcheck argsh-lint
  binary::exists() { case "${1}" in shellcheck|argsh-lint) return 0 ;; *) command -v "${1}" &>/dev/null ;; esac; }

  (argsh::lint "${_tmp}/a.sh") >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 1
}

@test "argsh::lint: detects extensionless scripts with #!/usr/bin/env sh shebang" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  local _tmp
  _tmp="$(mktemp -d)"
  # Extensionless script with env-style sh shebang — previously missed by
  # the `*"/sh"*` substring pattern.
  cat >"${_tmp}/envsh-script" <<'EOF'
#!/usr/bin/env sh
echo hi
EOF
  chmod +x "${_tmp}/envsh-script"

  # Stub shellcheck so we just echo the files passed to it.
  shellcheck() { echo "LINT: $*"; }
  export -f shellcheck
  binary::exists() { [[ "${1}" == "shellcheck" ]] || command -v "${1}" &>/dev/null; }
  argsh::discover_files() { :; }
  argsh::discover_dirs() { _search_dirs=("${_tmp}"); }

  # shellcheck disable=SC2119
  (argsh::lint) >"${stdout}" 2>"${stderr}" || status=$?
  rm -rf "${_tmp}"

  assert "${status}" -eq 0
  contains "envsh-script" stdout
}

@test "argsh::_docker_forward: errors without docker" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  binary::exists() { [[ "${1}" != "docker" ]]; }

  argsh::_docker_forward test >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "Docker" stderr
}

@test "argsh::_docker_forward: forwards exported ARGSH_ENV_* vars" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  # Stub docker to capture the args it receives.
  docker() { echo "DOCKER_ARGS: $*"; }
  export -f docker
  binary::exists() { true; }
  docker::user() { echo ""; }
  export ARGSH_ENV_MY_TOKEN=secret123
  export ARGSH_ENV_DEBUG=1

  argsh::_docker_forward test >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  # Values are passed via process env (not argv) for security,
  # so docker args contain only the name: -e MY_TOKEN, -e DEBUG.
  contains "-e MY_TOKEN" stdout
  contains "-e DEBUG" stdout
  # Verify the value isn't in the argv (security: no secret leaks in ps).
  ! grep -q "secret123" "${stdout}"
  unset ARGSH_ENV_MY_TOKEN ARGSH_ENV_DEBUG MY_TOKEN DEBUG
}

@test "argsh::_docker_forward: skips invalid ARGSH_ENV_ names" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  docker() { echo "DOCKER_ARGS: $*"; }
  export -f docker
  binary::exists() { true; }
  docker::user() { echo ""; }
  # ARGSH_ENV_ alone (empty name) and ARGSH_ENV_1BAD (starts with digit)
  export ARGSH_ENV_=empty
  export ARGSH_ENV_1BAD=nope
  export ARGSH_ENV_GOOD=yes

  argsh::_docker_forward test >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "-e GOOD" stdout
  # Must not contain the invalid ones
  ! grep -q "1BAD" "${stdout}"
  ! grep -q -- "-e =" "${stdout}"
  unset ARGSH_ENV_ ARGSH_ENV_1BAD ARGSH_ENV_GOOD GOOD
}

@test "argsh::main: dispatches test subcommand to argsh::test" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  # Override argsh::test to prove dispatch reached the handler
  # shellcheck disable=SC2120
  argsh::test() { echo "dispatched-to-test: $*"; }

  (argsh::main test foo bar) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "dispatched-to-test: foo bar" stdout
}

@test "argsh::main: dispatches lint subcommand to argsh::lint" {
  if [[ -n "${BATS_LOAD:-}" ]]; then set +u; skip "function stubs do not survive minified argsh"; fi
  # shellcheck disable=SC2120
  argsh::lint() { echo "dispatched-to-lint: $*"; }

  (argsh::main lint a b) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  contains "dispatched-to-lint: a b" stdout
}

@test "shebang: unknown command dispatches via argsh::main and suggests" {
  (argsh::shebang tests) >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "test" stderr
}

@test "shebang: path-like missing file errors with 'file not found'" {
  # Mistyped or missing script path should fail with a clear error,
  # NOT "Invalid command: ./missing.sh. Did you mean ...?"
  argsh::shebang ./does-not-exist.sh >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "file not found" stderr
}

@test "shebang: slashed path without extension also treated as file" {
  argsh::shebang /tmp/no/such/script >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -ne 0
  contains "file not found" stderr
}

@test "shebang: builtins alias still works" {
  ARGSH_BUILTIN=0 argsh::shebang builtins >"${stdout}" 2>"${stderr}" || status=$?

  assert "${status}" -eq 0
  # Both the pure-bash shim and the native .so version print a line
  # starting with "argsh builtin" (with or without trailing 's').
  contains "argsh builtin" stdout
}
