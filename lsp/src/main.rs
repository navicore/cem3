use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::info;

mod completion;
mod diagnostics;

use std::collections::HashMap;
use std::sync::RwLock;

struct SeqLanguageServer {
    client: Client,
    /// Document contents cache for context-aware completion
    documents: RwLock<HashMap<String, String>>,
}

impl SeqLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SeqLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        info!("Seq LSP server initializing");

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        " ".to_string(),
                        "\n".to_string(),
                        ":".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "seq-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        info!("Seq LSP server initialized");
        self.client
            .log_message(MessageType::INFO, "Seq LSP server ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Seq LSP server shutting down");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        info!("Document opened: {}", uri);

        // Cache document content
        if let Ok(mut docs) = self.documents.write() {
            docs.insert(uri.to_string(), text.clone());
        }

        let diagnostics = diagnostics::check_document(&text);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // With FULL sync, we get the entire document
        if let Some(change) = params.content_changes.into_iter().next() {
            let text = change.text;

            info!("Document changed: {}", uri);

            // Update cached document content
            if let Ok(mut docs) = self.documents.write() {
                docs.insert(uri.to_string(), text.clone());
            }

            let diagnostics = diagnostics::check_document(&text);
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        info!("Document closed: {}", uri);

        // Remove from cache
        if let Ok(mut docs) = self.documents.write() {
            docs.remove(&uri.to_string());
        }

        // Clear diagnostics when document is closed
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let _uri = params.text_document_position_params.text_document.uri;
        let _position = params.text_document_position_params.position;

        // TODO: Implement hover to show word signatures
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let _uri = params.text_document_position_params.text_document.uri;
        let _position = params.text_document_position_params.position;

        // TODO: Implement go-to-definition for word calls
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Try to get the current line prefix from cached document
        let line_prefix: Option<String> = if let Ok(docs) = self.documents.read() {
            docs.get(&uri.to_string()).and_then(|text| {
                text.lines().nth(position.line as usize).map(|line| {
                    let end = (position.character as usize).min(line.len());
                    line[..end].to_string()
                })
            })
        } else {
            None
        };

        let items = completion::get_completions(line_prefix.as_ref().map(|prefix| {
            completion::CompletionContext {
                line_prefix: prefix,
            }
        }));
        Ok(Some(CompletionResponse::Array(items)))
    }
}

#[tokio::main]
async fn main() {
    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("seq_lsp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    info!("Starting Seq LSP server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(SeqLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
