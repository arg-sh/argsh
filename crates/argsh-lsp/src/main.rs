use tower_lsp::{LspService, Server};

mod backend;
mod codelens;
mod completion;
mod diagnostics;
mod format;
mod goto_def;
mod hover;
mod preview;
mod rename;
mod resolver;
mod symbols;
mod util;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(backend::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
