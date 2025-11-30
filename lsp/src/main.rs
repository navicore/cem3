use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::RwLock;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::info;

mod completion;
mod diagnostics;
mod includes;

use includes::{IncludedWord, LocalWord};

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
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        info!("Seq LSP server initializing");

        // Check if inlay hints are enabled via initialization options
        let inlay_hints_enabled = params
            .initialization_options
            .as_ref()
            .and_then(|opts| opts.get("inlay_hints"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let inlay_hint_provider = if inlay_hints_enabled {
            info!("Inlay hints enabled");
            Some(OneOf::Right(InlayHintServerCapabilities::Options(
                InlayHintOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                },
            )))
        } else {
            None
        };

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
                document_symbol_provider: Some(OneOf::Left(true)),
                inlay_hint_provider,
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
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get document state and find word under cursor
        let (word, local_words, included_words) = if let Ok(docs) = self.documents.read() {
            if let Some(state) = docs.get(uri.as_str()) {
                let word = get_word_at_position(&state.content, position);
                (
                    word,
                    state.local_words.clone(),
                    state.included_words.clone(),
                )
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        let Some(word) = word else {
            return Ok(None);
        };

        // Look up the word in local words, included words, or builtins
        if let Some(hover) = lookup_word_hover(&word, &local_words, &included_words) {
            return Ok(Some(hover));
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get document state and find word under cursor
        let (word, local_words, included_words) = if let Ok(docs) = self.documents.read() {
            if let Some(state) = docs.get(uri.as_str()) {
                let word = get_word_at_position(&state.content, position);
                (
                    word,
                    state.local_words.clone(),
                    state.included_words.clone(),
                )
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        let Some(word) = word else {
            return Ok(None);
        };

        // Check local words first - jump to definition in current file
        for local in &local_words {
            if local.name == word {
                let location = Location {
                    uri: uri.clone(),
                    range: make_definition_range(local.start_line, &local.name),
                };
                return Ok(Some(GotoDefinitionResponse::Scalar(location)));
            }
        }

        // Check included words - jump to definition in included file
        for included in &included_words {
            if included.name == word
                && let Some(ref file_path) = included.file_path
                && file_path.exists()
                && let Ok(file_uri) = Url::from_file_path(file_path)
            {
                let location = Location {
                    uri: file_uri,
                    range: make_definition_range(included.start_line, &included.name),
                };
                return Ok(Some(GotoDefinitionResponse::Scalar(location)));
            }
        }

        // Builtins don't have a definition location
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

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        // Get local words and content from document state
        let (local_words, content) = if let Ok(docs) = self.documents.read() {
            if let Some(state) = docs.get(uri.as_str()) {
                (state.local_words.clone(), state.content.clone())
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        // Pre-compute line lengths for accurate end positions
        let line_lengths: Vec<u32> = content.lines().map(|l| l.len() as u32).collect();

        // Convert local words to document symbols for breadcrumbs
        // The range spans the entire word definition so editors show the symbol
        // when cursor is anywhere inside the definition
        // Note: We don't add child symbols for word calls because most breadcrumb
        // implementations don't do column-level matching within a line
        let symbols: Vec<DocumentSymbol> = local_words
            .iter()
            .map(|word| {
                // Get actual line length for end position
                let end_char = line_lengths.get(word.end_line).copied().unwrap_or(0);

                #[allow(deprecated)]
                DocumentSymbol {
                    name: word.name.clone(),
                    detail: None,
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position {
                            line: word.start_line as u32,
                            character: 0,
                        },
                        end: Position {
                            line: word.end_line as u32,
                            character: end_char,
                        },
                    },
                    selection_range: Range {
                        start: Position {
                            line: word.start_line as u32,
                            character: 0,
                        },
                        end: Position {
                            line: word.start_line as u32,
                            character: word.name.len() as u32 + 2, // `: name`
                        },
                    },
                    children: None,
                }
            })
            .collect();

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document state
        let (content, local_words, included_words) = if let Ok(docs) = self.documents.read() {
            if let Some(state) = docs.get(uri.as_str()) {
                (
                    state.content.clone(),
                    state.local_words.clone(),
                    state.included_words.clone(),
                )
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        let mut hints = Vec::new();

        // Process lines in the visible range
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num as u32;
            if line_num < range.start.line || line_num > range.end.line {
                continue;
            }

            // Skip comments and include lines
            let trimmed = line.trim();
            if trimmed.starts_with('\\') || trimmed.starts_with("include ") {
                continue;
            }

            // Find word calls in this line and add inlay hints
            for hint in find_word_calls_in_line(line, line_num, &local_words, &included_words) {
                hints.push(hint);
            }
        }

        Ok(Some(hints))
    }
}

/// Find word calls in a line and return inlay hints for their signatures
fn find_word_calls_in_line(
    line: &str,
    line_num: u32,
    local_words: &[includes::LocalWord],
    included_words: &[includes::IncludedWord],
) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Skip if this is a word definition line (starts with ":")
    if line.trim().starts_with(':') {
        return hints;
    }

    // Skip strings when looking for words
    let is_word_char = |c: char| c.is_alphanumeric() || "-_>?!<+=*/:".contains(c);

    let mut in_string = false;
    let mut word_start: Option<usize> = None;

    for (i, c) in line.char_indices() {
        if c == '"' {
            in_string = !in_string;
            continue;
        }

        if in_string {
            continue;
        }

        if is_word_char(c) {
            if word_start.is_none() {
                word_start = Some(i);
            }
        } else if let Some(start) = word_start {
            let word = &line[start..i];
            if let Some(hint) =
                make_inlay_hint_for_word(word, line_num, i as u32, local_words, included_words)
            {
                hints.push(hint);
            }
            word_start = None;
        }
    }

    // Handle word at end of line
    if let Some(start) = word_start {
        let word = &line[start..];
        if let Some(hint) = make_inlay_hint_for_word(
            word,
            line_num,
            line.len() as u32,
            local_words,
            included_words,
        ) {
            hints.push(hint);
        }
    }

    hints
}

/// Create an inlay hint for a word if it has a known signature
fn make_inlay_hint_for_word(
    word: &str,
    line: u32,
    end_col: u32,
    local_words: &[includes::LocalWord],
    included_words: &[includes::IncludedWord],
) -> Option<InlayHint> {
    // Skip common control flow and simple words that would be noisy
    let skip_words = [
        "if", "else", "then", "do", "loop", "begin", "until", "while", "dup", "drop", "swap",
        "over", "rot", "nip", "tuck", "pick", "+", "-", "*", "/", "=", "<", ">", "<=", ">=", "<>",
        "and", "or", "not", "true", "false",
    ];
    if skip_words.contains(&word) {
        return None;
    }

    // Look up in local words
    for local in local_words {
        if local.name == word
            && let Some(ref effect) = local.effect
        {
            let effect_str = completion::format_effect(effect);
            return Some(InlayHint {
                position: Position {
                    line,
                    character: end_col,
                },
                label: InlayHintLabel::String(format!(": {}", effect_str)),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }
    }

    // Look up in included words
    for included in included_words {
        if included.name == word
            && let Some(ref effect) = included.effect
        {
            let effect_str = completion::format_effect(effect);
            return Some(InlayHint {
                position: Position {
                    line,
                    character: end_col,
                },
                label: InlayHintLabel::String(format!(": {}", effect_str)),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }
    }

    // Look up in builtins
    for (name, effect) in seqc::builtins::builtin_signatures() {
        if name == word {
            let effect_str = completion::format_effect(&effect);
            return Some(InlayHint {
                position: Position {
                    line,
                    character: end_col,
                },
                label: InlayHintLabel::String(format!(": {}", effect_str)),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }
    }

    None
}

/// Get the word at a given position in the document
fn get_word_at_position(content: &str, position: Position) -> Option<String> {
    let line = content.lines().nth(position.line as usize)?;
    let col = position.character as usize;

    // Find word boundaries - Seq words can contain special chars like -, >, ?, !
    let is_word_char = |c: char| c.is_alphanumeric() || "-_>?!<+=*/:".contains(c);

    // Find start of word
    let start = line[..col.min(line.len())]
        .char_indices()
        .rev()
        .find(|(_, c)| !is_word_char(*c))
        .map(|(i, _)| i + 1)
        .unwrap_or(0);

    // Find end of word
    let end = line[col.min(line.len())..]
        .char_indices()
        .find(|(_, c)| !is_word_char(*c))
        .map(|(i, _)| col + i)
        .unwrap_or(line.len());

    if start >= end {
        return None;
    }

    Some(line[start..end].to_string())
}

/// Create a Range for a word definition (`: word-name`)
/// Uses character count (not byte length) for proper UTF-8 support
fn make_definition_range(start_line: usize, name: &str) -> Range {
    Range {
        start: Position {
            line: start_line as u32,
            character: 0,
        },
        end: Position {
            line: start_line as u32,
            // +2 for `: ` prefix, use chars().count() for UTF-8 correctness
            character: (name.chars().count() + 2) as u32,
        },
    }
}

/// Look up a word and return hover information
fn lookup_word_hover(
    word: &str,
    local_words: &[includes::LocalWord],
    included_words: &[includes::IncludedWord],
) -> Option<Hover> {
    use tower_lsp::lsp_types::{HoverContents, MarkupContent, MarkupKind};

    // Check local words first
    for local in local_words {
        if local.name == word {
            let effect = local
                .effect
                .as_ref()
                .map(completion::format_effect)
                .unwrap_or_else(|| "( ? )".to_string());

            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "```seq\n: {} {}\n```\n\n*Defined in this file*",
                        word, effect
                    ),
                }),
                range: None,
            });
        }
    }

    // Check included words
    for included in included_words {
        if included.name == word {
            let effect = included
                .effect
                .as_ref()
                .map(completion::format_effect)
                .unwrap_or_else(|| "( ? )".to_string());

            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "```seq\n: {} {}\n```\n\n*From {}*",
                        word, effect, included.source
                    ),
                }),
                range: None,
            });
        }
    }

    // Check builtins
    for (name, effect) in seqc::builtins::builtin_signatures() {
        if name == word {
            let signature = completion::format_effect(&effect);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```seq\n{} {}\n```\n\n*Built-in*", word, signature),
                }),
                range: None,
            });
        }
    }

    None
}

#[tokio::main]
async fn main() {
    // Handle --version flag
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("seq-lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }

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
