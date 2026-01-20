//! Weave operations for generator/coroutine-style concurrency
//!
//! A "weave" is a strand that can yield values back to its caller and be resumed.
//! Unlike regular strands (fire-and-forget), weaves allow bidirectional communication
//! with structured yield/resume semantics.
//!
//! ## Zero-Mutex Design
//!
//! Like channels, weaves pass their communication handles directly on the stack.
//! There is NO global registry and NO mutex contention. The weave context travels
//! with the stack values.
//!
//! ## API
//!
//! - `strand.weave`: ( Quotation -- WeaveHandle ) - creates a woven strand, returns handle
//! - `strand.resume`: ( WeaveHandle a -- WeaveHandle a Bool ) - resume with value
//! - `strand.weave-cancel`: ( WeaveHandle -- ) - cancel a weave and release its resources
//! - `yield`: ( WeaveCtx a -- WeaveCtx a ) - yield a value (only valid inside weave)
//!
//! ## Architecture
//!
//! Each weave has two internal channels that travel as values:
//! - The WeaveHandle (returned to caller) contains the yield_chan for receiving
//! - The WeaveCtx (on weave's stack) contains both channels for yield to use
//!
//! Flow:
//! 1. strand.weave creates channels, spawns coroutine with WeaveCtx on stack
//! 2. The coroutine waits on resume_chan for the first resume value
//! 3. Caller calls strand.resume with WeaveHandle, sending value to resume_chan
//! 4. Coroutine wakes, receives value, runs until yield
//! 5. yield uses WeaveCtx to send/receive, returns with new resume value
//! 6. When quotation returns, WeaveCtx signals completion
//!
//! ## Resource Management
//!
//! **Best practice:** Weaves should either be resumed until completion OR explicitly
//! cancelled with `strand.weave-cancel` to cleanly release resources.
//!
//! However, dropping a WeaveHandle without doing either is safe - the program will
//! still exit normally. The un-resumed weave is "dormant" (not counted as an active
//! strand) until its first resume, so it won't block program shutdown. The dormant
//! coroutine will be cleaned up when the program exits.
//!
//! Proper cleanup options:
//!
//! **Option 1: Resume until completion**
//! ```seq
//! [ generator-body ] strand.weave  # Create weave
//! 0 strand.resume                   # Resume until...
//! if                                # ...has_more is false
//!   # process value...
//!   drop 0 strand.resume           # Keep resuming
//! else
//!   drop drop                       # Clean up when done
//! then
//! ```
//!
//! **Option 2: Explicit cancellation**
//! ```seq
//! [ generator-body ] strand.weave  # Create weave
//! 0 strand.resume                   # Get first value
//! if
//!   drop                           # We only needed the first value
//!   strand.weave-cancel            # Cancel and clean up
//! else
//!   drop drop
//! then
//! ```
//!
//! ## Implementation Notes
//!
//! Control flow (completion, cancellation) is handled via a type-safe `WeaveMessage`
//! enum rather than sentinel values. This means **any** Value can be safely yielded
//! and resumed, including edge cases like `i64::MIN`.
//!
//! ## Error Handling
//!
//! All weave functions are `extern "C"` and never panic (panicking across FFI is UB).
//!
//! - **Type mismatches** (e.g., `strand.resume` without a WeaveHandle): These indicate
//!   a compiler bug or memory corruption. The function prints an error to stderr and
//!   calls `std::process::abort()` to terminate immediately.
//!
//! - **Channel errors in `yield`**: If channels close unexpectedly while a coroutine
//!   is yielding, the coroutine cleans up and blocks forever. The main program can
//!   still terminate normally since the strand is marked as completed.
//!
//! - **Channel errors in `resume`**: Returns `(handle, placeholder, false)` to indicate
//!   the weave has completed or failed. The caller should check the Bool result.

use crate::stack::{Stack, pop, push};
use crate::tagged_stack::StackValue;
use crate::value::{Value, WeaveChannelData, WeaveMessage};
use may::sync::mpmc;
use std::sync::Arc;

