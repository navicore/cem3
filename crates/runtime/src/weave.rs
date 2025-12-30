//! Weave operations for generator/coroutine-style concurrency
//!
//! A "weave" is a strand that can yield values back to its caller and be resumed.
//! Unlike regular strands (fire-and-forget), weaves allow bidirectional communication
//! with structured yield/resume semantics.
//!
//! ## API
//!
//! - `strand.weave`: ( Quotation -- Int ) - creates a woven strand, returns weave ID
//! - `strand.resume`: ( Int a -- Int a Bool ) - resume with value, get (weave_id, yielded_value, has_more)
//! - `yield`: ( a -- a ) - yield a value, receive resume value (only valid inside weave)
//!
//! ## Architecture
//!
//! Each weave has two internal channels:
//! - yield_chan: for sending yielded values from weave to caller
//! - resume_chan: for sending resume values from caller to weave
//!
//! Flow:
//! 1. strand.weave creates channels and spawns a coroutine
//! 2. The coroutine immediately waits on resume_chan for the first resume value
//! 3. Caller calls strand.resume, sending value to resume_chan
//! 4. Coroutine wakes, receives value, runs until yield
//! 5. yield sends to yield_chan, waits on resume_chan
//! 6. Caller's resume receives from yield_chan, returns the value
//! 7. When quotation returns naturally, Done sentinel is sent

use crate::stack::{Stack, pop, push};
use crate::tagged_stack::StackValue;
use crate::value::{ChannelData, Value};
use may::sync::mpmc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

/// Sentinel value to signal weave completion
/// We use a special Int value that's unlikely to be a real value
const DONE_SENTINEL: i64 = i64::MIN;

/// Global registry for weave data
/// Maps weave_id -> (yield_chan, resume_chan)
static WEAVE_REGISTRY: Mutex<Option<HashMap<i64, WeaveData>>> = Mutex::new(None);

/// Next weave ID
static NEXT_WEAVE_ID: AtomicI64 = AtomicI64::new(1);

/// Data associated with a weave
struct WeaveData {
    yield_chan: Arc<ChannelData>,
    resume_chan: Arc<ChannelData>,
}

// Thread-local weave context for yield to use
thread_local! {
    static CURRENT_WEAVE: RefCell<Option<WeaveContext>> = const { RefCell::new(None) };
}

/// Context for the currently executing weave (stored in thread-local)
struct WeaveContext {
    yield_chan: Arc<ChannelData>,
    resume_chan: Arc<ChannelData>,
}

/// Initialize the weave registry
fn init_registry() {
    let mut guard = WEAVE_REGISTRY.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
}

