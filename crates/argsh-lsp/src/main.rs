use tower_lsp::{LspService, Server};

// Binary-local modules (interactive/stateful — only used by the LSP server,
// not by CLI tools like argsh-lint).
mod backend;
mod codelens;
mod completion;
mod format;
mod goto_def;
mod hover;
mod preview;
mod rename;
mod symbols;
mod util;

// Shared analysis modules are pulled from the library (`argsh_lsp::diagnostics`,
// `argsh_lsp::resolver`) so standalone binaries can reuse them without
// duplicating code. The binary-local modules above reference these via the
// `argsh_lsp` crate path (same crate, library-side).

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(backend::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
