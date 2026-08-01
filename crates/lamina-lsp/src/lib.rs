//! Lamina Language Server.

pub mod analysis;
pub mod backend;
pub mod position;

use backend::Backend;
use tower_lsp::{LspService, Server};

/// Run the LSP on stdio (blocks until client disconnects).
pub async fn run_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