/// Create a woven strand from a quotation
///
/// Stack effect: ( Quotation -- Int )
///
/// Creates a weave from the quotation. The weave is initially suspended,
/// waiting to be resumed with the first value.
///
/// Returns the weave ID (Int).
///
/// # Safety
/// Stack must have a Quotation on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_weave(stack: Stack) -> Stack {
    init_registry();

    // Generate unique weave ID
    let weave_id = NEXT_WEAVE_ID.fetch_add(1, Ordering::Relaxed);

    // Create the two internal channels
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

    // Store weave data in registry
    {
        let mut guard = WEAVE_REGISTRY.lock().unwrap();
        let registry = guard.as_mut().unwrap();
        registry.insert(
            weave_id,
            WeaveData {
                yield_chan: Arc::clone(&yield_chan),
                resume_chan: Arc::clone(&resume_chan),
            },
        );
    }

    // Pop the quotation from stack
    let (stack, quot_value) = unsafe { pop(stack) };

    // Clone channels for the spawned strand
    let yield_chan_clone = Arc::clone(&yield_chan);
    let resume_chan_clone = Arc::clone(&resume_chan);

    match quot_value {
        Value::Quotation { wrapper, .. } => {
            if wrapper == 0 {
                panic!("strand.weave: quotation wrapper function pointer is null");
            }

            // Clone channels for use in the closure
            let yc = yield_chan_clone;
            let rc = resume_chan_clone;

            // Spawn a strand that:
            // 1. Sets up the weave context
            // 2. Waits for first resume value
            // 3. Runs the quotation
            // 4. Signals completion

            // We need to spawn using may's coroutine directly since we need
            // to set up the weave context before running the quotation
            use crate::scheduler::ACTIVE_STRANDS;
            use may::coroutine;

            let fn_ptr: extern "C" fn(Stack) -> Stack = unsafe { std::mem::transmute(wrapper) };

            // Clone the stack for the child
            let (child_stack, child_base) = unsafe { crate::stack::clone_stack_with_base(stack) };

            // Convert pointers to usize (which is Send) - same pattern as spawn
            let stack_addr = child_stack as usize;
            let base_addr = child_base as usize;

            // Increment active strand counter
            ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

            unsafe {
                coroutine::spawn(move || {
                    // Reconstruct pointers from addresses
                    let child_stack = stack_addr as *mut StackValue;
                    let child_base = base_addr as *mut StackValue;

                    // Set up stack base
                    if !child_base.is_null() {
                        crate::stack::patch_seq_set_stack_base(child_base);
                    }

                    // Set up weave context for this strand
                    CURRENT_WEAVE.with(|ctx| {
                        *ctx.borrow_mut() = Some(WeaveContext {
                            yield_chan: yc.clone(),
                            resume_chan: rc.clone(),
                        });
                    });

                    // Wait for first resume value before executing
                    let first_value = match rc.receiver.recv() {
                        Ok(v) => v,
                        Err(_) => {
                            // Resume channel closed before first resume - just exit
                            cleanup_strand();
                            return;
                        }
                    };

                    // Push the first resume value onto the child's stack
                    let stack_with_value = push(child_stack, first_value);

                    // Execute the quotation
                    let _final_stack = fn_ptr(stack_with_value);

                    // Quotation returned - send Done sentinel
                    let _ = yc.sender.send(Value::Int(DONE_SENTINEL));

                    // Clean up
                    crate::arena::arena_reset();
                    cleanup_strand();
                });
            }
        }
        Value::Closure { fn_ptr, env } => {
            if fn_ptr == 0 {
                panic!("strand.weave: closure function pointer is null");
            }

            let yc = yield_chan_clone;
            let rc = resume_chan_clone;

            use crate::scheduler::ACTIVE_STRANDS;
            use may::coroutine;

            let fn_ref: extern "C" fn(Stack, *const Value, usize) -> Stack =
                unsafe { std::mem::transmute(fn_ptr) };
            let env_clone: Vec<Value> = env.iter().cloned().collect();

            // Create initial stack for the child
            let child_base = crate::stack::alloc_stack();

            // Convert pointers to usize (which is Send)
            let base_addr = child_base as usize;

            ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

            unsafe {
                coroutine::spawn(move || {
                    // Reconstruct pointer from address
                    let child_base = base_addr as *mut StackValue;

                    crate::stack::patch_seq_set_stack_base(child_base);

                    // Set up weave context
                    CURRENT_WEAVE.with(|ctx| {
                        *ctx.borrow_mut() = Some(WeaveContext {
                            yield_chan: yc.clone(),
                            resume_chan: rc.clone(),
                        });
                    });

                    // Wait for first resume value
                    let first_value = match rc.receiver.recv() {
                        Ok(v) => v,
                        Err(_) => {
                            cleanup_strand();
                            return;
                        }
                    };

                    // Push the first resume value onto the stack
                    let stack_with_value = push(child_base, first_value);

                    // Execute the closure
                    let _final_stack =
                        fn_ref(stack_with_value, env_clone.as_ptr(), env_clone.len());

                    // Send Done sentinel
                    let _ = yc.sender.send(Value::Int(DONE_SENTINEL));

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

    // Return weave ID on the parent's stack
    unsafe { push(stack, Value::Int(weave_id)) }
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
/// Stack effect: ( Int a -- Int a Bool )
///
/// Sends value `a` to the weave and waits for it to yield.
/// Returns (weave_id, yielded_value, has_more).
/// - has_more = true: weave yielded a value
/// - has_more = false: weave completed (yielded_value is undefined)
///
/// # Safety
/// Stack must have a value on top and weave ID below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_resume(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "strand.resume: stack is empty");

    // Pop the value to send
    let (stack, value) = unsafe { pop(stack) };

    // Pop the weave ID
    let (stack, weave_id_val) = unsafe { pop(stack) };
    let weave_id = match weave_id_val {
        Value::Int(id) => id,
        _ => panic!(
            "strand.resume: expected Int weave ID, got {:?}",
            weave_id_val
        ),
    };

    // Look up the weave channels
    let (yield_chan, resume_chan) = {
        let guard = WEAVE_REGISTRY.lock().unwrap();
        let registry = guard
            .as_ref()
            .expect("strand.resume: weave registry not initialized");
        match registry.get(&weave_id) {
            Some(data) => (Arc::clone(&data.yield_chan), Arc::clone(&data.resume_chan)),
            None => {
                // Weave not found - return failure
                let stack = unsafe { push(stack, Value::Int(weave_id)) };
                let stack = unsafe { push(stack, Value::Int(0)) }; // placeholder value
                return unsafe { push(stack, Value::Bool(false)) };
            }
        }
    };

    // Clone value for sending (to handle arena strings)
    let value_to_send = clone_value_for_channel(&value);

    // Send resume value to the weave
    if resume_chan.sender.send(value_to_send).is_err() {
        // Channel closed - weave is done
        let stack = unsafe { push(stack, Value::Int(weave_id)) };
        let stack = unsafe { push(stack, Value::Int(0)) };
        return unsafe { push(stack, Value::Bool(false)) };
    }

    // Wait for yielded value
    match yield_chan.receiver.recv() {
        Ok(yielded) => {
            // Check for Done sentinel
            if let Value::Int(DONE_SENTINEL) = yielded {
                // Weave completed
                // Clean up registry
                {
                    let mut guard = WEAVE_REGISTRY.lock().unwrap();
                    if let Some(registry) = guard.as_mut() {
                        registry.remove(&weave_id);
                    }
                }
                let stack = unsafe { push(stack, Value::Int(weave_id)) };
                let stack = unsafe { push(stack, Value::Int(0)) }; // placeholder
                unsafe { push(stack, Value::Bool(false)) }
            } else {
                // Normal yield
                let stack = unsafe { push(stack, Value::Int(weave_id)) };
                let stack = unsafe { push(stack, yielded) };
                unsafe { push(stack, Value::Bool(true)) }
            }
        }
        Err(_) => {
            // Channel closed unexpectedly
            let stack = unsafe { push(stack, Value::Int(weave_id)) };
            let stack = unsafe { push(stack, Value::Int(0)) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

/// Yield a value from within a woven strand
///
/// Stack effect: ( a -- a )
///
/// Sends value `a` to the caller and waits for the next resume value.
/// Returns the resume value.
///
/// # Panics
/// Panics if called outside of a weave context.
///
/// # Safety
/// Stack must have a value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_yield(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "yield: stack is empty");

    // Pop the value to yield
    let (stack, value) = unsafe { pop(stack) };

    // Get the weave context from thread-local storage
    let (yield_chan, resume_chan) = CURRENT_WEAVE.with(|ctx| {
        let ctx_ref = ctx.borrow();
        match ctx_ref.as_ref() {
            Some(weave_ctx) => (
                Arc::clone(&weave_ctx.yield_chan),
                Arc::clone(&weave_ctx.resume_chan),
            ),
            None => {
                panic!("yield: not inside a weave - yield can only be called within strand.weave")
            }
        }
    });

    // Clone value for sending
    let value_to_send = clone_value_for_channel(&value);

    // Send the yielded value
    if yield_chan.sender.send(value_to_send).is_err() {
        panic!("yield: yield channel closed unexpectedly");
    }

    // Wait for resume value
    match resume_chan.receiver.recv() {
        Ok(resume_value) => unsafe { push(stack, resume_value) },
        Err(_) => panic!("yield: resume channel closed unexpectedly"),
    }
}

/// Clone a value for channel transmission
/// Uses Value's Clone impl which handles arena string promotion to global
fn clone_value_for_channel(value: &Value) -> Value {
    value.clone()
}

// Public re-exports
pub use patch_seq_resume as resume;
pub use patch_seq_weave as weave;
pub use patch_seq_yield as weave_yield;
