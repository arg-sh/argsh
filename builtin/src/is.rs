//! is::* builtins — variable introspection helpers.
//!
//! Mirrors: libraries/is.sh

use crate::{word_list_to_vec, BashBuiltin, SyncPtr, WordList, BUILTIN_ENABLED};
use crate::shell;
use std::ffi::{c_char, c_int};

// ── is::array ────────────────────────────────────────────────────

static IS_ARRAY_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Test if variable is declared as an array.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::array_struct"]
pub static mut IS_ARRAY_STRUCT: BashBuiltin = BashBuiltin {
    name: c"is::array".as_ptr(),
    function: is_array_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"is::array <varname>".as_ptr(),
    long_doc: IS_ARRAY_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::array_builtin_load"]
pub extern "C" fn is_array_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::array_builtin_unload"]
pub extern "C" fn is_array_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

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

// ── is::uninitialized ────────────────────────────────────────────

static IS_UNINIT_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Test if variable is uninitialized.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::uninitialized_struct"]
pub static mut IS_UNINIT_STRUCT: BashBuiltin = BashBuiltin {
    name: c"is::uninitialized".as_ptr(),
    function: is_uninit_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"is::uninitialized <varname...>".as_ptr(),
    long_doc: IS_UNINIT_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::uninitialized_builtin_load"]
pub extern "C" fn is_uninit_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::uninitialized_builtin_unload"]
pub extern "C" fn is_uninit_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn is_uninit_builtin_fn(word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        let args = word_list_to_vec(word_list);
        if args.is_empty() {
            return 2;
        }
        // Returns 0 (success) if ALL vars are uninitialized
        for name in &args {
            if shell::is_array(name) {
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

// ── is::set ──────────────────────────────────────────────────────

static IS_SET_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Test if variable is set.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::set_struct"]
pub static mut IS_SET_STRUCT: BashBuiltin = BashBuiltin {
    name: c"is::set".as_ptr(),
    function: is_set_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"is::set <varname>".as_ptr(),
    long_doc: IS_SET_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::set_builtin_load"]
pub extern "C" fn is_set_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::set_builtin_unload"]
pub extern "C" fn is_set_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

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

// ── is::tty ──────────────────────────────────────────────────────

static IS_TTY_LONG_DOC: [SyncPtr; 2] = [
    SyncPtr(c"Test if stdout is a terminal.".as_ptr()),
    SyncPtr(std::ptr::null()),
];

#[export_name = "is::tty_struct"]
pub static mut IS_TTY_STRUCT: BashBuiltin = BashBuiltin {
    name: c"is::tty".as_ptr(),
    function: is_tty_builtin_fn,
    flags: BUILTIN_ENABLED,
    short_doc: c"is::tty".as_ptr(),
    long_doc: IS_TTY_LONG_DOC.as_ptr().cast(),
    handle: std::ptr::null(),
};

#[export_name = "is::tty_builtin_load"]
pub extern "C" fn is_tty_builtin_load(_name: *const c_char) -> c_int { 1 }

#[export_name = "is::tty_builtin_unload"]
pub extern "C" fn is_tty_builtin_unload(_name: *const c_char) {} // coverage:off - bash internal callback, never called during tests

extern "C" fn is_tty_builtin_fn(_word_list: *const WordList) -> c_int {
    std::panic::catch_unwind(|| {
        if unsafe { libc::isatty(1) } == 1 { 0 } else { 1 }
    })
    .unwrap_or(1)
}
