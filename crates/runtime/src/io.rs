//! I/O Operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//!
//! # Safety Contract
//!
//! **IMPORTANT:** These functions are designed to be called ONLY by compiler-generated code,
//! not by end users or arbitrary C code. The compiler is responsible for:
//!
//! - Ensuring stack has correct types (verified by type checker)
//! - Passing valid, null-terminated C strings to `push_string`
//! - Never calling these functions directly from user code
//!
//! # String Handling
//!
//! String literals from the compiler must be valid UTF-8 C strings (null-terminated).
//! Currently, each string literal is allocated as an owned `String`. See
//! `docs/STRING_INTERNING_DESIGN.md` for discussion of future optimizations
//! (interning, static references, etc.).

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::ffi::CStr;
use std::io;
use std::sync::LazyLock;

/// Coroutine-aware stdout mutex.
/// Uses may::sync::Mutex which yields the coroutine when contended instead of blocking the OS thread.
/// By serializing access to stdout, we prevent RefCell borrow panics that occur when multiple
/// coroutines on the same thread try to access stdout's internal RefCell concurrently.
static STDOUT_MUTEX: LazyLock<may::sync::Mutex<()>> = LazyLock::new(|| may::sync::Mutex::new(()));

/// Valid exit code range for Unix compatibility
const EXIT_CODE_MIN: i64 = 0;
const EXIT_CODE_MAX: i64 = 255;

/// Write a string to stdout followed by a newline
///
/// Stack effect: ( str -- )
///
/// # Safety
/// Stack must have a String value on top
///
/// # Concurrency
/// Uses may::sync::Mutex to serialize stdout writes from multiple strands.
/// When the mutex is contended, the strand yields to the scheduler (doesn't block the OS thread).
/// This prevents RefCell borrow panics when multiple strands write concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_write_line(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "write_line: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            // Acquire coroutine-aware mutex (yields if contended, doesn't block)
            // This serializes access to stdout
            let _guard = STDOUT_MUTEX.lock().unwrap();

            // Write directly to fd 1 using libc to avoid Rust's std::io::stdout() RefCell.
            // Rust's standard I/O uses RefCell which panics on concurrent access from
            // multiple coroutines on the same thread.
            // Byte-clean: write the underlying bytes directly to fd 1.
            // libc::write takes a raw pointer + length, so we don't
            // need a `&str`. Binary response bodies, ANSI escapes,
            // arbitrary protocol output all flow through unchanged.
            let bytes = s.as_bytes();
            let newline = b"\n";
            unsafe {
                libc::write(1, bytes.as_ptr() as *const libc::c_void, bytes.len());
                libc::write(1, newline.as_ptr() as *const libc::c_void, newline.len());
            }

            rest
        }
        _ => panic!("write_line: expected String on stack, got {:?}", value),
    }
}

/// Write a string to stdout without a trailing newline
///
/// Stack effect: ( str -- )
///
/// This is useful for protocols like LSP that require exact byte output
/// without trailing newlines.
///
/// # Safety
/// Stack must have a String value on top
///
/// # Concurrency
/// Uses may::sync::Mutex to serialize stdout writes from multiple strands.
/// When the mutex is contended, the strand yields to the scheduler (doesn't block the OS thread).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_write(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "write: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            let _guard = STDOUT_MUTEX.lock().unwrap();

            // Byte-clean: write the underlying bytes directly to fd 1.
            let bytes = s.as_bytes();
            unsafe {
                libc::write(1, bytes.as_ptr() as *const libc::c_void, bytes.len());
            }

            rest
        }
        _ => panic!("write: expected String on stack, got {:?}", value),
    }
}

