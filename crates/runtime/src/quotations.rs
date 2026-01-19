//! Quotation operations for Seq
//!
//! Quotations are deferred code blocks (first-class functions).
//! A quotation is represented as a function pointer stored as usize.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Type alias for closure registry entries
/// Uses Box (not Arc) because cross-thread transfer needs owned data
/// and cloning ensures arena strings become global strings
type ClosureEntry = (usize, Box<[Value]>);

/// Global registry for closure environments in spawned strands
/// Maps closure_spawn_id -> (fn_ptr, env)
/// Cleaned up when the trampoline retrieves and executes the closure
static SPAWN_CLOSURE_REGISTRY: LazyLock<Mutex<HashMap<i64, ClosureEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// RAII guard for cleanup of spawn registry on failure
///
/// If the spawned strand fails to start or panics before retrieving
/// the closure from the registry, this guard ensures the environment
/// is cleaned up and not leaked.
struct SpawnRegistryGuard {
    closure_spawn_id: i64,
    should_cleanup: bool,
}

impl SpawnRegistryGuard {
    fn new(closure_spawn_id: i64) -> Self {
        Self {
            closure_spawn_id,
            should_cleanup: true,
        }
    }

    /// Disarm the guard - strand successfully started and will retrieve the closure
    fn disarm(&mut self) {
        self.should_cleanup = false;
    }
}

impl Drop for SpawnRegistryGuard {
    fn drop(&mut self) {
        if self.should_cleanup {
            let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            if let Some((_, env)) = registry.remove(&self.closure_spawn_id) {
                // env (Box<[Value]>) will be dropped here, freeing memory
                drop(env);
            }
        }
    }
}

/// Trampoline function for spawning closures
///
/// This function is passed to strand_spawn when spawning a closure.
/// It expects the closure_spawn_id on the stack, retrieves the closure data
/// from the registry, and calls the closure function with the environment.
///
/// Stack effect: ( closure_spawn_id -- ... )
/// The closure function determines the final stack state.
///
/// # Safety
/// This function is safe to call, but internally uses unsafe operations
/// to transmute function pointers and call the closure function.
extern "C" fn closure_spawn_trampoline(stack: Stack) -> Stack {
    unsafe {
        // Pop closure_spawn_id from stack
        let (stack, closure_spawn_id_val) = pop(stack);
        let closure_spawn_id = match closure_spawn_id_val {
            Value::Int(id) => id,
            _ => panic!(
                "closure_spawn_trampoline: expected Int (closure_spawn_id), got {:?}",
                closure_spawn_id_val
            ),
        };

        // Retrieve closure data from registry
        let (fn_ptr, env) = {
            let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            registry.remove(&closure_spawn_id).unwrap_or_else(|| {
                panic!(
                    "closure_spawn_trampoline: no data for closure_spawn_id {}",
                    closure_spawn_id
                )
            })
        };

        // Call closure function with empty stack and environment
        // Closure signature: fn(Stack, *const Value, usize) -> Stack
        let env_ptr = env.as_ptr();
        let env_len = env.len();

        let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
            std::mem::transmute(fn_ptr);

        // Call closure and return result (Arc ref count decremented after return)
        fn_ref(stack, env_ptr, env_len)
    }
}

/// Push a quotation onto the stack with both wrapper and impl pointers
///
/// Stack effect: ( -- quot )
///
/// # Arguments
/// - `wrapper`: C-convention function pointer for runtime calls
/// - `impl_`: tailcc function pointer for TCO tail calls
///
/// # Safety
/// - Stack pointer must be valid (or null for empty stack)
/// - Both function pointers must be valid (compiler guarantees this)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_quotation(
    stack: Stack,
    wrapper: usize,
    impl_: usize,
) -> Stack {
    // Debug-only validation - compiler guarantees non-null pointers
    // Using debug_assert to avoid UB from panicking across FFI boundary
    debug_assert!(
        wrapper != 0,
        "push_quotation: wrapper function pointer is null"
    );
    debug_assert!(impl_ != 0, "push_quotation: impl function pointer is null");
    unsafe { push(stack, Value::Quotation { wrapper, impl_ }) }
}