/// Create a woven strand from a quotation
///
/// Stack effect: ( Quotation -- WeaveHandle )
///
/// Creates a weave from the quotation. The weave is initially suspended,
/// waiting to be resumed with the first value. The quotation will receive
/// a WeaveCtx on its stack that it must pass to yield operations.
///
/// Returns a WeaveHandle that the caller uses with strand.resume.
///
/// # Error Handling
///
/// This function never panics (panicking in extern "C" is UB). On fatal error
/// (null stack, null function pointer, type mismatch), it prints an error
/// and aborts the process.
///
/// # Safety
/// Stack must have a Quotation on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_weave(stack: Stack) -> Stack {
    // Note: We can't use assert! here (it panics). Use abort() for fatal errors.
    if stack.is_null() {
        eprintln!("strand.weave: stack is null (fatal programming error)");
        std::process::abort();
    }

    // Create the two internal channels - NO registry, just Arc values
    // Uses WeaveMessage for type-safe control flow (no sentinel values)
    let (yield_sender, yield_receiver) = mpmc::channel();
    let yield_chan = Arc::new(WeaveChannelData {
        sender: yield_sender,
        receiver: yield_receiver,
    });

    let (resume_sender, resume_receiver) = mpmc::channel();
    let resume_chan = Arc::new(WeaveChannelData {
        sender: resume_sender,
        receiver: resume_receiver,
    });

    // Pop the quotation from stack
    let (stack, quot_value) = unsafe { pop(stack) };

    // Clone channels for the spawned strand's WeaveCtx
    let weave_ctx_yield = Arc::clone(&yield_chan);
    let weave_ctx_resume = Arc::clone(&resume_chan);

    // Clone for the WeaveHandle returned to caller
    let handle_yield = Arc::clone(&yield_chan);
    let handle_resume = Arc::clone(&resume_chan);

    match quot_value {
        Value::Quotation { wrapper, .. } => {
            if wrapper == 0 {
                eprintln!(
                    "strand.weave: quotation wrapper function pointer is null (compiler bug)"
                );
                std::process::abort();
            }

            use crate::scheduler::ACTIVE_STRANDS;
            use may::coroutine;
            use std::sync::atomic::Ordering;

            let fn_ptr: extern "C" fn(Stack) -> Stack = unsafe { std::mem::transmute(wrapper) };

            // Clone the stack for the child
            let (child_stack, child_base) = unsafe { crate::stack::clone_stack_with_base(stack) };

            // Convert pointers to usize (which is Send)
            let stack_addr = child_stack as usize;
            let base_addr = child_base as usize;

            // NOTE: We do NOT increment ACTIVE_STRANDS here!
            // The weave is "dormant" until first resume. This allows the scheduler
            // to exit cleanly if a weave is created but never resumed (fixes #287).
            // ACTIVE_STRANDS is incremented only after receiving the first resume.

            unsafe {
                coroutine::spawn(move || {
                    let child_stack = stack_addr as *mut StackValue;
                    let child_base = base_addr as *mut StackValue;

                    if !child_base.is_null() {
                        crate::stack::patch_seq_set_stack_base(child_base);
                    }

                    // Wait for first resume value before executing
                    // The weave is dormant at this point - not counted in ACTIVE_STRANDS
                    let first_msg = match weave_ctx_resume.receiver.recv() {
                        Ok(msg) => msg,
                        Err(_) => {
                            // Channel closed before we were resumed - just exit
                            // Don't call cleanup_strand since we never activated
                            return;
                        }
                    };

                    // Check for cancellation before starting
                    let first_value = match first_msg {
                        WeaveMessage::Cancel => {
                            // Weave was cancelled before it started - clean exit
                            // Don't call cleanup_strand since we never activated
                            crate::arena::arena_reset();
                            return;
                        }
                        WeaveMessage::Value(v) => v,
                        WeaveMessage::Done => {
                            // Shouldn't happen - Done is sent on yield_chan
                            // Don't call cleanup_strand since we never activated
                            return;
                        }
                    };

                    // NOW we're activated - increment ACTIVE_STRANDS
                    // From this point on, we must call cleanup_strand on exit
                    ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

                    // Push WeaveCtx onto stack (yield_chan, resume_chan as a pair)
                    let weave_ctx = Value::WeaveCtx {
                        yield_chan: weave_ctx_yield.clone(),
                        resume_chan: weave_ctx_resume.clone(),
                    };
                    let stack_with_ctx = push(child_stack, weave_ctx);

                    // Push the first resume value
                    let stack_with_value = push(stack_with_ctx, first_value);

                    // Execute the quotation - it receives (WeaveCtx, resume_value)
                    let final_stack = fn_ptr(stack_with_value);

                    // Quotation returned - pop WeaveCtx and signal completion
                    let (_, ctx_value) = pop(final_stack);
                    if let Value::WeaveCtx { yield_chan, .. } = ctx_value {
                        let _ = yield_chan.sender.send(WeaveMessage::Done);
                    }

                    crate::arena::arena_reset();
                    cleanup_strand();
                });
            }
        }
        Value::Closure { fn_ptr, env } => {
            if fn_ptr == 0 {
                eprintln!("strand.weave: closure function pointer is null (compiler bug)");
                std::process::abort();
            }

            use crate::scheduler::ACTIVE_STRANDS;
            use may::coroutine;
            use std::sync::atomic::Ordering;

            let fn_ref: extern "C" fn(Stack, *const Value, usize) -> Stack =
                unsafe { std::mem::transmute(fn_ptr) };
            let env_clone: Vec<Value> = env.iter().cloned().collect();

            let child_base = crate::stack::alloc_stack();
            let base_addr = child_base as usize;

            // NOTE: We do NOT increment ACTIVE_STRANDS here!
            // The weave is "dormant" until first resume. This allows the scheduler
            // to exit cleanly if a weave is created but never resumed (fixes #287).
            // ACTIVE_STRANDS is incremented only after receiving the first resume.

            unsafe {
                coroutine::spawn(move || {
                    let child_base = base_addr as *mut StackValue;
                    crate::stack::patch_seq_set_stack_base(child_base);

                    // Wait for first resume value
                    // The weave is dormant at this point - not counted in ACTIVE_STRANDS
                    let first_msg = match weave_ctx_resume.receiver.recv() {
                        Ok(msg) => msg,
                        Err(_) => {
                            // Channel closed before we were resumed - just exit
                            // Don't call cleanup_strand since we never activated
                            return;
                        }
                    };

                    // Check for cancellation before starting
                    let first_value = match first_msg {
                        WeaveMessage::Cancel => {
                            // Weave was cancelled before it started - clean exit
                            // Don't call cleanup_strand since we never activated
                            crate::arena::arena_reset();
                            return;
                        }
                        WeaveMessage::Value(v) => v,
                        WeaveMessage::Done => {
                            // Shouldn't happen - Done is sent on yield_chan
                            // Don't call cleanup_strand since we never activated
                            return;
                        }
                    };

                    // NOW we're activated - increment ACTIVE_STRANDS
                    // From this point on, we must call cleanup_strand on exit
                    ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

                    // Push WeaveCtx onto stack
                    let weave_ctx = Value::WeaveCtx {
                        yield_chan: weave_ctx_yield.clone(),
                        resume_chan: weave_ctx_resume.clone(),
                    };
                    let stack_with_ctx = push(child_base, weave_ctx);
                    let stack_with_value = push(stack_with_ctx, first_value);

                    // Execute the closure
                    let final_stack = fn_ref(stack_with_value, env_clone.as_ptr(), env_clone.len());

                    // Signal completion
                    let (_, ctx_value) = pop(final_stack);
                    if let Value::WeaveCtx { yield_chan, .. } = ctx_value {
                        let _ = yield_chan.sender.send(WeaveMessage::Done);
                    }

                    crate::arena::arena_reset();
                    cleanup_strand();
                });
            }
        }
        _ => {
            eprintln!(
                "strand.weave: expected Quotation or Closure, got {:?} (compiler bug or memory corruption)",
                quot_value
            );
            std::process::abort();
        }
    }

    // Return WeaveHandle (contains both channels for resume to use)
    let handle = Value::WeaveCtx {
        yield_chan: handle_yield,
        resume_chan: handle_resume,
    };
    unsafe { push(stack, handle) }
}

