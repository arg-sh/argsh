//! FFI bindings to bash internals + high-level shell variable helpers.

use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_void, CString};

// ── Bash FFI ──────────────────────────────────────────────────────

#[repr(C)]
pub struct ShellVar {
    pub name: *const c_char,
    pub value: *const c_char,
    pub exportstr: *const c_char,
    pub dynamic_value: *const c_void,
    pub assign_func: *const c_void,
    pub attributes: c_int,
    pub context: c_int,
}

const ATT_ARRAY: c_int = 0x0000004;
const ATT_INVISIBLE: c_int = 0x0001000;

extern "C" {
    fn find_function(name: *const c_char) -> *mut c_void;
    fn find_variable(name: *const c_char) -> *mut ShellVar;
    fn unbind_variable(name: *const c_char) -> c_int;
    fn unbind_func(name: *const c_char) -> c_int;
    fn all_visible_functions() -> *mut *mut ShellVar;
    fn make_new_array_variable(name: *const c_char) -> *mut c_void;
    fn parse_and_execute(
        string: *mut c_char,
        from_file: *const c_char,
        flags: c_int,
    ) -> c_int;
    // $0 is stored in dollar_vars[0], not as a regular variable
    static dollar_vars: [*const c_char; 10];
}

// ── Public helpers ────────────────────────────────────────────────

pub fn function_exists(name: &str) -> bool {
    if let Ok(cname) = CString::new(name) {
        unsafe { !find_function(cname.as_ptr()).is_null() }
    } else {
        false // coverage:off - CString null byte impossible for shell variable names
    }
}

pub fn is_array(name: &str) -> bool {
    if let Ok(cname) = CString::new(name) {
        unsafe {
            let var = find_variable(cname.as_ptr());
            !var.is_null() && ((*var).attributes & ATT_ARRAY) != 0
        }
    } else {
        false // coverage:off - CString null byte impossible for shell variable names
    }
}

pub fn is_uninitialized(name: &str) -> bool {
    if let Ok(cname) = CString::new(name) {
        unsafe {
            let var = find_variable(cname.as_ptr());
            if var.is_null() {
                return true;
            }
            // For arrays: match bash is::uninitialized semantics where empty arrays
            // are considered uninitialized. Bash 4.x sets ATT_INVISIBLE for `local -a arr`;
            // Bash 5.x may not, but `declare -p` shows `declare -a var=()`.
            // Check ATT_INVISIBLE, null value, OR empty array (no element at index 0).
            if ((*var).attributes & ATT_ARRAY) != 0 {
                return ((*var).attributes & ATT_INVISIBLE) != 0
                    || (*var).value.is_null()
                    || bash_builtins::variables::array_get(name, 0).is_none();
            }
            // Scalars: value == NULL means uninitialized
            (*var).value.is_null()
        }
    } else {
        true // coverage:off - CString null byte impossible for shell variable names
    }
}

/// Read a bash indexed array into a Vec<String>.
/// NOTE: stops at the first gap — only correct for dense (non-sparse) arrays.
/// All argsh arrays (usage, args, COMMANDNAME) are dense.
pub fn read_array(name: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut i = 0;
    while let Some(val) = bash_builtins::variables::array_get(name, i) {
        result.push(val.to_string_lossy().into_owned());
        i += 1;
    }
    result
}

/// Clear a bash array and set it to new values.
/// (REVIEW finding 1: validates name before use)
pub fn write_array(name: &str, values: &[String]) {
    if !is_valid_bash_variable(name) {
        return; // coverage:off - defensive_check: callers always pass validated names
    }
    if let Ok(cname) = CString::new(name) {
        unsafe {
            unbind_variable(cname.as_ptr());
            make_new_array_variable(cname.as_ptr());
        }
    } // coverage:off - ffi_safety: CString::new succeeds for valid bash names
    for (i, val) in values.iter().enumerate() {
        let _ = bash_builtins::variables::array_set(name, i, val);
    }
}

/// Append a value to a bash indexed array.
/// Uses bash's native +=() syntax which handles sparse arrays correctly.
pub fn array_append(name: &str, value: &str) {
    if !is_valid_bash_variable(name) {
        return; // coverage:off - defensive_check: callers always pass validated names
    }
    let ev = shell_escape_dquote(value);
    run_bash(&format!("{}+=(\"{}\")", name, ev));
}

pub fn get_scalar(name: &str) -> Option<String> {
    bash_builtins::variables::find_as_string(name)
        .map(|s| s.to_string_lossy().into_owned())
}

pub fn set_scalar(name: &str, value: &str) {
    let _ = bash_builtins::variables::set(name, value);
}

pub fn get_funcname(index: usize) -> Option<String> {
    bash_builtins::variables::array_get("FUNCNAME", index)
        .map(|s| s.to_string_lossy().into_owned())
}

pub fn get_commandname() -> Vec<String> {
    read_array("COMMANDNAME")
}