/// Check if the top of stack is a quotation (not a closure)
///
/// Used by the compiler for tail call optimization of `call`.
/// Returns 1 if the top value is a Quotation, 0 otherwise.
///
/// Stack effect: ( quot -- quot ) [non-consuming peek]
///
/// # Safety
/// - Stack must not be null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_peek_is_quotation(stack: Stack) -> i64 {
    use crate::stack::peek;
    unsafe {
        let value = peek(stack);
        match value {
            Value::Quotation { .. } => 1,
            _ => 0,
        }
    }
}

/// Get the impl_ function pointer from a quotation on top of stack
///
/// Used by the compiler for tail call optimization of `call`.
/// Returns the tailcc impl_ pointer for musttail calls from compiled code.
/// Caller must ensure the top value is a Quotation (use peek_is_quotation first).
///
/// Stack effect: ( quot -- quot ) [non-consuming peek]
///
/// # Safety
/// - Stack must not be null
/// - Top of stack must be a Quotation (panics otherwise)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_peek_quotation_fn_ptr(stack: Stack) -> usize {
    use crate::stack::peek;
    unsafe {
        let value = peek(stack);
        match value {
            Value::Quotation { impl_, .. } => {
                // Debug-only validation - compiler guarantees non-null pointers
                debug_assert!(
                    impl_ != 0,
                    "peek_quotation_fn_ptr: impl function pointer is null"
                );
                impl_
            }
            // This branch indicates a compiler bug - patch_seq_peek_is_quotation should
            // have been called first to verify the value type. In release builds,
            // returning 0 will cause a crash at the call site rather than here.
            _ => {
                debug_assert!(
                    false,
                    "peek_quotation_fn_ptr: expected Quotation, got {:?}",
                    value
                );
                0
            }
        }
    }
}

/// Call a quotation or closure
///
/// Pops a quotation or closure from the stack and executes it.
/// For stateless quotations, calls the function with just the stack.
/// For closures, calls the function with both the stack and captured environment.
/// The function takes the current stack and returns a new stack.
///
/// Stack effect: ( ..a quot -- ..b )
/// where the quotation has effect ( ..a -- ..b )
///
/// # TCO Considerations
///
/// With Arc-based closure environments, this function is tail-position friendly:
/// no cleanup is needed after the call returns (Arc ref-counting handles it).
///
/// However, full `musttail` TCO across quotations and closures is limited by
/// calling convention mismatches:
/// - Quotations use `tailcc` with signature: `fn(Stack) -> Stack`
/// - Closures use C convention with signature: `fn(Stack, *const Value, usize) -> Stack`
///
/// LLVM's `musttail` requires matching signatures, so the compiler can only
/// guarantee TCO within the same category (quotation-to-quotation or closure-to-closure).
/// Cross-category calls go through this function, which is still efficient but
/// doesn't use `musttail`.
///
/// # Safety
/// - Stack must not be null
/// - Top of stack must be a Quotation or Closure value
/// - Function pointer must be valid
/// - Quotation signature: Stack -> Stack
/// - Closure signature: Stack, *const [Value] -> Stack
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_call(stack: Stack) -> Stack {
    unsafe {
        let (stack, value) = pop(stack);

        match value {
            Value::Quotation { wrapper, .. } => {
                // Validate function pointer is not null
                if wrapper == 0 {
                    panic!("call: quotation wrapper function pointer is null");
                }

                // SAFETY: wrapper was created by the compiler's codegen and stored via push_quotation.
                // The compiler guarantees that quotation wrapper functions use C calling convention
                // with the signature: unsafe extern "C" fn(Stack) -> Stack.
                // We've verified wrapper is non-null above.
                let fn_ref: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(wrapper);
                fn_ref(stack)
            }
            Value::Closure { fn_ptr, env } => {
                // Validate function pointer is not null
                if fn_ptr == 0 {
                    panic!("call: closure function pointer is null");
                }

                // Get environment data pointer and length from Arc
                // Arc enables TCO: no explicit cleanup needed, ref-count handles it
                let env_data = env.as_ptr();
                let env_len = env.len();

                // SAFETY: fn_ptr was created by the compiler's codegen for a closure.
                // The compiler guarantees that closure functions have the signature:
                // unsafe extern "C" fn(Stack, *const Value, usize) -> Stack.
                // We pass the environment as (data, len) since LLVM can't handle fat pointers.
                // The Arc keeps the environment alive during the call and is dropped after.
                let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
                    std::mem::transmute(fn_ptr);
                fn_ref(stack, env_data, env_len)
            }
            _ => panic!(
                "call: expected Quotation or Closure on stack, got {:?}",
                value
            ),
        }
    }
}