/// Block the current coroutine forever without panicking.
///
/// This is used when an unrecoverable error occurs in an extern "C" function.
/// We can't panic (UB across FFI) and we can't return (invalid state), so we
/// clean up and block forever. The coroutine is already marked as completed
/// via cleanup_strand(), so the program can still terminate normally.
///
/// # Safety
/// Must only be called from within a spawned coroutine, never from the main thread.
fn block_forever() -> ! {
    let (_, rx): (mpmc::Sender<()>, mpmc::Receiver<()>) = mpmc::channel();
    let _ = rx.recv();
    // This loop is a fallback in case recv() somehow returns (it shouldn't)
    loop {
        std::thread::park();
    }
}

/// Helper to clean up strand on exit
fn cleanup_strand() {
    use crate::scheduler::{ACTIVE_STRANDS, SHUTDOWN_CONDVAR, SHUTDOWN_MUTEX, TOTAL_COMPLETED};
    use std::sync::atomic::Ordering;

    let prev_count = ACTIVE_STRANDS.fetch_sub(1, Ordering::AcqRel);
    TOTAL_COMPLETED.fetch_add(1, Ordering::Release);

    if prev_count == 1 {
        let _guard = SHUTDOWN_MUTEX
            .lock()
            .expect("weave: shutdown mutex poisoned");
        SHUTDOWN_CONDVAR.notify_all();
    }
}

