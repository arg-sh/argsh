# argsh Code Review — 29 Findings

Detailed analysis of issues found across the Rust builtins, Rust minifier,
Bash libraries, and DevOps/CI pipelines.

---

## Rust Builtin (1–7)

### 1. Validate `result_var` in `exec_capture` and `name` in `write_array`

| | |
|---|---|
| **Severity** | High |
| **File** | `builtin/src/shell.rs` |
| **Lines** | 93–103 (`write_array`), 178–201 (`exec_capture`) |

**Current code — `write_array`:**

```rust
pub fn write_array(name: &str, values: &[String]) {
    if let Ok(cname) = CString::new(name) {
        unsafe {
            unbind_variable(cname.as_ptr());
            make_new_array_variable(cname.as_ptr());
        }
    }
    for (i, val) in values.iter().enumerate() {
        let _ = bash_builtins::variables::array_set(name, i, val);
    }
}
```

**Current code — `exec_capture`:**

```rust
pub fn exec_capture(cmd: &str, result_var: &str) -> Option<String> {
    let full_cmd = format!("{}=\"$({})\"", result_var, cmd);
    // ...
}
```

**Problem:**
Neither function validates its name parameter against `is_valid_bash_name()`
before use. `exec_capture` formats `result_var` directly into a bash command
string — a malicious or corrupted name like `"; rm -rf /; x` becomes part of
the executed command. `write_array` passes an unchecked name straight to
`unbind_variable` / `make_new_array_variable`.

While these functions are only called internally (not from user input), adding
validation follows defense-in-depth and guards against future refactors that
might widen the call surface.

**Fix:**

```rust
pub fn write_array(name: &str, values: &[String]) {
    if !is_valid_bash_name(name) {
        return;
    }
    // ... existing body
}

pub fn exec_capture(cmd: &str, result_var: &str) -> Option<String> {
    if !is_valid_bash_name(result_var) {
        return None;
    }
    // ... existing body
}
```

---

### 2. `std::process::exit()` terminates the entire bash process

| | |
|---|---|
| **Severity** | Critical |
| **Files** | `builtin/src/args.rs`, `builtin/src/usage.rs` |
| **Lines** | args.rs: 64, 72, 360, 366 — usage.rs: 64, 73, 82, 119, 154, 192, 276, 290, 354, 475, 481 |

**Current code (args.rs:62–65):**

```rust
if !args_arr.len().is_multiple_of(2) {
    shell::write_stderr(":args error [???] ➜ args must be an associative array");
    std::process::exit(2);
}
```

**Current code (args.rs:354–361):**

```rust
fn error_usage(field: &str, msg: &str) {
    let field_display = field.split(['|', ':']).next().unwrap_or(field);
    let script = shell::get_script_name();
    eprint!("[ {} ] invalid usage\n\u{279c} {}\n\n", field_display, msg);
    eprintln!("Use \"{} -h\" for more information", script);
    std::process::exit(2);
}
```

**Problem:**
Bash builtins run inside the bash process. Calling `std::process::exit()`
terminates **the entire shell**, not just the builtin invocation. A user calling
`:args` with wrong arguments kills their interactive session or any parent
script that `source`'d the library. Builtins must return `c_int` exit codes
(0–255), never call `exit()`.

There are ~15 instances across both files.

**Fix:**
Change helper functions to return exit codes, propagate up via `return`:

```rust
fn error_usage(field: &str, msg: &str) -> c_int {
    let field_display = field.split(['|', ':']).next().unwrap_or(field);
    let script = shell::get_script_name();
    eprint!("[ {} ] invalid usage\n\u{279c} {}\n\n", field_display, msg);
    eprintln!("Use \"{} -h\" for more information", script);
    2
}
```

Then at call sites:

```rust
if args_arr.len() % 2 != 0 {
    shell::write_stderr(":args error [???] ➜ args must be an associative array");
    return 2;
}
```

---

### 3. Deduplicate `parse_flag_at`, `check_required_flags`, `error_usage`, `error_args`

| | |
|---|---|
| **Severity** | Medium |
| **Files** | `builtin/src/args.rs`, `builtin/src/usage.rs` |
| **Lines** | args.rs: 156–289, 354–367 — usage.rs: 209–357, 469–482 |

**Problem:**
Four functions are copy-pasted between `args.rs` and `usage.rs` with identical
(or near-identical) logic:

| Function | args.rs lines | usage.rs lines |
|---|---|---|
| `parse_flag_at` | 156–263 | 209–325 |
| `check_required_flags` | 265–289 | 335–357 |
| `error_usage` | 354–361 | 469–475 |
| `error_args` | 363–367 | 478–481 |

Bug fixes must be applied in two places. Any divergence becomes a latent bug.

**Fix:**
Extract into a shared module:

```rust
// src/shared.rs
pub fn parse_flag_at(...) -> Option<bool> { ... }
pub fn check_required_flags(...) { ... }
pub fn error_usage(...) -> c_int { ... }
pub fn error_args(...) -> c_int { ... }
```

Import in both files:

```rust
use crate::shared::{parse_flag_at, check_required_flags, error_usage, error_args};
```

---

### 4. `is_multiple_of(2)` requires Rust 1.87+

| | |
|---|---|
| **Severity** | Medium |
| **Files** | `builtin/src/args.rs:62`, `builtin/src/usage.rs:62` |

**Current code:**

```rust
if !args_arr.len().is_multiple_of(2) {
```

**Problem:**
`usize::is_multiple_of()` was stabilized in Rust 1.87.0 (May 2025). Users on
older toolchains (common in enterprise/distro Rust) cannot compile the crate.
The modulo operator works on all versions.

**Fix:**

```rust
if args_arr.len() % 2 != 0 {
```

---

### 5. `get_assoc_keys` splits on whitespace — keys with spaces break

| | |
|---|---|
| **Severity** | Medium |
| **File** | `builtin/src/shell.rs:337–353` |

**Current code:**

```rust
pub fn get_assoc_keys(array_name: &str) -> Vec<String> {
    // ...
    let cmd = format!("{}=\"${{!{}[@]}}\"", tmp, array_name);
    // ...
    let val = get_scalar(tmp).unwrap_or_default();
    // ...
    val.split_whitespace().map(|s| s.to_string()).collect()
}
```

**Problem:**
Bash's `${!array[@]}` returns keys space-separated. `split_whitespace()`
cannot distinguish between a space *inside* a key and a space *between* keys.

Example: `assoc["my key"]=1 assoc["other"]=2` → bash returns `my key other` →
`split_whitespace()` produces `["my", "key", "other"]` (3 items, expected 2).

**Fix:**
Use bash `printf '%s\n'` with null-delimited output, or iterate with indexed
slicing. Alternatively, document this as a known limitation — argsh's arg
definitions don't use spaces in keys, so practical impact is low.

```rust
// Alternative: use null-delimited iteration
let cmd = format!(
    "for __k in \"${{!{}[@]}}\"; do printf '%s\\0' \"$__k\"; done",
    array_name
);
// ... parse with split('\0')
```

---

### 6. `is_valid_bash_name` allows colons — valid for functions but not variables

| | |
|---|---|
| **Severity** | Medium |
| **File** | `builtin/src/shell.rs:276–284` |

**Current code:**

```rust
fn is_valid_bash_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}
```

**Problem:**
Bash **variables** are `[a-zA-Z_][a-zA-Z0-9_]*` — no colons. Bash
**functions** allow colons (e.g., `is::array`). This single validator is used
for both contexts. When called from `write_array()`, `array_append()`,
`get_assoc_keys()`, etc., it accepts names like `my:var` which are invalid as
variables.

**Fix:**
Split into two validators:

```rust
fn is_valid_bash_variable(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

fn is_valid_bash_name(name: &str) -> bool {
    // For function names — includes colons
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}
```

Use `is_valid_bash_variable` in variable-context functions (`write_array`,
`set_scalar`, `get_assoc_keys`, etc.) and `is_valid_bash_name` for function
contexts (`create_function_alias`).

---

### 7. `test/helper.bash:93–102` — `is::uninitialized` inverted loop logic

| | |
|---|---|
| **Severity** | Medium |
| **File** | `test/helper.bash:93–102` |

**Current code:**

```bash
is::uninitialized() {
  local var
  for var in "${@}"; do
    if is::array "${var}"; then
      [[ $(declare -p "${var}") == "declare -a ${var}" ]]
    else
      [[ ! ${!var+x} ]]
    fi
  done
}
```

**Problem:**
The function iterates over all arguments but only the **last** iteration's
return code matters. If called with multiple arguments, early failures are
silently ignored. For example:

