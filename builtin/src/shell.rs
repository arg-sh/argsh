//! FFI bindings to bash internals + high-level shell variable helpers.

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
            // For arrays: `local -a arr` sets ATT_INVISIBLE (declared but not assigned).
            // Array value is an ARRAY* pointer, not null, so we check the flag instead.
            if ((*var).attributes & ATT_ARRAY) != 0 {
                return ((*var).attributes & ATT_INVISIBLE) != 0 || (*var).value.is_null();
            }
            // Scalars: value == NULL means uninitialized
            (*var).value.is_null()
        }
    } else {
        true // coverage:off - CString null byte impossible for shell variable names
    }
}

/// Read a bash indexed array into a Vec<String>.
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

/// Append a value to a bash indexed array.
pub fn array_append(name: &str, value: &str) {
    // Find current length
    let mut len = 0;
    while bash_builtins::variables::array_get(name, len).is_some() {
        len += 1;
    }
    let _ = bash_builtins::variables::array_set(name, len, value);
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
        s
    }
}

/// Get a scalar variable's value for use in default display.
/// For arrays, returns space-joined elements.
pub fn get_var_display(name: &str) -> Option<String> {
    if is_array(name) {
        let arr = read_array(name);
        if arr.is_empty() {
            None
        } else {
            Some(arr.join(" "))
        }
    } else {
        get_scalar(name)
    }
}

/// Execute a bash command string and capture a variable result.
/// Used for custom type conversion (to::custom etc.).
pub fn exec_capture(cmd: &str, result_var: &str) -> Option<String> {
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
}
