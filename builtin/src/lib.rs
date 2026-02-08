//! argsh loadable builtins — self-contained argsh core as native bash builtins.
//!
//! Provides: :usage, :args, is::array, is::uninitialized, is::set, is::tty,
//!           args::field_name, to::int, to::float, to::boolean, to::file
//!
//! Build: cargo build --release
//! Load:  enable -f ./target/release/libargsh_builtin.so :usage :args \
//!            is::array is::uninitialized is::set is::tty args::field_name \
//!            to::int to::float to::boolean to::file

mod args_cmd;
mod field;
mod shell;
mod usage;

use std::ffi::{c_char, c_int};

// Raw pointer wrapper that is Sync (safe for static bash structs)
struct SyncPtr(*const c_char);
unsafe impl Sync for SyncPtr {}

// ── FFI types matching bash's struct builtin ──────────────────────

// Safety: BashBuiltin is only accessed from bash's main thread
unsafe impl Sync for BashBuiltin {}

#[repr(C)]
pub struct WordList {
    pub next: *const WordList,
    pub word: *const WordDesc,
}

#[repr(C)]
pub struct WordDesc {
    pub word: *const c_char,
    pub flags: c_int,
}

type BuiltinFunc = extern "C" fn(*const WordList) -> c_int;

#[repr(C)]
pub struct BashBuiltin {
    pub name: *const c_char,
    pub function: BuiltinFunc,
    pub flags: c_int,
    pub long_doc: *const *const c_char,
    pub short_doc: *const c_char,
    pub handle: *const c_char,
}

const BUILTIN_ENABLED: c_int = 0x01;

// ── Helper: iterate WordList into Vec<String> ────────────────────

fn word_list_to_vec(wl: *const WordList) -> Vec<String> {
    let mut result = Vec::new();
    let mut cur = wl;
    while !cur.is_null() {
        unsafe {
            if !(*cur).word.is_null() && !(*(*cur).word).word.is_null() {
                let cstr = std::ffi::CStr::from_ptr((*(*cur).word).word);
                if let Ok(s) = cstr.to_str() {
                    result.push(s.to_string());
                }
            }
            cur = (*cur).next;
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════
//  :usage builtin
// ═══════════════════════════════════════════════════════════════════

static USAGE_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Parse subcommands from the usage array and dispatch.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":usage_struct"]
pub static mut USAGE_STRUCT: BashBuiltin = BashBuiltin {
    name: b":usage\0".as_ptr().cast(),
    function: usage_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b":usage <title> [args...]\0".as_ptr().cast(),
    long_doc: USAGE_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":usage_builtin_load"]
pub extern "C" fn usage_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":usage_builtin_unload"]
pub extern "C" fn usage_builtin_unload(_name: *const c_char) {}

extern "C" fn usage_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        usage::usage_main(&args)
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  :args builtin
// ═══════════════════════════════════════════════════════════════════

static ARGS_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Parse arguments and flags from the args array.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = ":args_struct"]
pub static mut ARGS_STRUCT: BashBuiltin = BashBuiltin {
    name: b":args\0".as_ptr().cast(),
    function: args_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b":args <title> [args...]\0".as_ptr().cast(),
    long_doc: ARGS_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = ":args_builtin_load"]
pub extern "C" fn args_builtin_load(_name: *const c_char) -> c_int {
    1 // success
}

#[export_name = ":args_builtin_unload"]
pub extern "C" fn args_builtin_unload(_name: *const c_char) {}

extern "C" fn args_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        args_cmd::args_main(&args)
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  is::array builtin — test if variable is declared as array
// ═══════════════════════════════════════════════════════════════════