```bash
is::uninitialized initialized_var uninitialized_var  # returns 0 (wrong)
is::uninitialized uninitialized_var initialized_var  # returns 1 (correct)
```

The result depends on argument order, not whether all variables are
uninitialized.

**Fix:**

```bash
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
```

> **Note:** The production `is::uninitialized` in `libraries/is.sh:40–47` only
> accepts a single argument (`local var="${1}"`), so it's not affected. This
> issue is specific to the test helper variant which takes `"${@}"`.

---

## Rust Minifier (8–14)

### 8. ~~Heredoc content gets obfuscated inside quoted heredocs (`<<'EOF'`)~~ — NOT AN ISSUE

| | |
|---|---|
| **Severity** | ~~High~~ N/A |
| **File** | `minifier/src/strip.rs:30–31, 60–74` |

**Status: NOT AN ISSUE.** While the regex discards quote info from the
delimiter, the strip phase preserves **all** heredoc content verbatim (lines
61–66: once in heredoc mode, every line is pushed as-is until the delimiter).
The obfuscate phase runs on the output of strip, but heredoc lines are already
preserved intact. Revalidation confirmed this works correctly.

---

### 9. Heredoc detection is not quote-aware

| | |
|---|---|
| **Severity** | High |
| **File** | `minifier/src/strip.rs:30–31, 69–70` |

**Current code:**

```rust
if let Some(cap) = RE_HEREDOC.captures(line) {
    heredoc_delim = Some(cap[1].to_string());
}
```

**Problem:**
The regex scans the entire line. A string like `echo "not a <<EOF"` contains
`<<EOF` inside double quotes — it's **not** a heredoc. The current code doesn't
check whether `<<` is inside quotes, so it falsely enters heredoc mode and
treats subsequent lines as heredoc content until it finds `EOF`.

This causes those lines to be preserved verbatim (not stripped), and the
parser's state becomes corrupted for the rest of the file.

**Fix:**
Check that `<<` appears outside quotes before matching:

```rust
fn heredoc_outside_quotes(line: &str) -> Option<(String, bool)> {
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = line.chars().collect();
    for i in 0..chars.len().saturating_sub(1) {
        match chars[i] {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '<' if !in_single && !in_double && chars[i + 1] == '<' => {
                // Found << outside quotes — apply regex from this position
                if let Some(cap) = RE_HEREDOC.captures(&line[i..]) {
                    let is_quoted = !cap[1].is_empty();
                    return Some((cap[2].to_string(), is_quoted));
                }
            }
            _ => {}
        }
    }
    None
}
```

---

### 10. ~~`in_open_quote` continuation tracking may terminate prematurely~~ — NOT AN ISSUE

| | |
|---|---|
| **Severity** | ~~Medium~~ N/A |
| **File** | `minifier/src/bundle.rs` (quote continuation logic) |

**Status: NOT AN ISSUE.** The current implementation uses
`QuoteTracker::line_has_open_quote()` which returns separate `(sq, dq)`
booleans. The continuation check at line 182 (`if !sq && !dq`) correctly
waits until both quote types are closed. Bash syntax requires matching quotes
per line, so the even-count edge case (`''`) is handled correctly — two
consecutive single quotes in bash mean "end quote + start quote", which the
odd-count tracker correctly interprets.

---

### 11. Harden `resolve_path()` against path traversal

| | |
|---|---|
| **Severity** | High |
| **File** | `minifier/src/bundle.rs:73–90` |

**Current code:**

```rust
fn resolve_path(target: &str, current_dir: &Path, config: &BundleConfig) -> Option<PathBuf> {
    let stripped = strip_import_prefix(target);
    let extensions = ["", ".sh", ".bash"];
    let dirs = std::iter::once(current_dir.to_path_buf())
        .chain(config.search_paths.iter().cloned());

    for dir in dirs {
        for ext in &extensions {
            let candidate = dir.join(format!("{stripped}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
```

**Problem:**
`Path::join()` follows `..` components. An `import ../../../etc/passwd` would
resolve to `/etc/passwd` and bundle its contents into the output script. While
the minifier is typically run on trusted input, hardening against path traversal
prevents accidental or malicious inclusion of files outside the project.

**Fix:**

