//! Command-history store.
//!
//! A pure in-memory ring of past command strings with `prev`/`next` cursor,
//! consecutive-duplicate dedup, substring search, and a draft stash that
//! preserves the in-progress line while the user browses.
//!
//! ## Layering
//!
//! This module is the *store* half of "command history" — the data structure.
//! The *keymap* half lives in [`crate::vim`]: normal-mode `k`/`j` at a line
//! boundary emit `Action::HistoryPrev`/`Next`; arrows do the same in every
//! mode. The host wires those actions into `store.prev(...)` / `store.next()`
//! and applies the returned string via its own buffer.
//!
//! The store knows nothing about the editor: it neither owns nor mutates the
//! caller's text, and it never calls back into `vim-line`. The host is the
//! sole bridge.
//!
//! ## Persistence
//!
//! The store does no I/O. Hosts that persist history (e.g. seqr writing
//! `~/.local/share/seqr_history`) `load(...)` at startup and read
//! `entries()` at shutdown — the path and file format are entirely the
//! host's concern.

use std::collections::VecDeque;

/// Default ring capacity when none is specified.
const DEFAULT_MAX_SIZE: usize = 1000;

/// The result of stepping the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recall<'a> {
    /// A stored entry to display.
    Entry(&'a str),
    /// The user has stepped back past the newest entry; display the
    /// previously stashed draft (and the store is no longer browsing).
    Draft(&'a str),
}

/// A ring of past command strings with `prev`/`next` browsing and a draft
/// stash. See the module docs for the layering contract with the editor.
#[derive(Debug, Clone)]
pub struct Store {
    entries: VecDeque<String>,
    max_size: usize,
    /// `None` when not browsing; `Some(i)` while positioned at `entries[i]`.
    cursor: Option<usize>,
    /// In-progress text stashed on the first `prev` call, restored when
    /// `next` walks back past the newest entry.
    draft: String,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    /// Create an empty store with the default capacity (1000 entries).
    pub fn new() -> Self {
        Self::with_max_size(DEFAULT_MAX_SIZE)
    }

    /// Create an empty store with an explicit capacity. A capacity of zero
    /// is treated as "unbounded" — `push` will never evict.
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_size,
            cursor: None,
            draft: String::new(),
        }
    }

    /// Replace the contents of the store with `entries`, oldest first.
    /// Resets any in-progress browsing state. Used by hosts to seed the
    /// store from a persistence file.
    pub fn load<I, S>(&mut self, entries: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.entries.clear();
        self.cursor = None;
        self.draft.clear();
        for entry in entries {
            self.push_raw(entry.into());
        }
    }

    /// Iterate over the entries, oldest first. For hosts that persist history
    /// at shutdown.
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when there are no stored entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get an entry by index (oldest = 0). Returns `None` if out of range.
    pub fn at(&self, idx: usize) -> Option<&str> {
        self.entries.get(idx).map(String::as_str)
    }

    /// Add `entry` to the ring, evicting the oldest if capacity is exceeded.
    /// Empty strings are ignored. Consecutive duplicates are collapsed so
    /// repeated identical commands don't bloat the ring. Also resets the
    /// browsing cursor and clears any stashed draft — submitting commits.
    pub fn push(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        self.push_raw(entry);
        self.cursor = None;
        self.draft.clear();
    }

    fn push_raw(&mut self, entry: String) {
        if entry.is_empty() {
            return;
        }
        if self.entries.back().is_some_and(|last| last == &entry) {
            return;
        }
        self.entries.push_back(entry);
        if self.max_size > 0 {
            while self.entries.len() > self.max_size {
                self.entries.pop_front();
            }
        }
    }

    /// Step to the previous (older) entry. On the first call of a browsing
    /// session, stashes `current` as the draft so it can be restored by a
    /// later `next`. Returns `None` when there are no entries or the cursor
    /// is already at the oldest.
    pub fn prev(&mut self, current: &str) -> Option<Recall<'_>> {
        if self.entries.is_empty() {
            return None;
        }
        match self.cursor {
            None => {
                self.draft = current.to_string();
                let idx = self.entries.len() - 1;
                self.cursor = Some(idx);
                Some(Recall::Entry(&self.entries[idx]))
            }
            Some(0) => None,
            Some(idx) => {
                let new_idx = idx - 1;
                self.cursor = Some(new_idx);
                Some(Recall::Entry(&self.entries[new_idx]))
            }
        }
    }

    /// Step to the next (newer) entry. Returns:
    /// - `Some(Recall::Entry(_))` while still browsing newer entries
    /// - `Some(Recall::Draft(_))` when stepping past the newest, restoring
    ///   the stashed in-progress line (the store stops browsing)
    /// - `None` when not currently browsing (caller does nothing)
    ///
    /// `Store` is a bidirectional cursor, not a forward iterator — the
    /// `prev`/`next` pair is the readline-vi-mode vocabulary callers
    /// already know, so we opt out of clippy's "implement Iterator
    /// instead" suggestion. Implementing `Iterator` would lose the
    /// browsing-state semantics that make `next()` a no-op when no
    /// draft is stashed.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Recall<'_>> {
        let idx = self.cursor?;
        let last = self.entries.len().saturating_sub(1);
        if idx < last {
            let new_idx = idx + 1;
            self.cursor = Some(new_idx);
            Some(Recall::Entry(&self.entries[new_idx]))
        } else {
            self.cursor = None;
            Some(Recall::Draft(&self.draft))
        }
    }

    /// Abandon the current browsing session, restoring the draft. Returns
    /// the stashed draft (empty when not browsing). Hosts call this when
    /// the user cancels recall without selecting an entry.
    pub fn cancel_recall(&mut self) -> String {
        self.cursor = None;
        std::mem::take(&mut self.draft)
    }

    /// True when the user is browsing past entries (a draft is stashed).
    pub fn is_browsing(&self) -> bool {
        self.cursor.is_some()
    }

    /// Search for entries containing `query` (case-insensitive substring
    /// match). Returns indices into the store, most-recent first, so callers
    /// can step through matches in the natural display order. An empty
    /// `query` returns no matches.
    pub fn search(&self, query: &str) -> Vec<usize> {
        if query.is_empty() {
            return Vec::new();
        }
        let needle = query.to_lowercase();
        let mut hits = Vec::new();
        for (i, entry) in self.entries.iter().enumerate().rev() {
            if entry.to_lowercase().contains(&needle) {
                hits.push(i);
            }
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_collapses_consecutive_duplicates() {
        let mut s = Store::new();
        s.push("a");
        s.push("a");
        s.push("b");
        s.push("a");
        let xs: Vec<&str> = s.entries().collect();
        assert_eq!(xs, vec!["a", "b", "a"]);
    }

    #[test]
    fn push_ignores_empty() {
        let mut s = Store::new();
        s.push("");
        assert!(s.is_empty());
    }

    #[test]
    fn push_respects_max_size() {
        let mut s = Store::with_max_size(2);
        s.push("a");
        s.push("b");
        s.push("c");
        let xs: Vec<&str> = s.entries().collect();
        assert_eq!(xs, vec!["b", "c"]);
    }

    #[test]
    fn max_size_zero_means_unbounded() {
        let mut s = Store::with_max_size(0);
        for i in 0..50 {
            s.push(format!("e{i}"));
        }
        assert_eq!(s.len(), 50);
    }

    #[test]
    fn prev_stashes_draft_and_walks_back() {
        let mut s = Store::new();
        s.push("one");
        s.push("two");
        s.push("three");

        assert_eq!(s.prev("draft").unwrap(), Recall::Entry("three"));
        assert_eq!(s.prev("ignored").unwrap(), Recall::Entry("two"));
        assert_eq!(s.prev("ignored").unwrap(), Recall::Entry("one"));
        assert!(s.prev("ignored").is_none(), "stops at oldest");
    }

    #[test]
    fn next_restores_draft_when_past_newest() {
        let mut s = Store::new();
        s.push("a");
        s.push("b");

        let _ = s.prev("my draft");
        let _ = s.prev("ignored");
        // At "a"; step forward
        assert_eq!(s.next().unwrap(), Recall::Entry("b"));
        assert_eq!(s.next().unwrap(), Recall::Draft("my draft"));
        assert!(s.next().is_none(), "no-op when not browsing");
        assert!(!s.is_browsing());
    }

    #[test]
    fn prev_on_empty_returns_none() {
        let mut s = Store::new();
        assert!(s.prev("anything").is_none());
        assert!(s.next().is_none());
    }

    #[test]
    fn push_during_recall_resets_state() {
        let mut s = Store::new();
        s.push("a");
        let _ = s.prev("draft");
        assert!(s.is_browsing());
        s.push("b");
        assert!(!s.is_browsing());
    }

    #[test]
    fn cancel_recall_returns_draft() {
        let mut s = Store::new();
        s.push("a");
        let _ = s.prev("typed this");
        assert_eq!(s.cancel_recall(), "typed this");
        assert!(!s.is_browsing());
    }

    #[test]
    fn search_returns_indices_newest_first() {
        let mut s = Store::new();
        s.push("alpha");
        s.push("beta");
        s.push("alpha gamma");
        s.push("delta");

        let hits = s.search("alpha");
        assert_eq!(hits, vec![2, 0]);
        assert_eq!(s.at(hits[0]), Some("alpha gamma"));
        assert_eq!(s.at(hits[1]), Some("alpha"));
    }

    #[test]
    fn search_is_case_insensitive_and_substring() {
        let mut s = Store::new();
        s.push("Hello World");
        let hits = s.search("WORLD");
        assert_eq!(hits, vec![0]);
    }

    #[test]
    fn search_empty_query_returns_nothing() {
        let mut s = Store::new();
        s.push("anything");
        assert!(s.search("").is_empty());
    }

    #[test]
    fn load_replaces_entries_and_resets_state() {
        let mut s = Store::new();
        s.push("old");
        let _ = s.prev("draft");
        s.load(["a", "b", "c"]);
        let xs: Vec<&str> = s.entries().collect();
        assert_eq!(xs, vec!["a", "b", "c"]);
        assert!(!s.is_browsing());
    }
}
