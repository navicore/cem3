//! SeqString - Arena or Globally Allocated String
//!
//! Strings in Seq can be allocated from two sources:
//! 1. Thread-local arena (fast, bulk-freed on strand exit)
//! 2. Global allocator (persists across arena resets)
//!
//! This allows fast temporary string creation during strand execution
//! while maintaining safety for channel communication (clone to global).

use crate::arena;
use std::fmt;

/// String that tracks its allocation source
///
/// # Safety Invariants
/// - If global=true: ptr points to global-allocated String, must be dropped
/// - If global=false: ptr points to thread-local arena, no drop needed
/// - ptr + len must form a valid UTF-8 string
/// - For global strings: capacity must match the original String's capacity
pub struct SeqString {
    ptr: *const u8,
    len: usize,
    capacity: usize, // Only meaningful for global strings
    global: bool,
}

// Implement PartialEq manually to compare string content, not pointers
impl PartialEq for SeqString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for SeqString {}

// Safety: SeqString is Send because:
// - Global strings are truly independent (owned heap allocation)
// - Arena strings are cloned to global on channel send (see Clone impl)
// - We never send arena pointers across threads unsafely
unsafe impl Send for SeqString {}

// Safety: SeqString is Sync because:
// - The string content is immutable after construction
// - ptr/len are only read, never modified after construction
// - Global strings (Arc<String>) are already Sync
// - Arena strings point to memory that won't be deallocated while in use
unsafe impl Sync for SeqString {}

impl SeqString {
    /// Get string slice
    ///
    /// # Safety
    /// ptr + len must point to valid UTF-8. This is guaranteed by constructors.
    pub fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) }
    }

    /// Check if this string is globally allocated
    #[allow(dead_code)]
    pub fn is_global(&self) -> bool {
        self.global
    }

    /// Get length in bytes
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Check if this is an interned/static string (Issue #166)
    ///
    /// Interned strings have capacity=0 and point to static data.
    /// They are never freed and can be compared by pointer for O(1) equality.
    pub fn is_interned(&self) -> bool {
        self.capacity == 0 && self.global
    }

    /// Get raw pointer to string data
    ///
    /// Used for O(1) pointer comparison of interned symbols.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Consume self and return raw parts for storage in StackValue
    ///
    /// Returns (ptr, len, capacity, global)
    ///
    /// # Safety
    /// The caller must either reconstruct using `from_raw_parts` or
    /// properly handle drop (for global strings only).
    pub fn into_raw_parts(self) -> (*const u8, usize, usize, bool) {
        let parts = (self.ptr, self.len, self.capacity, self.global);
        std::mem::forget(self); // Don't run Drop
        parts
    }

    /// Reconstruct SeqString from raw parts
    ///
    /// # Safety
    /// The parts must have come from `into_raw_parts` on a valid SeqString,
    /// or be a new valid allocation matching the ptr/len/capacity/global invariants.
    pub unsafe fn from_raw_parts(
        ptr: *const u8,
        len: usize,
        capacity: usize,
        global: bool,
    ) -> Self {
        SeqString {
            ptr,
            len,
            capacity,
            global,
        }
    }
}

impl Clone for SeqString {
    /// Clone always allocates from global allocator for Send safety
    ///
    /// This ensures that when a String is sent through a channel,
    /// the receiving strand gets an independent copy that doesn't
    /// depend on the sender's arena.
    fn clone(&self) -> Self {
        let s = self.as_str().to_string();
        global_string(s)
    }
}

impl Drop for SeqString {
    fn drop(&mut self) {
        if self.global && self.capacity > 0 {
            // Reconstruct String and drop it
            // Safety: We created this from String in global_string() and stored
            // the original ptr, len, and capacity. This ensures correct deallocation.
            //
            // Note: capacity=0 marks interned/static symbols (Issue #166) - these
            // point to static data and must NOT be deallocated.
            unsafe {
                let _s = String::from_raw_parts(
                    self.ptr as *mut u8,
                    self.len,
                    self.capacity, // Use original capacity for correct deallocation
                );
                // _s is dropped here, freeing the memory with correct size
            }
        }
        // Arena strings don't need explicit drop - arena reset frees them
        // Static/interned strings (capacity=0) point to static data - no drop needed
    }
}

impl fmt::Debug for SeqString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SeqString({:?}, global={})", self.as_str(), self.global)
    }
}

impl fmt::Display for SeqString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Create arena-allocated string (fast path for temporaries)
///
/// # Performance
/// ~5ns vs ~100ns for global allocator (20x faster)
///
/// # Lifetime
/// Valid until arena_reset() is called (typically when strand exits)
pub fn arena_string(s: &str) -> SeqString {
    arena::with_arena(|arena| {
        let arena_str = arena.alloc_str(s);
        SeqString {
            ptr: arena_str.as_ptr(),
            len: arena_str.len(),
            capacity: 0, // Not used for arena strings
            global: false,
        }
    })
}

