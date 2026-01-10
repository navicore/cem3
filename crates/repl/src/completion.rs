//! Completion management for the REPL (Issue #209: extracted from App).
//!
//! Handles LSP completions and builtin completions for tab completion.

use crate::lsp_client::LspClient;
use lsp_types::CompletionItem;
use std::fs;
use std::path::Path;

/// Manages tab completion state and LSP communication.
pub struct CompletionManager {
    /// LSP client for completions (None if unavailable)
    lsp_client: Option<LspClient>,
    /// Current completion items
    items: Vec<CompletionItem>,
    /// Selected completion index
    index: usize,
    /// Whether completion popup is visible
    visible: bool,
}

impl CompletionManager {
    /// Create a new completion manager without LSP.
    pub fn new() -> Self {
        Self {
            lsp_client: None,
            items: Vec::new(),
            index: 0,
            visible: false,
        }
    }

    /// Create a new completion manager with LSP client.
    pub fn with_lsp(lsp_client: LspClient) -> Self {
        Self {
            lsp_client: Some(lsp_client),
            items: Vec::new(),
            index: 0,
            visible: false,
        }
    }

    /// Try to start LSP and return a completion manager.
    pub fn try_with_lsp(session_path: &Path, content: &str) -> Self {
        match LspClient::new(session_path) {
            Ok(mut client) => {
                // Open the document
                if client.did_open(content).is_ok() {
                    Self::with_lsp(client)
                } else {
                    Self::new()
                }
            }
            Err(_) => Self::new(),
        }
    }

    /// Check if completions are visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get current completion items.
    pub fn items(&self) -> &[CompletionItem] {
        &self.items
    }

    /// Get current selection index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Move up in completion list (wraps around).
    pub fn up(&mut self) {
        if !self.items.is_empty() {
            if self.index > 0 {
                self.index -= 1;
            } else {
                self.index = self.items.len() - 1;
            }
        }
    }

    /// Move down in completion list (wraps around).
    pub fn down(&mut self) {
        if !self.items.is_empty() {
            self.index = (self.index + 1) % self.items.len();
        }
    }

    /// Hide completion popup and clear items.
    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.index = 0;
    }

    /// Request completions for the given input and cursor position.
    ///
    /// Returns a status message if completions couldn't be provided.
    pub fn request(&mut self, input: &str, cursor: usize, session_path: &Path) -> Option<String> {
        // Find word start for replacement
        let word_start = input[..cursor]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        // Get the prefix the user has typed
        let prefix = &input[word_start..cursor];

        // Don't show completions for empty prefix - too noisy
        if prefix.is_empty() {
            return Some("Tab: type a prefix first".to_string());
        }

        // Try LSP completions first
        if self.try_lsp_completions(input, cursor, prefix, session_path) {
            return None;
        }

        // Fall back to builtin completions
        self.builtin_completions(prefix);
        None
    }

    /// Try to get completions from LSP.
    /// Returns true if LSP provided completions (even if empty).
    fn try_lsp_completions(
        &mut self,
        input: &str,
        cursor: usize,
        prefix: &str,
        session_path: &Path,
    ) -> bool {
        let Some(ref mut lsp) = self.lsp_client else {
            return false;
        };

        // Get file content
        let Ok(file_content) = fs::read_to_string(session_path) else {
            return false;
        };

        // Find where to insert (before stack.dump)
        let Some(insert_pos) = file_content.find("  stack.dump") else {
            return false;
        };

        // Create virtual document: file content + current line at end of main
        let virtual_content = format!(
            "{}  {}\n{}",
            &file_content[..insert_pos],
            input,
            &file_content[insert_pos..]
        );

        // Calculate line/column in virtual document
        let lines_before: u32 = file_content[..insert_pos].matches('\n').count() as u32;
        let line_num = lines_before; // 0-indexed
        let col_num = cursor as u32 + 2; // +2 for the "  " indent

        // Sync virtual document and get completions
        if lsp.did_change(&virtual_content).is_err() {
            return false;
        }

        let items = match lsp.completions(line_num, col_num) {
            Ok(items) => items,
            Err(_) => {
                // Restore original and fail
                let _ = lsp.did_change(&file_content);
                return false;
            }
        };

        // Restore original document
        let _ = lsp.did_change(&file_content);

        // Filter by prefix (case-insensitive)
        let prefix_lower = prefix.to_lowercase();
        self.items = items
            .into_iter()
            .filter(|item| item.label.to_lowercase().starts_with(&prefix_lower))
            .take(10)
            .collect();

        if !self.items.is_empty() {
            self.index = 0;
            self.visible = true;
        }

        true // LSP was available, even if no completions
    }

    /// Provide built-in completions when LSP is not available.
    fn builtin_completions(&mut self, prefix: &str) {
        // Get all builtin names from the compiler's canonical source
        let signatures = seqc::builtins::builtin_signatures();
        let builtins: Vec<&str> = signatures.keys().map(|s| s.as_str()).collect();

        self.items = builtins
            .iter()
            .filter(|b| b.starts_with(prefix) && **b != prefix)
            .take(10)
            .map(|s| CompletionItem {
                label: s.to_string(),
                ..Default::default()
            })
            .collect();

        if !self.items.is_empty() {
            self.index = 0;
            self.visible = true;
        }
    }

    /// Accept the current completion and return the replacement text.
    ///
    /// Returns (word_start, completion_text) if a completion is selected.
    pub fn accept(&mut self, input: &str, cursor: usize) -> Option<(usize, String)> {
        let item = self.items.get(self.index)?;

        // Find start of current word
        let word_start = input[..cursor]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        let completion = item.label.clone();
        self.hide();

        Some((word_start, completion))
    }
}

impl Default for CompletionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_navigation() {
        let mut mgr = CompletionManager::new();

        // Add some test items
        mgr.items = vec![
            CompletionItem {
                label: "dup".to_string(),
                ..Default::default()
            },
            CompletionItem {
                label: "drop".to_string(),
                ..Default::default()
            },
            CompletionItem {
                label: "swap".to_string(),
                ..Default::default()
            },
        ];
        mgr.visible = true;
        mgr.index = 0;

        // Test down navigation
        mgr.down();
        assert_eq!(mgr.index, 1);
        mgr.down();
        assert_eq!(mgr.index, 2);
        mgr.down(); // Wrap around
        assert_eq!(mgr.index, 0);

        // Test up navigation
        mgr.up(); // Wrap around
        assert_eq!(mgr.index, 2);
        mgr.up();
        assert_eq!(mgr.index, 1);
    }

    #[test]
    fn test_completion_hide() {
        let mut mgr = CompletionManager::new();
        mgr.items = vec![CompletionItem {
            label: "test".to_string(),
            ..Default::default()
        }];
        mgr.visible = true;
        mgr.index = 0;

        mgr.hide();

        assert!(!mgr.is_visible());
        assert!(mgr.items.is_empty());
        assert_eq!(mgr.index, 0);
    }

    #[test]
    fn test_builtin_completions() {
        let mut mgr = CompletionManager::new();
        mgr.builtin_completions("du");

        assert!(mgr.is_visible());
        assert!(!mgr.items.is_empty());
        // Should find "dup" at minimum
        assert!(mgr.items.iter().any(|i| i.label == "dup"));
    }
}
