//! to::* builtins — type validation and conversion.
//!
//! Mirrors: libraries/to.sh

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use std::ffi::{c_char, c_int};

// ── to::int ──────────────────────────────────────────────────────

static TO_INT_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Validate and echo integer value.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::int_struct"]
pub static mut TO_INT_STRUCT: BashBuiltin = BashBuiltin {
    name: c"to::int".as_ptr(),
    function: to_int_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"to::int <value>".as_ptr(),
    long_doc: TO_INT_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::int_builtin_load"]
pub extern "C" fn to_int_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::int_builtin_unload"]
pub extern "C" fn to_int_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

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

// ── to::float ────────────────────────────────────────────────────

static TO_FLOAT_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Validate and echo float value.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::float_struct"]
pub static mut TO_FLOAT_STRUCT: BashBuiltin = BashBuiltin {
    name: c"to::float".as_ptr(),
    function: to_float_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"to::float <value>".as_ptr(),
    long_doc: TO_FLOAT_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::float_builtin_load"]
pub extern "C" fn to_float_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::float_builtin_unload"]
pub extern "C" fn to_float_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

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

// ── to::boolean ──────────────────────────────────────────────────

static TO_BOOL_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Convert value to boolean (0 or 1).".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::boolean_struct"]
pub static mut TO_BOOL_STRUCT: BashBuiltin = BashBuiltin {
    name: c"to::boolean".as_ptr(),
    function: to_bool_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"to::boolean <value>".as_ptr(),
    long_doc: TO_BOOL_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::boolean_builtin_load"]
pub extern "C" fn to_bool_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::boolean_builtin_unload"]
pub extern "C" fn to_bool_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

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

// ── to::file ─────────────────────────────────────────────────────

static TO_FILE_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Validate that the value is an existing file path.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::file_struct"]
pub static mut TO_FILE_STRUCT: BashBuiltin = BashBuiltin {
    name: c"to::file".as_ptr(),
    function: to_file_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"to::file <path>".as_ptr(),
    long_doc: TO_FILE_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::file_builtin_load"]
pub extern "C" fn to_file_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::file_builtin_unload"]
pub extern "C" fn to_file_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

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

// ── to::string ───────────────────────────────────────────────────

static TO_STRING_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Echo string value (identity conversion).".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "to::string_struct"]
pub static mut TO_STRING_STRUCT: BashBuiltin = BashBuiltin {
    name: c"to::string".as_ptr(),
    function: to_string_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"to::string <value>".as_ptr(),
    long_doc: TO_STRING_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "to::string_builtin_load"]
pub extern "C" fn to_string_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "to::string_builtin_unload"]
pub extern "C" fn to_string_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn to_string_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        let value = args.first().map(|s| s.as_str()).unwrap_or("");
        println!("{}", value);
        0
    })
    .unwrap_or(1)
}
