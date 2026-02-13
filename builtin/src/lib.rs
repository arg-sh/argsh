//! argsh loadable builtins — native bash builtins compiled from Rust.
//!
//! Module layout mirrors libraries/*.sh:
//!   args.rs  ← args.sh (:args)
//!   usage/   ← args.sh (:usage, :usage::help, :usage::completion, :usage::docgen, :usage::mcp)
//!   field.rs ← args.sh (args::field_name, field parsing)
//!   is.rs    ← is.sh   (is::array, is::uninitialized, is::set, is::tty)
//!   to.rs    ← to.sh   (to::int, to::float, to::boolean, to::file, to::string)
//!   shell.rs — bash FFI bridge (no .sh counterpart)
//!
//! Build: cargo build --release
//! Load:  enable -f ./target/release/libargsh.so :usage :usage::help \
//!            :usage::completion :usage::docgen :usage::mcp :args \
//!            is::array is::uninitialized is::set is::tty args::field_name \
//!            to::int to::float to::boolean to::file to::string

mod args;
mod field;
mod import;
mod is;
mod shared;
mod shell;
mod to;
mod usage;

use std::ffi::{c_char, c_int};

// ── Shared FFI types matching bash's struct builtin ──────────────

/// Raw pointer wrapper that is Sync (safe for static bash structs).
pub struct SyncPtr(pub *const c_char);
unsafe impl Sync for SyncPtr {}

// Safety: BashBuiltin is only accessed from bash's main thread.
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

pub type BuiltinFunc = extern "C" fn(*const WordList) -> c_int;

#[repr(C)]
pub struct BashBuiltin {
    pub name: *const c_char,
    pub function: BuiltinFunc,
    pub flags: c_int,
    pub long_doc: *const *const c_char,
    pub short_doc: *const c_char,
    pub handle: *const c_char,
}

pub const BUILTIN_ENABLED: c_int = 0x01;

// ── Helper: iterate WordList into Vec<String> ────────────────────

pub fn word_list_to_vec(wl: *const WordList) -> Vec<String> {
    let mut result = Vec::new();
    let mut cur = wl;
    while !cur.is_null() {
        unsafe {
            // coverage:off - bash guarantees WordList entries have valid word pointers
            if !(*cur).word.is_null() && !(*(*cur).word).word.is_null() {
                // coverage:on
                let cstr = std::ffi::CStr::from_ptr((*(*cur).word).word);
                if let Ok(s) = cstr.to_str() {
                    result.push(s.to_string());
                }
            } // coverage:off - ffi_safety: closing brace of always-true null check
            cur = (*cur).next;
        }
    }
    result
}