```rust
fn resolve_path(target: &str, current_dir: &Path, config: &BundleConfig) -> Option<PathBuf> {
    let stripped = strip_import_prefix(target);
    if stripped.contains("..") || stripped.starts_with('/') {
        return None;
    }
    // ... rest unchanged
}
```

Or use `canonicalize()` and verify the result starts with an allowed directory.

---

### 12. `fix_keyword_semicolons` compiles regex on every call

| | |
|---|---|
| **Severity** | Low |
| **File** | `minifier/src/join.rs:234–238` |

**Current code:**

```rust
fn fix_keyword_semicolons(input: &str) -> String {
    let re = Regex::new(r"\b(then|do|else);").unwrap();
    re.replace_all(input, "$1 ").to_string()
}
```

**Problem:**
A new `Regex` is compiled on every call. All other regexes in the minifier use
`LazyLock<Regex>` statics (e.g., `RE_HEREDOC`, `RE_MIDLINE_SHEBANG`). This one
is inconsistent and unnecessarily slower.

**Fix:**

```rust
static RE_KEYWORD_SEMI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(then|do|else);").unwrap());

fn fix_keyword_semicolons(input: &str) -> String {
    RE_KEYWORD_SEMI.replace_all(input, "$1 ").to_string()
}
```

---

### 13. Naive semicolon splitting in `discover.rs` doesn't respect quotes

| | |
|---|---|
| **Severity** | High |
| **File** | `minifier/src/discover.rs:78–85` |

**Current code:**

```rust
for segment in line.split(';') {
    let segment = segment.trim();
    if segment.is_empty() {
        continue;
    }
    discover_from_segment(segment, &mut vars);
}
```

**Problem:**
`line.split(';')` splits on **all** semicolons, including those inside quoted
strings. A line like `local msg="hello; world"; x=1` splits into:

1. `local msg="hello`
2. ` world"`
3. ` x=1`

Segment 1 is malformed and may cause false variable discoveries. Segment 2 is
garbage.

**Fix:**
Split only on semicolons outside quotes:

```rust
fn split_outside_quotes(line: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let (mut sq, mut dq) = (false, false);
    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !dq => sq = !sq,
            '"' if !sq => dq = !dq,
            ';' if !sq && !dq => {
                segments.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(&line[start..]);
    segments
}
```

---

### 14. Escape sequence handling doesn't handle `\\"` (escaped backslash + quote)

| | |
|---|---|
| **Severity** | High |
| **Files** | `minifier/src/quote.rs:12–30`, `minifier/src/flatten.rs:19–31` |

**Current code (quote.rs):**

```rust
for (i, &ch) in chars.iter().enumerate() {
    let prev = if i > 0 { chars[i - 1] } else { '\0' };
    match ch {
        '\'' if !in_double && prev != '\\' => {
            in_single = !in_single;
        }
        '"' if !in_single && prev != '\\' => {
            in_double = !in_double;
        }
        _ => {}
    }
}
```

**Same pattern in flatten.rs:27–31.**

**Problem:**
Checking `prev != '\\'` only handles a single backslash. For `\\\\"`, the
sequence is: `\\` (escaped backslash = literal `\`) followed by `"` (real
closing quote). But the code sees `prev = '\\' ` and treats `"` as escaped,
keeping the quote state open incorrectly.

