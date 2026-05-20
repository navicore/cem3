//! Channel operations for CSP-style concurrency.
//!
//! Channels are the primary communication mechanism between strands.
//! They use May's MPMC channels with cooperative blocking.
//!
//! ## Zero-Mutex Design
//!
//! Channels are passed directly as `Value::Channel` on the stack. There is NO
//! global registry and NO mutex contention. Send/receive operations work
//! directly on the channel handles. The `closed` flag is a single atomic
//! load on the send hot path; no locking.
//!
//! ## Non-Blocking Guarantee
//!
//! All channel operations (`send`, `receive`) cooperatively block using May's
//! scheduler. They NEVER block OS threads — May handles scheduling other
//! strands while waiting.
//!
//! ## Multi-Consumer Support
//!
//! Channels support multiple producers AND multiple consumers (MPMC). Each
//! message is delivered to exactly one receiver (work-stealing semantics).
//!
//! ## `chan.close` Semantics
//!
//! Issue #499: `chan.close` is real, not "equivalent to drop". The
//! implementation uses the same typed-sentinel pattern as `WeaveChannelData`
//! — see `crates/core/src/value.rs::ChannelMsg` and
//! `docs/design/CHAN_CLOSE_SEMANTICS.md`.
//!
//! - `chan.close` atomically sets a shared `closed` flag (CAS) and, on the
//!   first close, enqueues a single `ChannelMsg::Closed` sentinel.
//! - `chan.send` short-circuits to `false` when the flag is set.
//! - `chan.receive` returns `( value true )` on `ChannelMsg::Value`. On
//!   `ChannelMsg::Closed` it re-broadcasts the sentinel (so the next blocked
//!   receiver also wakes — Go-style propagation across an unknown number of
//!   MPMC consumers) and returns `( default false )`.
//!
//! The user-facing API is unchanged: programs still write `Channel`, still
//! call `chan.make`/`chan.send`/`chan.receive`/`chan.close`, still see the
//! `( value Bool )` success-flag shape.
//!
//! ## Stack Effects
//!
//! - `chan.make`:    ( -- Channel )
//! - `chan.send`:    ( value Channel -- Bool )         consumes the channel
//! - `chan.receive`: ( Channel -- value Bool )         consumes the channel
//! - `chan.close`:   ( Channel -- )                    consumes the channel

use crate::stack::{Stack, pop, push};
use crate::value::{ChannelData, ChannelMsg, Value};
use may::sync::mpmc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "diagnostics")]
use std::sync::atomic::AtomicU64;

#[cfg(feature = "diagnostics")]
pub static TOTAL_MESSAGES_SENT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "diagnostics")]
pub static TOTAL_MESSAGES_RECEIVED: AtomicU64 = AtomicU64::new(0);

/// Create a new channel.
///
/// Stack effect: ( -- Channel )
///
/// Returns a Channel value that can be used with send/receive operations.
/// The channel can be duplicated (`dup`) to share between strands; each
/// clone shares the same underlying `mpmc` queue and closed flag.
///
/// # Safety
/// Always safe to call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_channel(stack: Stack) -> Stack {
    let (sender, receiver) = mpmc::channel::<ChannelMsg>();
    let channel = Arc::new(ChannelData {
        sender,
        receiver,
        closed: Arc::new(AtomicBool::new(false)),
    });
    unsafe { push(stack, Value::Channel(channel)) }
}

/// Close a channel.
///
/// Stack effect: ( Channel -- )
///
/// Atomically marks the channel closed (idempotent across multiple
/// `chan.close` callers via CAS) and, on the first close, enqueues one
/// `ChannelMsg::Closed` sentinel. Any blocked `chan.receive` calls wake
/// up: the first one consumes the sentinel and re-broadcasts it,
/// propagating the close through the MPMC fan-out lazily as each
/// blocked receiver is scheduled.
///
/// # Safety
/// Stack must have a Channel on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_close_channel(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "chan.close: stack is empty");

    let (rest, channel_value) = unsafe { pop(stack) };
    let channel = match channel_value {
        Value::Channel(ch) => ch,
        other => panic!("chan.close: expected Channel on stack, got {:?}", other),
    };

    // First-to-close wins the CAS and is responsible for enqueueing the
    // sentinel. Subsequent closes are no-ops (idempotent).
    if channel
        .closed
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        // Best-effort: if the queue is somehow broken, there's nothing
        // meaningful to do — the closed flag is the durable signal that
        // chan.send checks; chan.receive will also see Err from recv()
        // when every Arc<ChannelData> drops.
        let _ = channel.sender.send(ChannelMsg::Closed);
    }

    rest
}

