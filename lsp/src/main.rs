use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::info;

mod completion;
mod diagnostics;
mod includes;

use includes::{IncludedWord, LocalWord};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

/// State for a single document
struct DocumentState {
    /// Document content
    content: String,
    /// File path (for resolving relative includes)
    file_path: Option<PathBuf>,
    /// Words available from includes (cached)
    included_words: Vec<IncludedWord>,
    /// Words defined in this document
    local_words: Vec<LocalWord>,
}

struct SeqLanguageServer {
    client: Client,
    /// Document state cache
    documents: RwLock<HashMap<String, DocumentState>>,
    /// Path to stdlib (cached on startup)
    stdlib_path: Option<PathBuf>,
}

impl SeqLanguageServer {
    fn new(client: Client) -> Self {
        let stdlib_path = includes::find_stdlib_path();
        if let Some(ref path) = stdlib_path {
            info!("Found stdlib at: {}", path.display());
        } else {
            info!("Stdlib not found - include completions will be limited");
        }

        Self {
            client,
            documents: RwLock::new(HashMap::new()),
            stdlib_path,
        }
    }

    /// Get all words available from includes for a document
    fn get_included_words(&self, uri: &str) -> Vec<IncludedWord> {
        if let Ok(docs) = self.documents.read()
            && let Some(state) = docs.get(uri)
        {
            return state.included_words.clone();
        }
        Vec::new()
    }

    /// Update document state and resolve includes
    fn update_document(&self, uri: &str, content: String, file_path: Option<PathBuf>) {
        // Parse document to extract includes and local words
        let (includes, local_words) = includes::parse_document(&content);

        info!(
            "Parsed document: {} includes, {} local words, stdlib_path={:?}, file_path={:?}",
            includes.len(),
            local_words.len(),
            self.stdlib_path,
            file_path
        );

        // Resolve includes to get words from included files
        let included_words = includes::resolve_includes(
            &includes,
            file_path.as_deref(),
            self.stdlib_path.as_deref(),
        );

        info!(
            "Document has {} local words, {} included words from {} includes",
            local_words.len(),
            included_words.len(),
            includes.len()
        );

        let state = DocumentState {
            content,
            file_path,
            included_words,
            local_words,
        };

        if let Ok(mut docs) = self.documents.write() {
            docs.insert(uri.to_string(), state);
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

        // Extract file path from URI
        let file_path = includes::uri_to_path(uri.as_str());
        info!("File path: {:?}", file_path);

        // Update document state (parses includes)
        self.update_document(uri.as_str(), text.clone(), file_path);

        // Get included words for diagnostics
        let included_words = self.get_included_words(uri.as_str());
        info!(
            "Got {} included words for diagnostics",
            included_words.len()
        );

        let diagnostics = diagnostics::check_document_with_includes(&text, &included_words);
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

            // Get existing file path from cache
            let file_path = if let Ok(docs) = self.documents.read() {
                docs.get(uri.as_str()).and_then(|s| s.file_path.clone())
            } else {
                None
            };

            // Update document state (re-parses includes)
            self.update_document(uri.as_str(), text.clone(), file_path);

            // Get included words for diagnostics
            let included_words = self.get_included_words(uri.as_str());

            let diagnostics = diagnostics::check_document_with_includes(&text, &included_words);
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
            docs.remove(uri.as_str());
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

        // Get document state
        let (line_prefix, included_words, local_words) = if let Ok(docs) = self.documents.read() {
            if let Some(state) = docs.get(uri.as_str()) {
                let prefix = state
                    .content
                    .lines()
                    .nth(position.line as usize)
                    .map(|line| {
                        let end = (position.character as usize).min(line.len());
                        line[..end].to_string()
                    });
                (
                    prefix,
                    state.included_words.clone(),
                    state.local_words.clone(),
                )
            } else {
                (None, Vec::new(), Vec::new())
            }
        } else {
            (None, Vec::new(), Vec::new())
        };

        let context = line_prefix
            .as_ref()
            .map(|prefix| completion::CompletionContext {
                line_prefix: prefix,
                included_words: &included_words,
                local_words: &local_words,
            });

        let items = completion::get_completions(context);
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
