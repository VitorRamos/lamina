//! `lamina-lsp` binary — Language Server for Lamina (stdio).

#[tokio::main]
async fn main() {
    // Log to stderr so stdout stays clean for LSP JSON-RPC.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    lamina_lsp::run_stdio().await;
}