/// Spawn a quotation or closure as a new strand (green thread)
///
/// Pops a quotation or closure from the stack and spawns it as a new strand.
/// - For Quotations: The quotation executes concurrently with an empty initial stack
/// - For Closures: The closure executes with its captured environment
///
/// Returns the strand ID.
///
/// Stack effect: ( ..a quot -- ..a strand_id )
/// Spawns a quotation or closure as a new strand (green thread).
///
/// The child strand receives a COPY of the parent's stack (after popping the quotation).
/// This enables CSP/Actor patterns where actors receive arguments via the stack.
///
/// Stack effect: ( ...args quotation -- ...args strand-id )
/// - Parent: keeps original stack with quotation removed, plus strand-id
/// - Child: gets a clone of the stack (without quotation)
///
/// # Safety
/// - Stack must have at least 1 value
/// - Top must be Quotation or Closure
/// - Function must be safe to execute on any thread
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_spawn(stack: Stack) -> Stack {
    use crate::scheduler::patch_seq_strand_spawn_with_base;
    use crate::stack::clone_stack_with_base;

    unsafe {
        // Pop quotation or closure
        let (stack, value) = pop(stack);

        match value {
            Value::Quotation { wrapper, .. } => {
                // Validate function pointer is not null
                if wrapper == 0 {
                    panic!("spawn: quotation wrapper function pointer is null");
                }

                // SAFETY: wrapper was created by the compiler's codegen and stored via push_quotation.
                // The compiler guarantees that quotation wrapper functions use C calling convention.
                // We've verified wrapper is non-null above.
                let fn_ref: extern "C" fn(Stack) -> Stack = std::mem::transmute(wrapper);

                // Clone the parent's stack for the child, getting both sp and base
                // The child gets a copy of the stack (after the quotation was popped)
                let (child_stack, child_base) = clone_stack_with_base(stack);

                // Spawn the strand with the cloned stack and its base
                // The scheduler will set STACK_BASE for the child strand
                let strand_id = patch_seq_strand_spawn_with_base(fn_ref, child_stack, child_base);

                // Push strand ID back onto the parent's stack
                push(stack, Value::Int(strand_id))
            }
            Value::Closure { fn_ptr, env } => {
                // Validate function pointer is not null
                if fn_ptr == 0 {
                    panic!("spawn: closure function pointer is null");
                }

                // We need to pass the closure data to the spawned strand.
                // We use a registry with a unique ID (separate from strand_id).
                use std::sync::atomic::{AtomicI64, Ordering};
                static NEXT_CLOSURE_SPAWN_ID: AtomicI64 = AtomicI64::new(1);
                let closure_spawn_id = NEXT_CLOSURE_SPAWN_ID.fetch_add(1, Ordering::Relaxed);

                // Store closure data in registry
                // Clone the Arc contents to Box - this ensures:
                // 1. Arena-allocated strings are copied to global memory
                // 2. The spawned strand gets independent ownership
                {
                    let env_box: Box<[Value]> = env.iter().cloned().collect();
                    let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
                    registry.insert(closure_spawn_id, (fn_ptr, env_box));
                }

                // Create a guard to cleanup registry on failure
                // If spawn fails or the strand panics before retrieving the closure,
                // the guard's Drop impl will remove the registry entry
                let mut guard = SpawnRegistryGuard::new(closure_spawn_id);

                // Create initial stack with the closure_spawn_id
                // The base is the freshly allocated stack pointer
                let stack_base = crate::stack::alloc_stack();
                let initial_stack = push(stack_base, Value::Int(closure_spawn_id));

                // Spawn strand with trampoline, passing the stack base
                let strand_id = patch_seq_strand_spawn_with_base(
                    closure_spawn_trampoline,
                    initial_stack,
                    stack_base,
                );

                // Spawn succeeded - disarm the guard so it won't cleanup
                // The trampoline will retrieve and remove the closure data from the registry
                guard.disarm();

                // Push strand ID back onto stack
                push(stack, Value::Int(strand_id))
            }
            _ => panic!("spawn: expected Quotation or Closure, got {:?}", value),
        }
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_call as call;
pub use patch_seq_push_quotation as push_quotation;
pub use patch_seq_spawn as spawn;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arithmetic::push_int;
    use crate::value::Value;

    #[test]
    fn test_spawn_registry_guard_cleanup() {
        // Test that the RAII guard cleans up the registry on drop
        let closure_id = 12345;

        // Create a test closure environment
        let env: Box<[Value]> = vec![Value::Int(42), Value::Int(99)].into_boxed_slice();
        let fn_ptr: usize = 0x1234;

        // Insert into registry
        {
            let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            registry.insert(closure_id, (fn_ptr, env));
        }

        // Verify it's in the registry
        {
            let registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            assert!(registry.contains_key(&closure_id));
        }

        // Create a guard (without disarming) and let it drop
        {
            let _guard = SpawnRegistryGuard::new(closure_id);
            // Guard drops here, should clean up the registry
        }

        // Verify the registry was cleaned up
        {
            let registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            assert!(
                !registry.contains_key(&closure_id),
                "Guard should have cleaned up registry entry on drop"
            );
        }
    }

    #[test]
    fn test_spawn_registry_guard_disarm() {
        // Test that disarming the guard prevents cleanup
        let closure_id = 54321;

        // Create a test closure environment
        let env: Box<[Value]> = vec![Value::Int(10), Value::Int(20)].into_boxed_slice();
        let fn_ptr: usize = 0x5678;

        // Insert into registry
        {
            let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            registry.insert(closure_id, (fn_ptr, env));
        }

        // Create a guard, disarm it, and let it drop
        {
            let mut guard = SpawnRegistryGuard::new(closure_id);
            guard.disarm();
            // Guard drops here, but should NOT clean up because it's disarmed
        }

        // Verify the registry entry is still there
        {
            let registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            assert!(
                registry.contains_key(&closure_id),
                "Disarmed guard should not clean up registry entry"
            );

            // Manual cleanup for this test
            drop(registry);
            let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            registry.remove(&closure_id);
        }
    }

    // Helper function for testing: a quotation that adds 1
    unsafe extern "C" fn add_one_quot(stack: Stack) -> Stack {
        unsafe {
            let stack = push_int(stack, 1);
            crate::arithmetic::add(stack)
        }
    }

    #[test]
    fn test_push_quotation() {
        unsafe {
            let stack: Stack = crate::stack::alloc_test_stack();

            // Push a quotation (for tests, wrapper and impl are the same C function)
            let fn_ptr = add_one_quot as usize;
            let stack = push_quotation(stack, fn_ptr, fn_ptr);

            // Verify it's on the stack
            let (_stack, value) = pop(stack);
            assert!(matches!(value, Value::Quotation { .. }));
        }
    }

    #[test]
    fn test_call_quotation() {
        unsafe {
            let stack: Stack = crate::stack::alloc_test_stack();

            // Push 5, then a quotation that adds 1
            let stack = push_int(stack, 5);
            let fn_ptr = add_one_quot as usize;
            let stack = push_quotation(stack, fn_ptr, fn_ptr);

            // Call the quotation
            let stack = call(stack);

            // Result should be 6
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(6));
        }
    }

    // Helper quotation for spawn test: does nothing, just completes
    unsafe extern "C" fn noop_quot(stack: Stack) -> Stack {
        stack
    }

    #[test]
    fn test_spawn_quotation() {
        unsafe {
            // Initialize scheduler
            crate::scheduler::scheduler_init();

            let stack: Stack = crate::stack::alloc_test_stack();

            // Push a quotation
            let fn_ptr = noop_quot as usize;
            let stack = push_quotation(stack, fn_ptr, fn_ptr);

            // Spawn it
            let stack = spawn(stack);

            // Should have strand ID on stack
            let (_stack, result) = pop(stack);
            match result {
                Value::Int(strand_id) => {
                    assert!(strand_id > 0, "Strand ID should be positive");
                }
                _ => panic!("Expected Int (strand ID), got {:?}", result),
            }

            // Wait for strand to complete
            crate::scheduler::wait_all_strands();
        }
    }
}