/// Read a line from stdin
///
/// Returns the line and a success flag:
/// - ( line true ) on success (line includes trailing newline)
/// - ( "" false ) on I/O error or EOF
///
/// Use `string.chomp` to remove trailing newlines if needed.
///
/// # Line Ending Normalization
///
/// Line endings are normalized to `\n` regardless of platform. Windows-style
/// `\r\n` endings are converted to `\n`. This ensures consistent behavior
/// across different operating systems.
///
/// Stack effect: ( -- String Bool )
///
/// Errors are values, not crashes.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_read_line(stack: Stack) -> Stack {
    use std::io::BufRead;

    let stdin = io::stdin();
    let mut line = String::new();

    match stdin.lock().read_line(&mut line) {
        Ok(0) => {
            // EOF - return empty string and false
            let stack = unsafe { push(stack, Value::String("".to_string().into())) };
            unsafe { push(stack, Value::Bool(false)) }
        }
        Ok(_) => {
            // Normalize line endings: \r\n -> \n
            if line.ends_with("\r\n") {
                line.pop(); // remove \n
                line.pop(); // remove \r
                line.push('\n'); // add back \n
            }
            let stack = unsafe { push(stack, Value::String(line.into())) };
            unsafe { push(stack, Value::Bool(true)) }
        }
        Err(_) => {
            // I/O error - return empty string and false
            let stack = unsafe { push(stack, Value::String("".to_string().into())) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

/// Maximum bytes allowed for a single read_n call (10MB)
/// This prevents accidental or malicious massive memory allocations.
/// LSP messages are typically < 1MB, so 10MB provides generous headroom.
const READ_N_MAX_BYTES: i64 = 10 * 1024 * 1024;

/// Validates and extracts the byte count from a Value for read_n.
/// Returns Ok(usize) on success, Err(message) on validation failure.
fn validate_read_n_count(value: &Value) -> Result<usize, String> {
    match value {
        Value::Int(n) if *n < 0 => Err(format!(
            "read_n: byte count must be non-negative, got {}",
            n
        )),
        Value::Int(n) if *n > READ_N_MAX_BYTES => Err(format!(
            "read_n: byte count {} exceeds maximum allowed ({})",
            n, READ_N_MAX_BYTES
        )),
        Value::Int(n) => Ok(*n as usize),
        _ => Err(format!("read_n: expected Int on stack, got {:?}", value)),
    }
}

/// Read exactly N bytes from stdin
///
/// Returns the bytes read and a status flag:
/// - ( string 1 ) on success (read all N bytes)
/// - ( string 0 ) at EOF, partial read, or error (string may be shorter than N)
///
/// Stack effect: ( Int -- String Int )
///
/// Like `io.read-line`, this returns a result pattern (value + status) to allow
/// explicit EOF detection. The function name omits the `+` suffix for brevity
/// since byte-count reads are inherently status-oriented.
///
/// Errors are values, not crashes.
///
/// This is used for protocols like LSP where message bodies are byte-counted
/// and don't have trailing newlines.
///
/// # UTF-8 Handling
/// The bytes are interpreted as UTF-8. Invalid UTF-8 sequences are replaced
/// with the Unicode replacement character (U+FFFD). This is appropriate for
/// text-based protocols like LSP but may not be suitable for binary data.
///
/// # Safety
/// Stack must have an Int on top. The integer must be non-negative and
/// not exceed READ_N_MAX_BYTES (10MB).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_read_n(stack: Stack) -> Stack {
    use std::io::Read;

    assert!(!stack.is_null(), "read_n: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    // Validate input - return error status for invalid input
    let n = match validate_read_n_count(&value) {
        Ok(n) => n,
        Err(_) => {
            // Invalid input - return empty string and error status
            let stack = unsafe { push(stack, Value::String("".to_string().into())) };
            return unsafe { push(stack, Value::Int(0)) };
        }
    };

    let stdin = io::stdin();
    let mut buffer = vec![0u8; n];
    let mut total_read = 0;

    {
        let mut handle = stdin.lock();
        while total_read < n {
            match handle.read(&mut buffer[total_read..]) {
                Ok(0) => break, // EOF
                Ok(bytes_read) => total_read += bytes_read,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break, // I/O error - stop reading, return what we have
            }
        }
    }

    // Truncate to actual bytes read
    buffer.truncate(total_read);

    // Convert to String (assuming UTF-8)
    let s = String::from_utf8_lossy(&buffer).into_owned();

    // Status: 1 if we read all N bytes, 0 otherwise
    let status = if total_read == n { 1i64 } else { 0i64 };

    let stack = unsafe { push(stack, Value::String(s.into())) };
    unsafe { push(stack, Value::Int(status)) }
}

/// Convert an integer to a string
///
/// Stack effect: ( Int -- String )
///
/// # Safety
/// Stack must have an Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_int_to_string(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "int_to_string: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::Int(n) => unsafe { push(rest, Value::String(n.to_string().into())) },
        _ => panic!("int_to_string: expected Int on stack, got {:?}", value),
    }
}

/// Push a C string literal onto the stack (for compiler-generated code).
///
/// Used by codegen paths whose source is always an ASCII identifier
/// (variant tag comparisons, NULL-FFI fallbacks, etc.) — they have no
/// embedded NULs, so the C-string convention is fine. Byte-clean
/// string *literals* go through `patch_seq_push_string_bytes` instead.
///
/// In debug builds, this asserts the input is ASCII to catch a future
/// codegen path that accidentally routes binary data here. In release
/// the bytes are taken as-is — the comment above is the contract.
///
/// Stack effect: ( -- str )
///
/// # Safety
/// The c_str pointer must be valid and null-terminated
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_string(stack: Stack, c_str: *const i8) -> Stack {
    assert!(!c_str.is_null(), "push_string: null string pointer");

    let bytes = unsafe { CStr::from_ptr(c_str).to_bytes() };
    debug_assert!(
        std::str::from_utf8(bytes).is_ok(),
        "push_string: input must be valid UTF-8 (variant tags, identifier-shaped \
         literals, FFI fallbacks); arbitrary binary string literals must use \
         push_string_bytes instead",
    );
    let seqstr = crate::seqstring::global_bytes(bytes.to_vec());
    unsafe { push(stack, Value::String(seqstr)) }
}

/// Push a byte-clean string literal onto the stack (for compiler-generated
/// code). Carries an explicit length so embedded NULs and arbitrary bytes
/// flow through unchanged — this is the codegen target for Seq string
/// literals after the byte-cleanliness landing.
///
/// Stack effect: ( -- str )
///
/// # Safety
/// `ptr` must point to at least `len` valid bytes. `ptr` may not be null
/// unless `len` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_string_bytes(
    stack: Stack,
    ptr: *const u8,
    len: usize,
) -> Stack {
    let bytes = if len == 0 {
        Vec::new()
    } else {
        assert!(
            !ptr.is_null(),
            "push_string_bytes: null pointer with non-zero length"
        );
        unsafe { std::slice::from_raw_parts(ptr, len).to_vec() }
    };
    let seqstr = crate::seqstring::global_bytes(bytes);
    unsafe { push(stack, Value::String(seqstr)) }
}