/// Send a value through a channel.
///
/// Stack effect: ( value Channel -- Bool )
///
/// Returns `true` on success, `false` if the channel is closed (either via
/// `chan.close` or because every receiver has been dropped).
///
/// # Safety
/// Stack must have a Channel on top and a value below it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_send(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "chan.send: stack is empty");

    let (stack, channel_value) = unsafe { pop(stack) };
    let channel = match channel_value {
        Value::Channel(ch) => ch,
        _ => {
            // Wrong type — consume value (if any) and return failure.
            if !stack.is_null() {
                let (rest, _value) = unsafe { pop(stack) };
                return unsafe { push(rest, Value::Bool(false)) };
            }
            return unsafe { push(stack, Value::Bool(false)) };
        }
    };

    if stack.is_null() {
        return unsafe { push(stack, Value::Bool(false)) };
    }

    let (rest, value) = unsafe { pop(stack) };

    // Closed gate: short-circuit on the fast path so user code reliably
    // sees `false` after close.
    if channel.closed.load(Ordering::Acquire) {
        return unsafe { push(rest, Value::Bool(false)) };
    }

    let global_value = value.clone();
    match channel.sender.send(ChannelMsg::Value(global_value)) {
        Ok(()) => {
            #[cfg(feature = "diagnostics")]
            TOTAL_MESSAGES_SENT.fetch_add(1, Ordering::Relaxed);
            unsafe { push(rest, Value::Bool(true)) }
        }
        Err(_) => unsafe { push(rest, Value::Bool(false)) },
    }
}

/// Receive a value from a channel.
///
/// Stack effect: ( Channel -- value Bool )
///
/// Blocks cooperatively until a value arrives or the channel is closed and
/// drained. Returns `( value true )` on success, `( Int(0) false )` on
/// close. The closed-sentinel is re-broadcast before returning so other
/// blocked receivers wake up too — see module doc.
///
/// # Safety
/// Stack must have a Channel on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_receive(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "chan.receive: stack is empty");

    let (rest, channel_value) = unsafe { pop(stack) };
    let channel = match channel_value {
        Value::Channel(ch) => ch,
        _ => {
            let stack = unsafe { push(rest, Value::Int(0)) };
            return unsafe { push(stack, Value::Bool(false)) };
        }
    };

    match channel.receiver.recv() {
        Ok(ChannelMsg::Value(value)) => {
            #[cfg(feature = "diagnostics")]
            TOTAL_MESSAGES_RECEIVED.fetch_add(1, Ordering::Relaxed);
            let stack = unsafe { push(rest, value) };
            unsafe { push(stack, Value::Bool(true)) }
        }
        Ok(ChannelMsg::Closed) => {
            // Propagate the close to other waiters. Ignore the result —
            // the queue may be in any state; the closed flag is the
            // durable signal, this is just a wake-up nudge.
            let _ = channel.sender.send(ChannelMsg::Closed);
            let stack = unsafe { push(rest, Value::Int(0)) };
            unsafe { push(stack, Value::Bool(false)) }
        }
        Err(_) => {
            // All Arc<ChannelData> instances dropped — every sender clone
            // and every receiver clone is gone. Treat as closed.
            let stack = unsafe { push(rest, Value::Int(0)) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_chan_receive as receive;
pub use patch_seq_chan_send as send;
pub use patch_seq_close_channel as close_channel;
pub use patch_seq_make_channel as make_channel;

#[cfg(test)]
mod tests;
