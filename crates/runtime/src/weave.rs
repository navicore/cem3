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

use crate::stack::{Stack, pop, push};
use crate::tagged_stack::StackValue;
use crate::value::{ChannelData, Value};
use may::sync::mpmc;
use std::sync::Arc;

/// Sentinel value to signal weave completion
const DONE_SENTINEL: i64 = i64::MIN;

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
/// # Safety
/// Stack must have a Quotation on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_weave(stack: Stack) -> Stack {
    // Create the two internal channels - NO registry, just Arc values
    let (yield_sender, yield_receiver) = mpmc::channel();
    let yield_chan = Arc::new(ChannelData {
        sender: yield_sender,
        receiver: yield_receiver,
    });

    let (resume_sender, resume_receiver) = mpmc::channel();
    let resume_chan = Arc::new(ChannelData {
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
                panic!("strand.weave: quotation wrapper function pointer is null");
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

            ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

            unsafe {
                coroutine::spawn(move || {
                    let child_stack = stack_addr as *mut StackValue;
                    let child_base = base_addr as *mut StackValue;

                    if !child_base.is_null() {
                        crate::stack::patch_seq_set_stack_base(child_base);
                    }

                    // Wait for first resume value before executing
                    let first_value = match weave_ctx_resume.receiver.recv() {
                        Ok(v) => v,
                        Err(_) => {
                            cleanup_strand();
                            return;
                        }
                    };

                    // Push WeaveCtx onto stack (yield_chan, resume_chan as a pair)
                    // We use a Variant to bundle both channels
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
                        let _ = yield_chan.sender.send(Value::Int(DONE_SENTINEL));
                    }

                    crate::arena::arena_reset();
                    cleanup_strand();
                });
            }
        }
        Value::Closure { fn_ptr, env } => {
            if fn_ptr == 0 {
                panic!("strand.weave: closure function pointer is null");
            }

            use crate::scheduler::ACTIVE_STRANDS;
            use may::coroutine;
            use std::sync::atomic::Ordering;

            let fn_ref: extern "C" fn(Stack, *const Value, usize) -> Stack =
                unsafe { std::mem::transmute(fn_ptr) };
            let env_clone: Vec<Value> = env.iter().cloned().collect();

            let child_base = crate::stack::alloc_stack();
            let base_addr = child_base as usize;

            ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

            unsafe {
                coroutine::spawn(move || {
                    let child_base = base_addr as *mut StackValue;
                    crate::stack::patch_seq_set_stack_base(child_base);

                    // Wait for first resume value
                    let first_value = match weave_ctx_resume.receiver.recv() {
                        Ok(v) => v,
                        Err(_) => {
                            cleanup_strand();
                            return;
                        }
                    };

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
                        let _ = yield_chan.sender.send(Value::Int(DONE_SENTINEL));
                    }

                    crate::arena::arena_reset();
                    cleanup_strand();
                });
            }
        }
        _ => panic!(
            "strand.weave: expected Quotation or Closure, got {:?}",
            quot_value
        ),
    }

    // Return WeaveHandle (contains both channels for resume to use)
    let handle = Value::WeaveCtx {
        yield_chan: handle_yield,
        resume_chan: handle_resume,
    };
    unsafe { push(stack, handle) }
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
/// # Safety
/// Stack must have a value on top and WeaveHandle below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_resume(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "strand.resume: stack is empty");

    // Pop the value to send
    let (stack, value) = unsafe { pop(stack) };

    // Pop the WeaveHandle
    let (stack, handle) = unsafe { pop(stack) };

    let (yield_chan, resume_chan) = match &handle {
        Value::WeaveCtx {
            yield_chan,
            resume_chan,
        } => (Arc::clone(yield_chan), Arc::clone(resume_chan)),
        _ => panic!("strand.resume: expected WeaveHandle, got {:?}", handle),
    };

    // Clone value for sending
    let value_to_send = value.clone();

    // Send resume value to the weave
    if resume_chan.sender.send(value_to_send).is_err() {
        // Channel closed - weave is done
        let stack = unsafe { push(stack, handle) };
        let stack = unsafe { push(stack, Value::Int(0)) };
        return unsafe { push(stack, Value::Bool(false)) };
    }

    // Wait for yielded value
    match yield_chan.receiver.recv() {
        Ok(yielded) => {
            // Check for Done sentinel
            if let Value::Int(DONE_SENTINEL) = yielded {
                // Weave completed
                let stack = unsafe { push(stack, handle) };
                let stack = unsafe { push(stack, Value::Int(0)) };
                unsafe { push(stack, Value::Bool(false)) }
            } else {
                // Normal yield
                let stack = unsafe { push(stack, handle) };
                let stack = unsafe { push(stack, yielded) };
                unsafe { push(stack, Value::Bool(true)) }
            }
        }
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
/// # Panics
/// Panics if WeaveCtx is not on the stack.
///
/// # Safety
/// Stack must have a value on top and WeaveCtx below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_yield(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "yield: stack is empty");

    // Pop the value to yield
    let (stack, value) = unsafe { pop(stack) };

    // Pop the WeaveCtx
    let (stack, ctx) = unsafe { pop(stack) };

    let (yield_chan, resume_chan) = match &ctx {
        Value::WeaveCtx {
            yield_chan,
            resume_chan,
        } => (Arc::clone(yield_chan), Arc::clone(resume_chan)),
        _ => panic!(
            "yield: expected WeaveCtx on stack, got {:?}. yield can only be called inside strand.weave with context threaded through.",
            ctx
        ),
    };

    // Clone value for sending
    let value_to_send = value.clone();

    // Send the yielded value
    if yield_chan.sender.send(value_to_send).is_err() {
        panic!("yield: yield channel closed unexpectedly");
    }

    // Wait for resume value
    let resume_value = match resume_chan.receiver.recv() {
        Ok(v) => v,
        Err(_) => panic!("yield: resume channel closed unexpectedly"),
    };

    // Push WeaveCtx back, then resume value
    let stack = unsafe { push(stack, ctx) };
    unsafe { push(stack, resume_value) }
}

// Public re-exports
pub use patch_seq_resume as resume;
pub use patch_seq_weave as weave;
pub use patch_seq_yield as weave_yield;
