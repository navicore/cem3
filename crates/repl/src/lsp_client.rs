//! LSP Client for TUI REPL
//!
//! Connects to seq-lsp to provide completions.
//! Uses JSON-RPC over stdin/stdout to communicate with the language server.
//!
//! Ported from crates/repl/src/lsp_client.rs

use lsp_types::{
    ClientCapabilities, CompletionItem, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, InitializeResult,
    Position, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, Uri, VersionedTextDocumentIdentifier,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};

/// JSON-RPC request
#[derive(Serialize)]
struct Request<T> {
    jsonrpc: &'static str,
    id: i64,
    method: &'static str,
    params: T,
}

/// JSON-RPC notification (no id, no response expected)
#[derive(Serialize)]
struct Notification<T> {
    jsonrpc: &'static str,
    method: &'static str,
    params: T,
}

/// JSON-RPC response
#[derive(Deserialize, Debug)]
#[allow(dead_code)] // jsonrpc field is required by protocol but not used
struct Response<T> {
    jsonrpc: String,
    id: Option<i64>,
    result: Option<T>,
    error: Option<ResponseError>,
}

#[derive(Deserialize, Debug)]
struct ResponseError {
    code: i64,
    message: String,
}

/// LSP Client that manages communication with seq-lsp
pub struct LspClient {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: AtomicI64,
    document_version: i32,
    document_uri: Uri,
}

impl LspClient {
    /// Spawn seq-lsp and initialize the connection
    pub fn new(seq_file: &Path) -> Result<Self, String> {
        // Find seq-lsp binary (same directory as seqr, or in PATH)
        let lsp_path = find_seq_lsp()?;

        let mut process = Command::new(&lsp_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // Suppress stderr for now
            .spawn()
            .map_err(|e| format!("Failed to spawn seq-lsp: {}", e))?;

        let stdin = process.stdin.take().ok_or("Failed to get stdin")?;
        let stdout = process.stdout.take().ok_or("Failed to get stdout")?;

        // Create a file:// URI from the path
        let abs_path = seq_file
            .canonicalize()
            .map_err(|e| format!("Cannot canonicalize path {:?}: {}", seq_file, e))?;
        let uri_str = format!("file://{}", abs_path.display());
        let document_uri: Uri = uri_str
            .parse()
            .map_err(|e| format!("Invalid URI {}: {:?}", uri_str, e))?;

        let mut client = Self {
            process,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: AtomicI64::new(1),
            document_version: 0,
            document_uri,
        };

        // Initialize LSP connection
        client.initialize()?;

        Ok(client)
    }

    /// Send initialize request and wait for response
    fn initialize(&mut self) -> Result<InitializeResult, String> {
        let params = InitializeParams {
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };

        let response: InitializeResult = self.request("initialize", params)?;

        // Send initialized notification
        self.notify("initialized", json!({}))?;

        Ok(response)
    }

    /// Notify the server that a document was opened
    pub fn did_open(&mut self, content: &str) -> Result<(), String> {
        self.document_version = 1;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: self.document_uri.clone(),
                language_id: "seq".to_string(),
                version: self.document_version,
                text: content.to_string(),
            },
        };