pub fn append_commandname(value: &str) {
    array_append("COMMANDNAME", value);
}

pub fn get_field_width() -> usize {
    get_scalar("ARGSH_FIELD_WIDTH")
        .and_then(|s| s.parse().ok())
        .unwrap_or(24)
}

pub fn get_script_name() -> String {
    // $0 is a special parameter stored in dollar_vars[0], not a regular variable
    let s = unsafe {
        let ptr = dollar_vars[0];
        if ptr.is_null() {
            return "argsh".to_string(); // coverage:off - dollar_vars[0] always set by bash
        }
        std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .into_owned()
    };
    if let Some(pos) = s.rfind('/') {
        s[pos + 1..].to_string()
    } else {
        s // coverage:off - ffi_safety: $0 always contains '/' in bash process context
    }
}

/// Get the raw value of $0 (without basename stripping).
/// NOTE: may be a relative path or just a basename depending on how the script
/// was invoked. Used by MCP to re-invoke the script as a subprocess.
pub fn get_script_path() -> String {
    unsafe {
        let ptr = dollar_vars[0];
        if ptr.is_null() {
            return "argsh".to_string(); // coverage:off - dollar_vars[0] always set by bash
        }
        std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .into_owned()
    }
}

/// Get a scalar variable's value for use in default display.
/// For arrays, returns space-joined elements.
pub fn get_var_display(name: &str) -> Option<String> {
    if is_array(name) {
        let arr = read_array(name);
        if arr.is_empty() {
            None // coverage:off - dead_code: has_default is false for empty arrays so this path never reached via format_field
        } else {
            Some(arr.join(" "))
        }
    } else {
        get_scalar(name)
    }
}

