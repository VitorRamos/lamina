//! tower-lsp backend for Lamina.

use crate::analysis::{analyze, goto_at, hover_at, stage_method_completions, Analysis};
use crate::position::position_to_offset;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, String>>>,
    analyses: Arc<RwLock<HashMap<Url, Analysis>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            analyses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn reanalyze(&self, uri: &Url, text: &str) {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.path()));
        let analysis = analyze(&path, text);
        let diags = analysis.diagnostics.clone();
        self.analyses.write().await.insert(uri.clone(), analysis);
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "lamina-lsp".into(),
                version: Some(lamina_lang::VERSION.into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".into()]),
                    ..Default::default()
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "lamina-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents
            .write()
            .await
            .insert(uri.clone(), text.clone());
        self.reanalyze(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Full sync: last change has full text
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;
            self.documents
                .write()
                .await
                .insert(uri.clone(), text.clone());
            self.reanalyze(&uri, &text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        self.analyses.write().await.remove(&uri);
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let analyses = self.analyses.read().await;
        let Some(analysis) = analyses.get(&uri) else {
            return Ok(None);
        };
        Ok(hover_at(analysis, pos).map(|contents| Hover {
            contents: HoverContents::Markup(contents),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let analyses = self.analyses.read().await;
        let Some(analysis) = analyses.get(&uri) else {
            return Ok(None);
        };
        Ok(goto_at(analysis, pos).map(GotoDefinitionResponse::Scalar))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let docs = self.documents.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };
        let offset = position_to_offset(text, pos);
        // Trigger after '.' for Stage methods
        let after_dot = offset > 0 && text.as_bytes().get(offset.saturating_sub(1)) == Some(&b'.');
        if !after_dot {
            // Still offer method names if user typed partially after dot earlier
            let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let prefix = &text[line_start..offset];
            if !prefix.contains('.') {
                return Ok(None);
            }
        }
        let items: Vec<CompletionItem> = stage_method_completions()
            .into_iter()
            .map(|(name, detail)| CompletionItem {
                label: name.into(),
                kind: Some(CompletionItemKind::METHOD),
                detail: Some(detail.into()),
                insert_text: Some(format!("{name}($0)")),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            })
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        let Some(text) = docs.get(&uri) else {
            return Ok(None);
        };
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from("buffer.lam"));
        match lamina_lang::fmt::format_source(path.to_string_lossy().as_ref(), text) {
            Ok(formatted) if formatted != *text => {
                let end = crate::position::offset_to_position(text, text.len());
                Ok(Some(vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end,
                    },
                    new_text: formatted,
                }]))
            }
            Ok(_) => Ok(Some(vec![])),
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("format failed: {e}"))
                    .await;
                Ok(None)
            }
        }
    }
}