/// Resume a woven strand with a value
///
/// Stack effect: ( WeaveHandle a -- WeaveHandle a Bool )
///
/// Sends value `a` to the weave and waits for it to yield.
/// Returns (handle, yielded_value, has_more).
/// - has_more = true: weave yielded a value
/// - has_more = false: weave completed
///
/// # Error Handling
///
/// This function never panics (panicking in extern "C" is UB). On fatal error
/// (null stack, type mismatch), it prints an error and aborts the process.
///
/// # Safety
/// Stack must have a value on top and WeaveHandle below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_resume(stack: Stack) -> Stack {
    // Note: We can't use assert! here (it panics). Use abort() for fatal errors.
    if stack.is_null() {
        eprintln!("strand.resume: stack is null (fatal programming error)");
        std::process::abort();
    }

    // Pop the value to send
    let (stack, value) = unsafe { pop(stack) };

    // Pop the WeaveHandle
    let (stack, handle) = unsafe { pop(stack) };

    let (yield_chan, resume_chan) = match &handle {
        Value::WeaveCtx {
            yield_chan,
            resume_chan,
        } => (Arc::clone(yield_chan), Arc::clone(resume_chan)),
        _ => {
            eprintln!("strand.resume: expected WeaveHandle, got {:?}", handle);
            std::process::abort();
        }
    };

    // Wrap value in WeaveMessage for sending
    let msg_to_send = WeaveMessage::Value(value.clone());

    // Send resume value to the weave
    if resume_chan.sender.send(msg_to_send).is_err() {
        // Channel closed - weave is done
        let stack = unsafe { push(stack, handle) };
        let stack = unsafe { push(stack, Value::Int(0)) };
        return unsafe { push(stack, Value::Bool(false)) };
    }

    // Wait for yielded value
    match yield_chan.receiver.recv() {
        Ok(msg) => match msg {
            WeaveMessage::Done => {
                // Weave completed
                let stack = unsafe { push(stack, handle) };
                let stack = unsafe { push(stack, Value::Int(0)) };
                unsafe { push(stack, Value::Bool(false)) }
            }
            WeaveMessage::Value(yielded) => {
                // Normal yield
                let stack = unsafe { push(stack, handle) };
                let stack = unsafe { push(stack, yielded) };
                unsafe { push(stack, Value::Bool(true)) }
            }
            WeaveMessage::Cancel => {
                // Shouldn't happen - Cancel is sent on resume_chan
                let stack = unsafe { push(stack, handle) };
                let stack = unsafe { push(stack, Value::Int(0)) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        Err(_) => {
            // Channel closed unexpectedly
            let stack = unsafe { push(stack, handle) };
            let stack = unsafe { push(stack, Value::Int(0)) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

/// Yield a value from within a woven strand
///
/// Stack effect: ( WeaveCtx a -- WeaveCtx a )
///
/// Sends value `a` to the caller and waits for the next resume value.
/// The WeaveCtx must be passed through - it contains the channels.
///
/// # Error Handling
///
/// This function never panics (panicking in extern "C" is UB). On error:
/// - Type mismatch: eprintln + cleanup + block forever
/// - Channel closed: cleanup + block forever
///
/// The coroutine is marked as completed before blocking, so the program
/// can still terminate normally.
///
/// # Safety
/// Stack must have a value on top and WeaveCtx below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_yield(stack: Stack) -> Stack {
    // Note: We can't use assert! here (it panics). A null stack is a fatal
    // programming error, but we handle it gracefully to avoid UB.
    if stack.is_null() {
        eprintln!("yield: stack is null (fatal programming error)");
        crate::arena::arena_reset();
        cleanup_strand();
        block_forever();
    }

    // Pop the value to yield
    let (stack, value) = unsafe { pop(stack) };

    // Pop the WeaveCtx
    let (stack, ctx) = unsafe { pop(stack) };

    let (yield_chan, resume_chan) = match &ctx {
        Value::WeaveCtx {
            yield_chan,
            resume_chan,
        } => (Arc::clone(yield_chan), Arc::clone(resume_chan)),
        _ => {
            // Type mismatch - yield called without WeaveCtx on stack
            // This is a programming error but we can't panic (UB)
            eprintln!(
                "yield: expected WeaveCtx on stack, got {:?}. \
                 yield can only be called inside strand.weave with context threaded through.",
                ctx
            );
            crate::arena::arena_reset();
            cleanup_strand();
            block_forever();
        }
    };

    // Wrap value in WeaveMessage for sending
    let msg_to_send = WeaveMessage::Value(value.clone());

    // Send the yielded value
    if yield_chan.sender.send(msg_to_send).is_err() {
        // Channel unexpectedly closed - caller dropped the handle
        // Clean up and block forever (can't panic in extern "C")
        // We're still active here, so call cleanup_strand
        crate::arena::arena_reset();
        cleanup_strand();
        block_forever();
    }

    // IMPORTANT: Become "dormant" before waiting for resume (fixes #287)
    // This allows the scheduler to exit if the program ends while we're waiting.
    // We'll re-activate after receiving the resume value.
    use crate::scheduler::ACTIVE_STRANDS;
    use std::sync::atomic::Ordering;
    ACTIVE_STRANDS.fetch_sub(1, Ordering::AcqRel);

    // Wait for resume value (we're dormant now - not counted as active)
    let resume_msg = match resume_chan.receiver.recv() {
        Ok(msg) => msg,
        Err(_) => {
            // Resume channel closed - caller dropped the handle
            // We're already dormant (decremented above), don't call cleanup_strand
            crate::arena::arena_reset();
            block_forever();
        }
    };

    // Handle the message
    match resume_msg {
        WeaveMessage::Cancel => {
            // Weave was cancelled - signal completion and exit cleanly
            // We're already dormant (decremented above), don't call cleanup_strand
            let _ = yield_chan.sender.send(WeaveMessage::Done);
            crate::arena::arena_reset();
            block_forever();
        }
        WeaveMessage::Value(resume_value) => {
            // Re-activate: we're about to run user code again
            ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

            // Push WeaveCtx back, then resume value
            let stack = unsafe { push(stack, ctx) };
            unsafe { push(stack, resume_value) }
        }
        WeaveMessage::Done => {
            // Protocol error - Done should only be sent on yield_chan
            // We're already dormant (decremented above), don't call cleanup_strand
            crate::arena::arena_reset();
            block_forever();
        }
    }
}

/// Cancel a weave, releasing its resources
///
/// Stack effect: ( WeaveHandle -- )
///
/// Sends a cancellation signal to the weave, causing it to exit cleanly.
/// This is necessary to avoid resource leaks when abandoning a weave
/// before it completes naturally.
///
/// If the weave is:
/// - Waiting for first resume: exits immediately
/// - Waiting inside yield: receives cancel signal and can exit
/// - Already completed: no effect (signal is ignored)
///
/// # Error Handling
///
/// This function never panics (panicking in extern "C" is UB). On fatal error
/// (null stack, type mismatch), it prints an error and aborts the process.
///
/// # Safety
/// Stack must have a WeaveHandle (WeaveCtx) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_weave_cancel(stack: Stack) -> Stack {
    // Note: We can't use assert! here (it panics). Use abort() for fatal errors.
    if stack.is_null() {
        eprintln!("strand.weave-cancel: stack is null (fatal programming error)");
        std::process::abort();
    }

    // Pop the WeaveHandle
    let (stack, handle) = unsafe { pop(stack) };

    // Extract the resume channel to send cancel signal
    match handle {
        Value::WeaveCtx { resume_chan, .. } => {
            // Send cancel signal - if this fails, weave is already done (fine)
            let _ = resume_chan.sender.send(WeaveMessage::Cancel);
        }
        _ => {
            eprintln!(
                "strand.weave-cancel: expected WeaveHandle, got {:?}",
                handle
            );
            std::process::abort();
        }
    }

    // Handle is consumed (dropped), stack returned without it
    stack
}

// Public re-exports
pub use patch_seq_resume as resume;
pub use patch_seq_weave as weave;
pub use patch_seq_weave_cancel as weave_cancel;
pub use patch_seq_yield as weave_yield;