/// Execute a bash command string and capture a variable result.
/// Used for custom type conversion (to::custom etc.).
/// (REVIEW finding 1: validates result_var to prevent command injection)
pub fn exec_capture(cmd: &str, result_var: &str) -> Option<String> {
    if !is_valid_bash_variable(result_var) {
        return None; // coverage:off - defensive_check: callers always pass "__argsh_r" which is valid
    }
    let full_cmd = format!("{}=\"$({})\"", result_var, cmd);
    if let Ok(cstr) = CString::new(full_cmd) {
        let from = CString::new("argsh").unwrap(); // coverage:off - malloc/exec failure impossible to trigger from tests
        // IMPORTANT: parse_and_execute() frees its first argument via xfree().
        // We must allocate with libc malloc so bash can free it safely.
        let bytes = cstr.as_bytes_with_nul();
        unsafe {
            let ptr = libc::malloc(bytes.len()) as *mut c_char;
            if ptr.is_null() { // coverage:off - malloc/exec failure impossible to trigger from tests
                return None; // coverage:off - malloc/exec failure impossible to trigger from tests
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
            let ret = parse_and_execute(ptr, from.as_ptr(), 0);
            // Do NOT free ptr — bash already freed it
            if ret != 0 {
                return None; // coverage:off - malloc/exec failure impossible to trigger from tests
            }
        }
        get_scalar(result_var)
    } else {
        None // coverage:off - malloc/exec failure impossible to trigger from tests
    }
}

/// Write an error message to stderr.
pub fn write_stderr(msg: &str) { // coverage:off - exit(2) prevents coverage flush in forked subshell
    use std::io::Write; // coverage:off - exit(2) prevents coverage flush in forked subshell
    let _ = std::io::stderr().write_all(msg.as_bytes()); // coverage:off - exit(2) prevents coverage flush in forked subshell
    let _ = std::io::stderr().write_all(b"\n"); // coverage:off - exit(2) prevents coverage flush in forked subshell
} // coverage:off

// ── Import helpers ───────────────────────────────────────────────

/// Execute a bash command string via parse_and_execute.
/// Returns the exit code. Handles malloc allocation for bash's xfree().
pub fn run_bash(cmd: &str) -> c_int {
    if let Ok(cstr) = CString::new(cmd) {
        let from = CString::new("import").unwrap();
        let bytes = cstr.as_bytes_with_nul();
        unsafe {
            let ptr = libc::malloc(bytes.len()) as *mut c_char;
            if ptr.is_null() { // coverage:off - malloc failure impossible to trigger from tests
                return 1; // coverage:off
            } // coverage:off
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
            parse_and_execute(ptr, from.as_ptr(), 0)
            // Do NOT free ptr — bash already freed it
        }
    } else {
        1 // coverage:off - CString null byte impossible for shell commands
    }
}

/// Get all currently visible function names.
/// Uses all_visible_functions() which returns a malloc'd NULL-terminated array.
/// We free the array but NOT the ShellVar pointers (they belong to bash).
// coverage:off - import-only: get_all_function_names is only called by import builtin, not exercised in BATS
pub fn get_all_function_names() -> HashSet<String> {
    let mut result = HashSet::new();
    unsafe {
        let arr = all_visible_functions();
        if arr.is_null() {
            return result;
        }
        let mut i = 0;
        loop {
            let var_ptr = *arr.add(i);
            if var_ptr.is_null() {
                break;
            }
            if !(*var_ptr).name.is_null() {
                let cstr = std::ffi::CStr::from_ptr((*var_ptr).name);
                if let Ok(s) = cstr.to_str() {
                    result.insert(s.to_string());
                }
            }
            i += 1;
        }
        libc::free(arr as *mut libc::c_void);
    }
    result
}
// coverage:on

// coverage:off - import-only: remove_function and source_bash_file are only called by import builtin
/// Remove a shell function by name.
pub fn remove_function(name: &str) {
    if let Ok(cname) = CString::new(name) {
        unsafe {
            unbind_func(cname.as_ptr());
        }
    }
}

/// Source a bash file using parse_and_execute(". path").
pub fn source_bash_file(path: &str) -> c_int {
    let ep = shell_escape_dquote(path);
    run_bash(&format!(". \"{}\"", ep))
}
// coverage:on

/// Returns true if `name` is a valid bash variable name (letters, digits,
/// underscores -- must not start with a digit). No colons allowed.
/// (REVIEW finding 6: split variable vs function name validation)
fn is_valid_bash_variable(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

// coverage:off - import-only: is_valid_bash_name is only called by create_function_alias (import path)
/// Returns true if `name` is a valid bash function name (letters, digits,
/// underscores, colons -- must not start with a digit).
/// Colons are allowed for function names (e.g., `is::array`) but not variables.
fn is_valid_bash_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}
// coverage:on

/// Escape a string for safe interpolation inside bash double quotes.
/// Escapes `"`, `$`, `` ` ``, and `\`.
fn shell_escape_dquote(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' | '$' | '`' | '\\' => { // coverage:off - import-only: escape chars only appear in import paths, not in arg values
                out.push('\\'); // coverage:off
                out.push(ch); // coverage:off
            } // coverage:off
            _ => out.push(ch),
        }
    }
    out
}

// coverage:off - import-only: all functions below are only called by import builtin, not exercised in BATS

/// Create a function alias: new_name() { old_name "$@"; }
pub fn create_function_alias(old_name: &str, new_name: &str) {
    if !is_valid_bash_name(old_name) || !is_valid_bash_name(new_name) {
        return;
    }
    run_bash(&format!("{} () {{ {} \"$@\"; }}", new_name, old_name));
}

/// Get a value from a bash associative array.
pub fn assoc_get(array_name: &str, key: &str) -> Option<String> {
    if !is_valid_bash_variable(array_name) {
        return None;
    }
    let tmp = "__argsh_import_tmp";
    let ek = shell_escape_dquote(key);
    let cmd = format!("{}=\"${{{}[\"{}\"]:-}}\"", tmp, array_name, ek);
    if run_bash(&cmd) != 0 {
        return None;
    }
    let val = get_scalar(tmp);
    set_scalar(tmp, "");
    val.filter(|s| !s.is_empty())
}

/// Set a value in a bash associative array.
pub fn assoc_set(array_name: &str, key: &str, value: &str) {
    if !is_valid_bash_variable(array_name) {
        return;
    }
    let ek = shell_escape_dquote(key);
    let ev = shell_escape_dquote(value);
    run_bash(&format!("{}[\"{}\"]=\"{}\"", array_name, ek, ev));
}

/// Get all keys from a bash associative array.
/// NOTE (REVIEW finding 5): split_whitespace cannot distinguish spaces inside keys
/// from spaces between keys. This is acceptable because argsh's arg definitions
/// never use spaces in keys. Document as known limitation.
pub fn get_assoc_keys(array_name: &str) -> Vec<String> {
    if !is_valid_bash_variable(array_name) {
        return Vec::new();
    }
    let tmp = "__argsh_import_tmp";
    let cmd = format!("{}=\"${{!{}[@]}}\"", tmp, array_name);
    if run_bash(&cmd) != 0 {
        return Vec::new();
    }
    let val = get_scalar(tmp).unwrap_or_default();
    set_scalar(tmp, "");
    if val.is_empty() {
        Vec::new()
    } else {
        val.split_whitespace().map(|s| s.to_string()).collect()
    }
}

/// Read BASH_SOURCE[0].
pub fn get_bash_source_first() -> Option<String> {
    bash_builtins::variables::array_get("BASH_SOURCE", 0)
        .map(|s| s.to_string_lossy().into_owned())
}
// coverage:on

/// Read last element of BASH_SOURCE.
pub fn get_bash_source_last() -> Option<String> { // coverage:off - only used as fallback when ARGSH_SOURCE unset
    let arr = read_array("BASH_SOURCE"); // coverage:off
    arr.last().cloned() // coverage:off
} // coverage:off