Example: `echo "test\\\\"` should be a closed string (`test\`), but the parser
thinks the quote is still open.

**Fix:**
Count consecutive preceding backslashes. Odd count = escaped, even count = not:

```rust
let mut backslashes = 0;
let mut j = i;
while j > 0 && chars[j - 1] == '\\' {
    backslashes += 1;
    j -= 1;
}
let is_escaped = backslashes % 2 == 1;

match ch {
    '\'' if !in_double && !is_escaped => in_single = !in_single,
    '"' if !in_single && !is_escaped => in_double = !in_double,
    _ => {}
}
```

Apply to both `quote.rs` and `flatten.rs`.

---

## Bash Libraries (15–22)

### 15. `is::array` calls `declare -p` twice

| | |
|---|---|
| **Severity** | Low |
| **File** | `libraries/is.sh:27–29` |

**Current code:**

```bash
is::array() {
  declare -p "${1}" &>/dev/null && [[ $(declare -p "${1}") == "declare -a"* ]]
}
```

**Problem:**
`declare -p` is called twice for the same variable — once to check existence
(left side of `&&`), once to check the type (the `[[` test). Each call forks a
subshell for the `$()` capture.

**Fix:**

```bash
is::array() {
  local _decl
  _decl="$(declare -p "${1}" 2>/dev/null)" || return 1
  [[ "${_decl}" == "declare -a"* ]]
}
```

---

### 16. `is::uninitialized` may break on bash 5.x for empty arrays

| | |
|---|---|
| **Severity** | Medium |
| **File** | `libraries/is.sh:40–47` |

**Current code:**

```bash
is::uninitialized() {
  local var="${1}"
  if is::array "${var}"; then
    [[ $(declare -p "${var}") == "declare -a ${var}" ]]
  else
    [[ ! ${!var+x} ]]
  fi
}
```

**Problem:**
On **Bash 5.0+**, `declare -p` for an empty declared array outputs
`declare -a var=()` (with `=()` suffix). On **Bash 4.x**, it outputs
`declare -a var` (no suffix). The exact-match check on line 43 fails on
Bash 5.x because `"declare -a var" != "declare -a var=()"`.

Result: Empty arrays are incorrectly reported as **initialized** on Bash 5.x.

**Fix:**

```bash
is::uninitialized() {
  local var="${1}"
  if is::array "${var}"; then
    [[ $(declare -p "${var}") =~ ^declare\ -a\ ${var}(=\(\))?$ ]]
  else
    [[ ! ${!var+x} ]]
  fi
}
```

Or check the array length directly:

```bash
    local -n _ref="${var}"
    (( ${#_ref[@]} == 0 ))
```

---

### 17. `args.sh:155` — unquoted command substitution in `for` alias loop

| | |
|---|---|
| **Severity** | Medium |
| **File** | `libraries/args.sh:155` |

**Current code:**

```bash
for alias in $(echo "${usage[i]/:*}" | tr '|' "\n"); do
```

**Problem:**
The unquoted `$(...)` is subject to word splitting on `IFS` and pathname
expansion (globbing). If an alias value contains whitespace or glob characters
(`*`, `?`, `[`), it would be split or expanded incorrectly.

Additionally, `echo | tr` forks two subprocesses when bash can do this natively.

**Fix:**

```bash
IFS='|' read -ra _aliases <<< "${usage[i]/:*}"
for alias in "${_aliases[@]}"; do
```

---

### 18. `.bin/argsh:39` — `lint::vale` references undeclared variable `vale`

| | |
|---|---|
| **Severity** | Medium |
| **File** | `.bin/argsh:37–45` |

**Current code:**

```bash
lint::vale() {
  :args "Run vale for the documentation" "${@}"
  local alert_level="${vale?"need to specify alert level"}"
  binary::exists docker || exit 1
  docker run --rm $(docker::user) jdkato/vale:latest sh -c "
    cd /workspace/www/vale
    ./run-vale.sh docs content \"${alert_level}\"
  "
}
```

**Problem:**
Line 39 uses `${vale?...}` — this references a variable named `vale` which is
never declared. The `:args` call on line 38 should populate parsed arguments
into local variables, but no `local -a args=(...)` definition exists to declare
`vale` as a parameter.

The function will always fail with the error message unless `vale` happens to
exist in the outer scope.

**Fix:**
Add the proper argument definition:

```bash
lint::vale() {
  local alert_level
  local -a args=(
    'alert_level' "Alert level for vale (error, warning, suggestion)"
  )
  :args "Run vale for the documentation" "${@}"
  binary::exists docker || exit 1
  # ... rest unchanged, using ${alert_level}
}
```

---

### 19. `binary.sh:35` — unused `binary` variable

| | |
|---|---|
| **Severity** | Low |
| **File** | `libraries/binary.sh:33–47` |

**Current code:**

```bash
binary::github() {
  local path="${1}"
  local -r binary="$(basename "${path}")"
  local repo="${2}"
  # ...
}
```

**Problem:**
`binary` is declared with `local -r` (readonly) on line 35 and assigned the
basename of `path`, but it is never referenced anywhere in the function body.
Dead code that adds confusion.

**Fix:**
Remove the unused declaration:

```bash
binary::github() {
  local path="${1}"
  local repo="${2}"
```

---

### 20. `docker::user` temp files never cleaned up

| | |
|---|---|
| **Severity** | Medium |
| **File** | `libraries/docker.sh:22–47` |

**Current code:**

```bash
docker::user() {
  # ...
  local _passwd _group
  _passwd="$(mktemp /tmp/docker_passwd.XXXXXX)"
  _group="$(mktemp /tmp/docker_group.XXXXXX)"
  echo "${user}:x:${uid}:${gid}::${home}:${shell}" > "${_passwd}"
  echo "${user}:x:${gid}:" > "${_group}"
  echo "-v ${_passwd}:/etc/passwd -v ${_group}:/etc/group"
  # ...
}
```

**Problem:**
Two temp files are created on every call (lines 38–39) but never cleaned up.
The function outputs `-v` flags that mount these files into Docker — so they
must exist when `docker run` executes later. But no cleanup happens after Docker
finishes. Over time, `/tmp` accumulates orphaned files containing user/group
info.

**Fix:**
This is a design issue. The caller must clean up. Document the requirement
and provide a cleanup wrapper:

```bash
# In calling code:
local _docker_flags
_docker_flags="$(docker::user)"
trap 'rm -f /tmp/docker_passwd.* /tmp/docker_group.*' EXIT
docker run ${_docker_flags} image cmd
```

Or use a process substitution approach that avoids temp files entirely.

---

### 21. `error.sh:18` — `for i in $(seq ...)` could use arithmetic for loop

| | |
|---|---|
| **Severity** | Low |
| **File** | `libraries/error.sh:14–24` |

**Current code:**

```bash
error::stacktrace() {
  local -r code="${1:-${?}}"
  if (( code )); then
    echo -e "\n\033[38;5;196m■■ Stacktrace(${code}): \e[1m${BASH_COMMAND}\e[22m"
    for i in $(seq 1 $((${#FUNCNAME[@]} - 2))); do
      echo -e "${i}. ${BASH_SOURCE[i]}:${BASH_LINENO[i-1]} ➜ ${FUNCNAME[i]}()"
    done
```

**Problem:**
`seq` is an external process. Bash has built-in arithmetic `for` loops that
require no fork/exec and are more idiomatic.

**Fix:**

```bash
    for (( i = 1; i <= ${#FUNCNAME[@]} - 2; i++ )); do
```

---

### 22. `args.sh:155,639` — `echo | tr` pipeline could use bash parameter expansion

| | |
|---|---|
| **Severity** | Low |
| **Files** | `libraries/args.sh:155`, `libraries/args.sh:639` |

**Current code (line 155):**

```bash
for alias in $(echo "${usage[i]/:*}" | tr '|' "\n"); do
```

**Current code (line 639):**

```bash
mapfile -t flags < <(echo "${field/[:]*}" | tr '|' '\n')
```

**Problem:**
Both lines pipe through `tr` to replace `|` with newlines. Bash parameter
expansion `${var//|/$'\n'}` does this without forking an external process.

**Fix for line 155** (combines with issue 17):

```bash
IFS='|' read -ra _aliases <<< "${usage[i]/:*}"
for alias in "${_aliases[@]}"; do
```

**Fix for line 639:**

```bash
IFS='|' read -ra flags <<< "${field/[:]*}"
```

---

## DevOps / CI (23–29)

### 23. Add concurrency group to `argsh.yaml`

| | |
|---|---|
| **Severity** | Medium |
| **File** | `.github/workflows/argsh.yaml:1–21` |

**Problem:**
The workflow has no `concurrency` block. When multiple pushes to the same PR
happen quickly, all in-progress runs continue wasting CI minutes.
`docs.yaml` already has a concurrency group (lines 11–13), but `argsh.yaml`
does not.

**Fix:**
Add after line 16 (before `defaults:`):

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.ref != 'refs/heads/master' }}
```

---

### 24. Pin third-party actions to SHA digests

| | |
|---|---|
| **Severity** | High |
| **Files** | `.github/workflows/argsh.yaml`, `.github/workflows/docs.yaml` |

**Unpinned actions found:**

| File | Line | Action | Current |
|---|---|---|---|
| argsh.yaml | 29 | `HatsuneMiku3939/direnv-action` | `@v1` |
| argsh.yaml | 43, 76, 124, 242, 387 | `dtolnay/rust-toolchain` | `@stable` |
| argsh.yaml | 95, 143 | `bats-core/bats-action` | `@2.0.0` |
| argsh.yaml | 472 | `docker/metadata-action` | `@v5` |
| argsh.yaml | 480 | `docker/setup-buildx-action` | `@v3` |
| argsh.yaml | 482 | `docker/login-action` | `@v3` |
| argsh.yaml | 488 | `docker/build-push-action` | `@v5` |
| docs.yaml | 20, 45, 70, 86 | `actions/checkout` | `@v3` |
| docs.yaml | 25 | `actions/setup-node` | `@v3` |
| docs.yaml | 55 | `errata-ai/vale-action` | `@reviewdog` |

**Problem:**
Tag references are mutable. A compromised upstream repo can retag `v1` to a
malicious commit. SHA digests are immutable and prevent supply-chain attacks.

**Fix:**
For each action, resolve the SHA:

```bash
gh api repos/HatsuneMiku3939/direnv-action/commits/v1 --jq '.sha'
```

Then pin:

```yaml
- uses: HatsuneMiku3939/direnv-action@<full-sha>  # v1
```

---

### 25. Add top-level `permissions: read-all`

| | |
|---|---|
| **Severity** | High |
| **Files** | `.github/workflows/argsh.yaml`, `.github/workflows/docs.yaml` |

**Problem:**
Only the `docker` job (line 457) and `release` job (line 499) declare
permissions. All other jobs — `test`, `minifier-test`, `builtin-test`,
`coverage`, `minifier-coverage`, `minify`, `minify-so` — inherit the default
GitHub token permissions, which include write access to the repository.

If a PR-triggered job is compromised (e.g., via a malicious action), it has
write permissions it doesn't need.

**Fix:**
Add at the workflow level (after `defaults:`):

```yaml
permissions: read-all
```

Then selectively override in jobs that need write:

```yaml
  docker:
    permissions:
      contents: read
      packages: write
```

---

### 26. Pin Docker base images by digest

| | |
|---|---|
| **Severity** | High |
| **File** | `Dockerfile:4, 10, 32, 35` |

**Unpinned images:**

| Line | Image | Tag |
|---|---|---|
| 4 | `rust:1-slim` | Rolling `1.x` |
| 10 | `kcov/kcov` | `latest` (implicit) |
| 32 | `koalaman/shellcheck:stable` | `stable` (rolling) |
| 35 | `ghcr.io/jqlang/jq:latest` | `latest` (explicit) |

**Problem:**
Without digest pins, builds are non-reproducible. A new Rust 1.x release,
shellcheck update, or jq version can silently change behavior or break builds.

**Fix:**

```dockerfile
FROM rust:1.83-slim@sha256:<digest> AS minifier-build
FROM kcov/kcov@sha256:<digest>
COPY --from=koalaman/shellcheck:v0.10.0@sha256:<digest> /bin/shellcheck ...
COPY --from=ghcr.io/jqlang/jq:1.7.1@sha256:<digest> /jq ...
```

Resolve digests with:

```bash
docker pull rust:1-slim && docker inspect --format='{{index .RepoDigests 0}}' rust:1-slim
```

---

### 27. Add `restore-keys` to cargo cache steps

| | |
|---|---|
| **Severity** | Medium |
| **File** | `.github/workflows/argsh.yaml` |
| **Lines** | 46–52, 79–85, 128–133, 246–251, 389–394 |

**Current code (line 46–52):**

```yaml
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            minifier/target
            ~/.cargo/registry
          key: ${{ runner.os }}-cargo-minifier-${{ hashFiles('minifier/Cargo.lock') }}
```

**Problem:**
Five cargo cache steps lack `restore-keys`. When `Cargo.lock` changes (any
dependency update), the cache key changes completely — no partial hit is
possible. The build re-downloads all crates from scratch.

The project's own `cache-deps` action correctly uses `restore-keys` for yarn,
but the cargo caches don't follow the same pattern.

**Fix:**
Add `restore-keys` to all five cache steps:

```yaml
          key: ${{ runner.os }}-cargo-minifier-${{ hashFiles('minifier/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-minifier-
            ${{ runner.os }}-cargo-
```

---

### 28. Upgrade `docs.yaml` actions from `@v3` to `@v4`

| | |
|---|---|
| **Severity** | Medium |
| **File** | `.github/workflows/docs.yaml` |
| **Lines** | 20, 25, 45, 70, 86 |

**Outdated actions:**

| Line | Action | Current | Target |
|---|---|---|---|
| 20 | `actions/checkout` | `@v3` | `@v4` |
| 25 | `actions/setup-node` | `@v3` | `@v4` |
| 45 | `actions/checkout` | `@v3` | `@v4` |
| 70 | `actions/checkout` | `@v3` | `@v4` |
| 86 | `actions/checkout` | `@v3` | `@v4` |

**Problem:**
`argsh.yaml` already uses `@v4` for `actions/checkout` (line 27, 41, 74, etc.).
`docs.yaml` is inconsistent and misses bug fixes, performance improvements,
and security patches in v4.

**Fix:**
Replace all `@v3` with `@v4` (or better, SHA-pinned as in issue 24).

---

### 29. Align artifact retention periods

| | |
|---|---|
| **Severity** | Low |
| **File** | `.github/workflows/argsh.yaml` |

**Current retention:**

| Line | Artifact | Retention | Type |
|---|---|---|---|
| 62–68 | `minifier-bin` | **1 day** | Build intermediate |
| 109–115 | `argsh-builtin-so` | 7 days | Build intermediate |
| 371–378 | `argsh` | 7 days | Release artifact |
| 445–452 | `argsh-so` | 7 days | Release artifact |

**Problem:**
`minifier-bin` has 1-day retention while everything else uses 7 days. The
`minify` job depends on `minifier-bin` — if there's a CI queue delay or a
manual re-run after 24 hours, the artifact is gone and the dependent job fails.
The inconsistency is also confusing.

**Fix:**
Align all intermediates to the same retention:

```yaml
          retention-days: 7  # was 1
```

Or adopt a two-tier policy: intermediates = 3 days, release artifacts = 7 days.
Either way, 1 day is too aggressive for a build artifact consumed by downstream
jobs.

---

## Summary

| # | Category | Severity | Issue |
|---|---|---|---|
| 1 | Rust Builtin | High | No validation in `write_array`/`exec_capture` |
| 2 | Rust Builtin | **Critical** | `std::process::exit()` kills bash |
| 3 | Rust Builtin | Medium | 4 functions duplicated across files |
| 4 | Rust Builtin | Medium | `is_multiple_of` needs Rust 1.87+ |
| 5 | Rust Builtin | Medium | `get_assoc_keys` whitespace splitting |
| 6 | Rust Builtin | Medium | `is_valid_bash_name` allows colons for variables |
| 7 | Rust Builtin | Medium | Test helper loop returns only last result |
| 8 | ~~Rust Minifier~~ | ~~High~~ | ~~Quoted heredocs get obfuscated~~ — NOT AN ISSUE |
| 9 | Rust Minifier | High | Heredoc detection inside quoted strings |
| 10 | ~~Rust Minifier~~ | ~~Medium~~ | ~~Quote continuation tracks wrong type~~ — NOT AN ISSUE |
| 11 | Rust Minifier | High | Path traversal in `resolve_path` |
| 12 | Rust Minifier | Low | Regex compiled on every call |
| 13 | Rust Minifier | High | Semicolon split ignores quotes |
| 14 | Rust Minifier | High | `\\\"` escape sequence not handled |
| 15 | Bash Libraries | Low | `is::array` double `declare -p` |
| 16 | Bash Libraries | Medium | Bash 5.x empty array format change |
| 17 | Bash Libraries | Medium | Unquoted `$(echo \| tr)` in for loop |
| 18 | Bash Libraries | Medium | `lint::vale` references undeclared `vale` |
| 19 | Bash Libraries | Low | Unused `binary` variable |
| 20 | Bash Libraries | Medium | Temp files never cleaned up |
| 21 | Bash Libraries | Low | `seq` instead of arithmetic loop |
| 22 | Bash Libraries | Low | `echo \| tr` instead of parameter expansion |
| 23 | DevOps/CI | Medium | Missing concurrency group |
| 24 | DevOps/CI | High | Actions not pinned to SHA |
| 25 | DevOps/CI | High | Missing `permissions: read-all` |
| 26 | DevOps/CI | High | Docker images not pinned by digest |
| 27 | DevOps/CI | Medium | Missing `restore-keys` in cargo caches |
| 28 | DevOps/CI | Medium | `docs.yaml` actions on `@v3` |
| 29 | DevOps/CI | Low | Misaligned artifact retention |

**Severity distribution:** 1 Critical, 7 High, 11 Medium, 6 Low, 2 Not-an-issue (8, 10)