/// Push a C string literal onto the stack as a Symbol (for compiler-generated code)
///
/// Stack effect: ( -- symbol )
///
/// # Safety
/// The c_str pointer must be valid and null-terminated
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_symbol(stack: Stack, c_str: *const i8) -> Stack {
    assert!(!c_str.is_null(), "push_symbol: null string pointer");

    let s = unsafe {
        CStr::from_ptr(c_str)
            .to_str()
            .expect("push_symbol: invalid UTF-8 in symbol literal")
            .to_owned()
    };

    unsafe { push(stack, Value::Symbol(s.into())) }
}

/// Layout of static interned symbol data from LLVM IR
///
/// Matches the LLVM IR structure:
/// `{ ptr, i64 len, i64 capacity, i8 global }`
///
/// # Safety Contract
///
/// This struct must ONLY be constructed by the compiler in static globals.
/// Invariants that MUST hold:
/// - `ptr` points to valid static UTF-8 string data with lifetime `'static`
/// - `len` matches the actual byte length of the string
/// - `capacity` MUST be 0 (marks symbol as interned/static)
/// - `global` MUST be 1 (marks symbol as static allocation)
///
/// Violating these invariants causes undefined behavior (memory corruption,
/// double-free, or null pointer dereference).
#[repr(C)]
pub struct InternedSymbolData {
    ptr: *const u8,
    len: i64,
    capacity: i64, // MUST be 0 for interned symbols
    global: i8,    // MUST be 1 for interned symbols
}