/// Create globally-allocated string (persists across arena resets)
///
/// # Usage
/// For strings that need to outlive the current strand, or be sent through channels.
///
/// # Performance
/// Same as regular String allocation
pub fn global_string(s: String) -> SeqString {
    let len = s.len();
    let capacity = s.capacity();
    let ptr = s.as_ptr();
    std::mem::forget(s); // Transfer ownership, don't drop

    SeqString {
        ptr,
        len,
        capacity, // Store original capacity for correct deallocation
        global: true,
    }
}

/// Convert &str to SeqString using arena allocation
impl From<&str> for SeqString {
    fn from(s: &str) -> Self {
        arena_string(s)
    }
}

/// Convert String to SeqString using global allocation
impl From<String> for SeqString {
    fn from(s: String) -> Self {
        global_string(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_string() {
        let s = arena_string("Hello, arena!");
        assert_eq!(s.as_str(), "Hello, arena!");
        assert_eq!(s.len(), 13);
        assert!(!s.is_global());
    }

    #[test]
    fn test_global_string() {
        let s = global_string("Hello, global!".to_string());
        assert_eq!(s.as_str(), "Hello, global!");
        assert_eq!(s.len(), 14);
        assert!(s.is_global());
    }

    #[test]
    fn test_clone_creates_global() {
        // Clone an arena string
        let s1 = arena_string("test");
        let s2 = s1.clone();

        assert_eq!(s1.as_str(), s2.as_str());
        assert!(!s1.is_global());
        assert!(s2.is_global()); // Clone is always global!
    }

    #[test]
    fn test_clone_global() {
        let s1 = global_string("test".to_string());
        let s2 = s1.clone();

        assert_eq!(s1.as_str(), s2.as_str());
        assert!(s1.is_global());
        assert!(s2.is_global());
    }

    #[test]
    fn test_drop_global() {
        // Create and drop a global string
        {
            let s = global_string("Will be dropped".to_string());
            assert_eq!(s.as_str(), "Will be dropped");
        }
        // If we get here without crashing, drop worked
    }

    #[test]
    fn test_drop_arena() {
        // Create and drop an arena string
        {
            let s = arena_string("Will be dropped (no-op)");
            assert_eq!(s.as_str(), "Will be dropped (no-op)");
        }
        // Arena strings don't need explicit drop
    }

    #[test]
    fn test_equality() {
        let s1 = arena_string("test");
        let s2 = arena_string("test");
        let s3 = global_string("test".to_string());
        let s4 = arena_string("different");

        assert_eq!(s1, s2); // Same content, both arena
        assert_eq!(s1, s3); // Same content, different allocation
        assert_ne!(s1, s4); // Different content
    }

    #[test]
    fn test_from_str() {
        let s: SeqString = "test".into();
        assert_eq!(s.as_str(), "test");
        assert!(!s.is_global()); // from &str uses arena
    }

    #[test]
    fn test_from_string() {
        let s: SeqString = "test".to_string().into();
        assert_eq!(s.as_str(), "test");
        assert!(s.is_global()); // from String uses global
    }

    #[test]
    fn test_debug_format() {
        let s = arena_string("debug");
        let debug_str = format!("{:?}", s);
        assert!(debug_str.contains("debug"));
        assert!(debug_str.contains("global=false"));
    }

    #[test]
    fn test_display_format() {
        let s = global_string("display".to_string());
        let display_str = format!("{}", s);
        assert_eq!(display_str, "display");
    }

    #[test]
    fn test_empty_string() {
        let s = arena_string("");
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert_eq!(s.as_str(), "");
    }

    #[test]
    fn test_unicode() {
        let s = arena_string("Hello, 世界! 🦀");
        assert_eq!(s.as_str(), "Hello, 世界! 🦀");
        assert!(s.len() > 10); // UTF-8 bytes, not chars
    }

    #[test]
    fn test_global_string_preserves_capacity() {
        // PR #11 Critical fix: Verify capacity is preserved for correct deallocation
        let mut s = String::with_capacity(100);
        s.push_str("hi");

        assert_eq!(s.len(), 2);
        assert_eq!(s.capacity(), 100);

        let cem = global_string(s);

        // Verify the SeqString captured the original capacity
        assert_eq!(cem.len(), 2);
        assert_eq!(cem.capacity, 100); // Critical: Must be 100, not 2!
        assert_eq!(cem.as_str(), "hi");
        assert!(cem.is_global());

        // Drop cem - if capacity was wrong, this would cause heap corruption
        drop(cem);

        // If we get here without crash/UB, the fix worked
    }

    #[test]
    fn test_arena_string_capacity_zero() {
        // Arena strings don't use capacity field
        let s = arena_string("test");
        assert_eq!(s.capacity, 0); // Arena strings have capacity=0
        assert!(!s.is_global());
    }
}