        self.notify("textDocument/didOpen", params)
    }

    /// Notify the server that document content changed
    pub fn did_change(&mut self, content: &str) -> Result<(), String> {
        self.document_version += 1;

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: self.document_uri.clone(),
                version: self.document_version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None, // Full document sync
                range_length: None,
                text: content.to_string(),
            }],
        };

        self.notify("textDocument/didChange", params)
    }

    /// Request completions at a position
    pub fn completions(
        &mut self,
        line: u32,
        character: u32,
    ) -> Result<Vec<CompletionItem>, String> {
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: self.document_uri.clone(),
                },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        };

        let response: Option<CompletionResponse> =
            self.request("textDocument/completion", params)?;

        match response {
            Some(CompletionResponse::Array(items)) => Ok(items),
            Some(CompletionResponse::List(list)) => Ok(list.items),
            None => Ok(vec![]),
        }
    }

    /// Send a request and wait for response
    fn request<P: Serialize, R: for<'de> Deserialize<'de>>(
        &mut self,
        method: &'static str,
        params: P,
    ) -> Result<R, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = Request {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };

        self.send_message(&request)?;
        self.read_response(id)
    }

    /// Send a notification (no response expected)
    fn notify<P: Serialize>(&mut self, method: &'static str, params: P) -> Result<(), String> {
        let notification = Notification {
            jsonrpc: "2.0",
            method,
            params,
        };

        self.send_message(&notification)
    }

    /// Send a JSON-RPC message with Content-Length header
    fn send_message<T: Serialize>(&mut self, message: &T) -> Result<(), String> {
        let content = serde_json::to_string(message)
            .map_err(|e| format!("JSON serialization error: {}", e))?;

        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        self.stdin
            .write_all(header.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        self.stdin
            .write_all(content.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        self.stdin
            .flush()
            .map_err(|e| format!("Flush error: {}", e))?;

        Ok(())
    }

    /// Read a response with the given ID
    ///
    /// Skips notifications and other messages until we find the expected response.
    /// Gives up after MAX_SKIPPED_MESSAGES to prevent infinite loops.
    fn read_response<R: for<'de> Deserialize<'de>>(
        &mut self,
        expected_id: i64,
    ) -> Result<R, String> {
        const MAX_SKIPPED_MESSAGES: usize = 100;
        let mut skipped = 0;

        loop {
            let content = self.read_message()?;

            let response: Response<R> = serde_json::from_str(&content)
                .map_err(|e| format!("JSON parse error: {} in: {}", e, content))?;

            // Check if this is our response
            if let Some(id) = response.id
                && id == expected_id
            {
                if let Some(error) = response.error {
                    return Err(format!("LSP error {}: {}", error.code, error.message));
                }
                return response
                    .result
                    .ok_or_else(|| "Missing result in response".to_string());
            }

            // Otherwise it's a notification or different request - skip it
            skipped += 1;
            if skipped >= MAX_SKIPPED_MESSAGES {
                return Err(format!(
                    "LSP response not found after {} messages",
                    MAX_SKIPPED_MESSAGES
                ));
            }
        }
    }

    /// Read a single JSON-RPC message
    fn read_message(&mut self) -> Result<String, String> {
        // Read headers until empty line
        let mut content_length: Option<usize> = None;

        loop {
            let mut line = String::new();
            self.stdout
                .read_line(&mut line)
                .map_err(|e| format!("Read error: {}", e))?;

            let line = line.trim();
            if line.is_empty() {
                break;
            }

            if let Some(len_str) = line.strip_prefix("Content-Length: ") {
                content_length = Some(
                    len_str
                        .parse()
                        .map_err(|e| format!("Invalid Content-Length: {}", e))?,
                );
            }
        }

        let length = content_length.ok_or("Missing Content-Length header")?;

        // Read exactly `length` bytes
        let mut content = vec![0u8; length];
        self.stdout
            .read_exact(&mut content)
            .map_err(|e| format!("Read error: {}", e))?;

        String::from_utf8(content).map_err(|e| format!("UTF-8 error: {}", e))
    }

    /// Shutdown the LSP server gracefully
    pub fn shutdown(&mut self) -> Result<(), String> {
        // Send shutdown request
        let _: Value = self.request("shutdown", json!({}))?;

        // Send exit notification
        self.notify("exit", json!({}))?;

        // Wait for process to exit
        let _ = self.process.wait();

        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Attempt graceful shutdown (send shutdown request + exit notification)
        let _ = self.shutdown();

        // Check if process exited, otherwise force kill immediately
        // We avoid blocking in Drop - if graceful shutdown didn't work, just kill it
        match self.process.try_wait() {
            Ok(Some(_)) => (), // Process exited cleanly
            _ => {
                // Still running or error - force kill and reap
                let _ = self.process.kill();
                let _ = self.process.wait();
            }
        }
    }
}

/// Find the seq-lsp binary
fn find_seq_lsp() -> Result<String, String> {
    // First try same directory as current executable
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let lsp_path = dir.join("seq-lsp");
        if lsp_path.exists() {
            return Ok(lsp_path.to_string_lossy().to_string());
        }
    }

    // Then try PATH
    if Command::new("seq-lsp")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
    {
        return Ok("seq-lsp".to_string());
    }

    // Finally try target/release (for development)
    let dev_path = "target/release/seq-lsp";
    if std::path::Path::new(dev_path).exists() {
        return Ok(dev_path.to_string());
    }

    Err("seq-lsp not found. Install with: cargo install --path crates/lsp".to_string())
}
