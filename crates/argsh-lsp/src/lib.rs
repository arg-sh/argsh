//! argsh-lsp library: shared analysis code for both the LSP server binary and
//! standalone CLI tools (e.g. `argsh-lint`).
//!
//! Only the modules that are needed for static analysis outside an LSP session
//! are re-exported here. Interactive/stateful modules (backend, completion,
//! hover, etc.) remain binary-local.

pub mod diagnostics;
pub mod resolver;
