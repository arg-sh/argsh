use tower_lsp::{LspService, Server};

mod backend;
mod completion;
mod diagnostics;
mod goto_def;
mod hover;
mod preview;
mod symbols;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(backend::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