static IS_ARRAY_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Test if variable is declared as an array.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::array_struct"]
pub static mut IS_ARRAY_STRUCT: BashBuiltin = BashBuiltin {
    name: b"is::array\0".as_ptr().cast(),
    function: is_array_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"is::array <varname>\0".as_ptr().cast(),
    long_doc: IS_ARRAY_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::array_builtin_load"]
pub extern "C" fn is_array_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::array_builtin_unload"]
pub extern "C" fn is_array_builtin_unload(_name: *const c_char) {}

extern "C" fn is_array_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        if args.is_empty() {
            return 2;
        }
        // Returns 0 (success) if array, 1 (failure) if not
        for name in &args {
            if !shell::is_array(name) {
                return 1;
            }
        }
        0
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  is::uninitialized builtin — test if variable is uninitialized
// ═══════════════════════════════════════════════════════════════════

static IS_UNINIT_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Test if variable is uninitialized.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::uninitialized_struct"]
pub static mut IS_UNINIT_STRUCT: BashBuiltin = BashBuiltin {
    name: b"is::uninitialized\0".as_ptr().cast(),
    function: is_uninit_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"is::uninitialized <varname...>\0".as_ptr().cast(),
    long_doc: IS_UNINIT_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::uninitialized_builtin_load"]
pub extern "C" fn is_uninit_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::uninitialized_builtin_unload"]
pub extern "C" fn is_uninit_builtin_unload(_name: *const c_char) {}

extern "C" fn is_uninit_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        if args.is_empty() {
            return 2;
        }
        // Returns 0 (success) if ALL vars are uninitialized
        for name in &args {
            if shell::is_array(name) {
                // Array: check declare -p output format (declared but empty)
                if !shell::is_uninitialized(name) {
                    return 1;
                }
            } else if !shell::is_uninitialized(name) {
                return 1;
            }
        }
        0
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  is::set builtin — test if variable is set (opposite of uninitialized)
// ═══════════════════════════════════════════════════════════════════

static IS_SET_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Test if variable is set.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::set_struct"]
pub static mut IS_SET_STRUCT: BashBuiltin = BashBuiltin {
    name: b"is::set\0".as_ptr().cast(),
    function: is_set_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"is::set <varname>\0".as_ptr().cast(),
    long_doc: IS_SET_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::set_builtin_load"]
pub extern "C" fn is_set_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::set_builtin_unload"]
pub extern "C" fn is_set_builtin_unload(_name: *const c_char) {}

extern "C" fn is_set_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        if args.is_empty() {
            return 2;
        }
        for name in &args {
            if shell::is_uninitialized(name) {
                return 1;
            }
        }
        0
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  is::tty builtin — test if stdout is a terminal
// ═══════════════════════════════════════════════════════════════════

static IS_TTY_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Test if stdout is a terminal.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::tty_struct"]
pub static mut IS_TTY_STRUCT: BashBuiltin = BashBuiltin {
    name: b"is::tty\0".as_ptr().cast(),
    function: is_tty_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"is::tty\0".as_ptr().cast(),
    long_doc: IS_TTY_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::tty_builtin_load"]
pub extern "C" fn is_tty_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::tty_builtin_unload"]
pub extern "C" fn is_tty_builtin_unload(_name: *const c_char) {}

extern "C" fn is_tty_builtin_fn(_word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        if unsafe { libc::isatty(1) } == 1 { 0 } else { 1 }
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  args::field_name builtin — extract variable name from field def
// ═══════════════════════════════════════════════════════════════════

static FIELD_NAME_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Extract variable name from an argsh field definition.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "args::field_name_struct"]
pub static mut FIELD_NAME_STRUCT: BashBuiltin = BashBuiltin {
    name: b"args::field_name\0".as_ptr().cast(),
    function: field_name_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"args::field_name <field> [asref]\0".as_ptr().cast(),
    long_doc: FIELD_NAME_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "args::field_name_builtin_load"]
pub extern "C" fn field_name_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "args::field_name_builtin_unload"]
pub extern "C" fn field_name_builtin_unload(_name: *const c_char) {}

extern "C" fn field_name_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        if args.is_empty() {
            return 2;
        }
        let asref = args.get(1).map(|s| s != "0").unwrap_or(true);
        let name = field::field_name(&args[0], asref);
        println!("{}", name);
        0
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  to::int builtin — validate integer type
// ═══════════════════════════════════════════════════════════════════

static TO_INT_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Validate and echo integer value.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::int_struct"]
pub static mut TO_INT_STRUCT: BashBuiltin = BashBuiltin {
    name: b"to::int\0".as_ptr().cast(),
    function: to_int_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"to::int <value>\0".as_ptr().cast(),
    long_doc: TO_INT_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::int_builtin_load"]
pub extern "C" fn to_int_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::int_builtin_unload"]
pub extern "C" fn to_int_builtin_unload(_name: *const c_char) {}

extern "C" fn to_int_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        let value = args.first().map(|s| s.as_str()).unwrap_or("");
        if value.parse::<i64>().is_ok() {
            println!("{}", value);
            0
        } else {
            1
        }
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  to::float builtin — validate float type
// ═══════════════════════════════════════════════════════════════════

static TO_FLOAT_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Validate and echo float value.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::float_struct"]
pub static mut TO_FLOAT_STRUCT: BashBuiltin = BashBuiltin {
    name: b"to::float\0".as_ptr().cast(),
    function: to_float_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"to::float <value>\0".as_ptr().cast(),
    long_doc: TO_FLOAT_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::float_builtin_load"]
pub extern "C" fn to_float_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::float_builtin_unload"]
pub extern "C" fn to_float_builtin_unload(_name: *const c_char) {}

extern "C" fn to_float_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        let value = args.first().map(|s| s.as_str()).unwrap_or("");
        let valid = value
            .strip_prefix('-')
            .unwrap_or(value)
            .split_once('.')
            .map(|(a, b)| {
                !a.is_empty()
                    && a.chars().all(|c| c.is_ascii_digit())
                    && !b.is_empty()
                    && b.chars().all(|c| c.is_ascii_digit())
            })
            .unwrap_or_else(|| {
                let s = value.strip_prefix('-').unwrap_or(value);
                !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
            });
        if valid {
            println!("{}", value);
            0
        } else {
            1
        }
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  to::boolean builtin — validate boolean type
// ═══════════════════════════════════════════════════════════════════

static TO_BOOL_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Convert value to boolean (0 or 1).\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::boolean_struct"]
pub static mut TO_BOOL_STRUCT: BashBuiltin = BashBuiltin {
    name: b"to::boolean\0".as_ptr().cast(),
    function: to_bool_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"to::boolean <value>\0".as_ptr().cast(),
    long_doc: TO_BOOL_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::boolean_builtin_load"]
pub extern "C" fn to_bool_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::boolean_builtin_unload"]
pub extern "C" fn to_bool_builtin_unload(_name: *const c_char) {}

extern "C" fn to_bool_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        let value = args.first().map(|s| s.as_str()).unwrap_or("");
        match value {
            "" | "false" | "0" => println!("0"),
            _ => println!("1"),
        }
        0
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  to::file builtin — validate file exists
// ═══════════════════════════════════════════════════════════════════

static TO_FILE_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Validate that the value is an existing file path.\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::file_struct"]
pub static mut TO_FILE_STRUCT: BashBuiltin = BashBuiltin {
    name: b"to::file\0".as_ptr().cast(),
    function: to_file_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"to::file <path>\0".as_ptr().cast(),
    long_doc: TO_FILE_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::file_builtin_load"]
pub extern "C" fn to_file_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::file_builtin_unload"]
pub extern "C" fn to_file_builtin_unload(_name: *const c_char) {}

extern "C" fn to_file_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        let value = args.first().map(|s| s.as_str()).unwrap_or("");
        if std::path::Path::new(value).is_file() {
            println!("{}", value);
            0
        } else {
            1
        }
    })
    .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════════
//  to::string builtin — passthrough (identity)
// ═══════════════════════════════════════════════════════════════════

static TO_STRING_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(b"Echo string value (identity conversion).\0".as_ptr().cast()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::string_struct"]
pub static mut TO_STRING_STRUCT: BashBuiltin = BashBuiltin {
    name: b"to::string\0".as_ptr().cast(),
    function: to_string_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: b"to::string <value>\0".as_ptr().cast(),
    long_doc: TO_STRING_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::string_builtin_load"]
pub extern "C" fn to_string_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::string_builtin_unload"]
pub extern "C" fn to_string_builtin_unload(_name: *const c_char) {}

extern "C" fn to_string_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        let value = args.first().map(|s| s.as_str()).unwrap_or("");
        println!("{}", value);
        0
    })
    .unwrap_or(1)
}