/// Push an interned symbol onto the stack (Issue #166)
///
/// This pushes a compile-time symbol literal that shares static memory.
/// The SeqString has capacity=0 to mark it as interned (never freed).
///
/// Stack effect: ( -- Symbol )
///
/// # Safety
/// The symbol_data pointer must point to a valid static InternedSymbolData structure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_interned_symbol(
    stack: Stack,
    symbol_data: *const InternedSymbolData,
) -> Stack {
    assert!(
        !symbol_data.is_null(),
        "push_interned_symbol: null symbol data pointer"
    );

    let data = unsafe { &*symbol_data };

    // Validate interned symbol invariants - these are safety-critical
    // and must run in release builds to prevent memory corruption
    assert!(!data.ptr.is_null(), "Interned symbol data pointer is null");
    assert_eq!(data.capacity, 0, "Interned symbols must have capacity=0");
    assert_ne!(data.global, 0, "Interned symbols must have global=1");

    // Create SeqString that points to static data
    // capacity=0 marks it as interned (Drop will skip deallocation)
    // Safety: from_raw_parts requires valid ptr/len/capacity, which we trust
    // from the LLVM-generated static data
    let seq_str = unsafe {
        crate::seqstring::SeqString::from_raw_parts(
            data.ptr,
            data.len as usize,
            data.capacity as usize, // 0 for interned
            data.global != 0,       // true for interned
        )
    };

    unsafe { push(stack, Value::Symbol(seq_str)) }
}

/// Push a SeqString value onto the stack
///
/// This is used when we already have a SeqString (e.g., from closures).
/// Unlike push_string which takes a C string, this takes a SeqString by value.
///
/// Stack effect: ( -- String )
///
/// # Safety
/// The SeqString must be valid. This is only called from LLVM-generated code, not actual C code.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_seqstring(
    stack: Stack,
    seq_str: crate::seqstring::SeqString,
) -> Stack {
    unsafe { push(stack, Value::String(seq_str)) }
}

/// Convert a Symbol to a String
///
/// Stack effect: ( Symbol -- String )
///
/// # Safety
/// Stack must have a Symbol on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_symbol_to_string(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "symbol_to_string: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::Symbol(s) => unsafe { push(rest, Value::String(s)) },
        _ => panic!(
            "symbol_to_string: expected Symbol on stack, got {:?}",
            value
        ),
    }
}

/// Convert a String to a Symbol
///
/// Stack effect: ( String -- Symbol )
///
/// # Safety
/// Stack must have a String on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_to_symbol(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_to_symbol: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => unsafe { push(rest, Value::Symbol(s)) },
        _ => panic!(
            "string_to_symbol: expected String on stack, got {:?}",
            value
        ),
    }
}

/// Exit the program with a status code
///
/// Stack effect: ( exit_code -- )
///
/// # Safety
/// Stack must have an Int on top. Never returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_exit_op(stack: Stack) -> ! {
    assert!(!stack.is_null(), "exit_op: stack is empty");

    let (_rest, value) = unsafe { pop(stack) };

    match value {
        Value::Int(code) => {
            // Explicitly validate exit code is in Unix-compatible range
            if !(EXIT_CODE_MIN..=EXIT_CODE_MAX).contains(&code) {
                panic!(
                    "exit_op: exit code must be in range {}-{}, got {}",
                    EXIT_CODE_MIN, EXIT_CODE_MAX, code
                );
            }
            std::process::exit(code as i32);
        }
        _ => panic!("exit_op: expected Int on stack, got {:?}", value),
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_exit_op as exit_op;
pub use patch_seq_int_to_string as int_to_string;
pub use patch_seq_push_interned_symbol as push_interned_symbol;
pub use patch_seq_push_seqstring as push_seqstring;
pub use patch_seq_push_string as push_string;
pub use patch_seq_push_symbol as push_symbol;
pub use patch_seq_read_line as read_line;
pub use patch_seq_read_n as read_n;
pub use patch_seq_string_to_symbol as string_to_symbol;
pub use patch_seq_symbol_to_string as symbol_to_string;
pub use patch_seq_write as write;
pub use patch_seq_write_line as write_line;

#[cfg(test)]
mod tests;
