//! argsh-syntax — pure-Rust parsing library for argsh field definitions,
//! usage entries, annotations, and source-file analysis.
//!
//! This crate contains no FFI or shell interaction. It operates on strings
//! and produces structured data that both the `builtin/` runtime and the
//! `lsp/` language server can consume.

pub mod document;
pub mod field;
pub mod scope;
pub mod usage;

// Re-exports for convenience.
pub use document::{
    analyze, ArgsArrayEntry, DocumentAnalysis, FunctionInfo, ImportStatement, LocalVar,
};
pub use field::{parse_field, FieldDef, FieldError};
pub use scope::{Scope, ScopeChain};
pub use usage::{parse_annotations, parse_usage_entry, UsageEntry};
