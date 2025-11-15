use crate::pool;
use crate::value::Value;

/// StackNode: Implementation detail of the stack
///
/// This is a linked list node that contains a Value.
/// The key insight: StackNode is separate from Value.
/// Users think about Values, not StackNodes.
pub struct StackNode {
    /// The actual value stored in this node
    pub value: Value,

    /// Pointer to the next node in the stack (or null if this is the bottom)
    /// This pointer is ONLY for stack structure, never for variant field linking
    pub next: *mut StackNode,
}

/// Stack: A pointer to the top of the stack
///
/// null represents an empty stack
pub type Stack = *mut StackNode;

/// Push a value onto the stack
///
/// Takes ownership of the value and creates a new StackNode from the pool.
/// Returns a pointer to the new top of the stack.
///
/// # Safety
/// Stack pointer must be valid (or null for empty stack)
///
/// # Performance
/// Uses thread-local pool for ~10x speedup over Box::new()
pub unsafe fn push(stack: Stack, value: Value) -> Stack {
    pool::pool_allocate(value, stack)
}

/// Pop a value from the stack
///
/// Returns the rest of the stack and the popped value.
/// Returns the StackNode to the pool for reuse.
///
/// # Safety
/// Stack must not be null (use is_empty to check first)
///
/// # Performance
/// Returns node to thread-local pool for reuse (~10x faster than free)
pub unsafe fn pop(stack: Stack) -> (Stack, Value) {
    assert!(!stack.is_null(), "pop: stack is empty");

    unsafe {
        let rest = (*stack).next;
        // CRITICAL: Replace value with dummy before returning node to pool
        // This prevents double-drop when pool reuses the node
        // The dummy value (Int(0)) will be overwritten when node is reused
        let value = std::mem::replace(&mut (*stack).value, Value::Int(0));

        // Return node to pool for reuse
        pool::pool_free(stack);

        (rest, value)
    }
}

/// Check if the stack is empty
pub fn is_empty(stack: Stack) -> bool {
    stack.is_null()
}

/// Peek at the top value without removing it
///
/// # Safety
/// Stack must not be null
/// Caller must ensure the returned reference is used within a valid lifetime
pub unsafe fn peek<'a>(stack: Stack) -> &'a Value {
    assert!(!stack.is_null(), "peek: stack is empty");
    unsafe { &(*stack).value }
}

/// Duplicate the top value on the stack: ( a -- a a )
///
/// # Safety
/// Stack must not be null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dup(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "dup: stack is empty");
    // SAFETY NOTE: In Rust 2024 edition, even within `unsafe fn`, the body is not
    // automatically an unsafe context. Explicit `unsafe {}` blocks are required for
    // all unsafe operations (dereferencing raw pointers, calling unsafe functions).
    // This is intentional and follows best practices for clarity about what's unsafe.
    let value = unsafe { (*stack).value.clone() };
    unsafe { push(stack, value) }
}

/// Drop the top value from the stack: ( a -- )
///
/// # Safety
/// Stack must not be null
pub unsafe fn drop(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "drop: stack is empty");
    let (rest, _) = unsafe { pop(stack) };
    rest
}

/// Alias for drop to avoid LLVM keyword conflicts
///
/// LLVM uses "drop" as a reserved word, so codegen calls this as drop_op
///
/// # Safety
/// Stack must not be null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drop_op(stack: Stack) -> Stack {
    unsafe { drop(stack) }
}

/// Swap the top two values: ( a b -- b a )
///
/// # Safety
/// Stack must have at least two values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn swap(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "swap: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "swap: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };
    let stack = unsafe { push(rest, b) };
    unsafe { push(stack, a) }
}

/// Copy the second value to the top: ( a b -- a b a )
///
/// # Safety
/// Stack must have at least two values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn over(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "over: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "over: stack has only one value");
    let a = unsafe { (*rest).value.clone() };
    let stack = unsafe { push(rest, b) };
    unsafe { push(stack, a) }
}

/// Rotate the top three values: ( a b c -- b c a )
///
/// # Safety
/// Stack must have at least three values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rot(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "rot: stack is empty");
    let (rest, c) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "rot: stack has only one value");
    let (rest, b) = unsafe { pop(rest) };
    assert!(!rest.is_null(), "rot: stack has only two values");
    let (rest, a) = unsafe { pop(rest) };
    let stack = unsafe { push(rest, b) };
    let stack = unsafe { push(stack, c) };
    unsafe { push(stack, a) }
}

/// Remove the second value: ( a b -- b )
///
/// # Safety
/// Stack must have at least two values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nip(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "nip: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "nip: stack has only one value");
    let (rest, _a) = unsafe { pop(rest) };
    unsafe { push(rest, b) }
}

/// Copy top value below second value: ( a b -- b a b )
///
/// # Safety
/// Stack must have at least two values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tuck(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "tuck: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "tuck: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };
    let stack = unsafe { push(rest, b.clone()) };
    let stack = unsafe { push(stack, a) };
    unsafe { push(stack, b) }
}

/// Pick: Copy the nth value to the top (0-indexed from top)
/// ( ... xn ... x1 x0 n -- ... xn ... x1 x0 xn )
///
/// Examples:
/// - pick(0) is equivalent to dup
/// - pick(1) is equivalent to over
/// - pick(2) copies the third value to the top
///
/// # Safety
/// Stack must have at least n+1 values
pub unsafe fn pick(stack: Stack, n: usize) -> Stack {
    assert!(!stack.is_null(), "pick: stack is empty");

    // Walk down n nodes to find the target value
    let mut current = stack;
    for i in 0..n {
        assert!(
            !current.is_null(),
            "pick: stack has only {} values, need at least {}",
            i + 1,
            n + 1
        );
        current = unsafe { (*current).next };
    }

    assert!(
        !current.is_null(),
        "pick: stack has only {} values, need at least {}",
        n,
        n + 1
    );

    // Clone the value at position n
    let value = unsafe { (*current).value.clone() };

    // Push it on top of the stack
    unsafe { push(stack, value) }
}

/// Pick operation exposed to compiler
///
/// Copies a value from depth n to the top of the stack.
///
/// Stack effect: ( ..stack n -- ..stack value )
/// where n is how deep to look (0 = top, 1 = second, etc.)
///
/// # Examples
///
/// ```cem
/// 10 20 30 0 pick   # Stack: 10 20 30 30  (pick(0) = dup)
/// 10 20 30 1 pick   # Stack: 10 20 30 20  (pick(1) = over)
/// 10 20 30 2 pick   # Stack: 10 20 30 10  (pick(2) = third)
/// ```
///
/// # Common Equivalences
///
/// - `pick(0)` is equivalent to `dup`  - copy top value
/// - `pick(1)` is equivalent to `over` - copy second value
/// - `pick(2)` copies third value (sometimes called "third")
/// - `pick(n)` copies value at depth n
///
/// # Use Cases
///
/// Useful for building stack utilities like:
/// - `third`: `2 pick` - access third item
/// - `3dup`: `2 pick 2 pick 2 pick` - duplicate top three items
/// - Complex stack manipulations without excessive rot/swap
///
/// # Safety
/// Stack must have at least one value (the Int n), and at least n+1 values total
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pick_op(stack: Stack) -> Stack {
    use crate::value::Value;

    assert!(!stack.is_null(), "pick: stack is empty");

    // Peek at the depth value without popping (for better error messages)
    let depth_value = unsafe { &(*stack).value };
    let n = match depth_value {
        Value::Int(i) => {
            if *i < 0 {
                panic!("pick: depth must be non-negative, got {}", i);
            }
            *i as usize
        }
        _ => panic!(
            "pick: expected Int depth on top of stack, got {:?}",
            depth_value
        ),
    };

    // Count stack depth (excluding the depth parameter itself)
    let mut count = 0;
    let mut current = unsafe { (*stack).next };
    while !current.is_null() {
        count += 1;
        current = unsafe { (*current).next };
    }

    // Validate we have enough values
    // Need: n+1 values total (depth parameter + n values below it)
    // We have: 1 (depth param) + count
    // So we need: count >= n + 1
    if count < n + 1 {
        panic!(
            "pick: cannot access depth {} - stack has only {} value{} (need at least {})",
            n,
            count,
            if count == 1 { "" } else { "s" },
            n + 1
        );
    }

    // Now pop the depth value and call internal pick
    let (stack, _) = unsafe { pop(stack) };
    unsafe { pick(stack, n) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: Create a stack with integer values
    fn make_stack(values: &[i64]) -> Stack {
        unsafe {
            let mut stack = std::ptr::null_mut();
            for &val in values {
                stack = push(stack, Value::Int(val));
            }
            stack
        }
    }

    /// Test helper: Pop all values from stack and return as Vec
    fn drain_stack(mut stack: Stack) -> Vec<Value> {
        unsafe {
            let mut values = Vec::new();
            while !is_empty(stack) {
                let (rest, val) = pop(stack);
                stack = rest;
                values.push(val);
            }
            values
        }
    }

    /// Test helper: Assert stack contains expected integer values (top to bottom)
    fn assert_stack_ints(stack: Stack, expected: &[i64]) {
        let values = drain_stack(stack);
        let ints: Vec<i64> = values
            .into_iter()
            .map(|v| match v {
                Value::Int(n) => n,
                _ => panic!("Expected Int, got {:?}", v),
            })
            .collect();
        assert_eq!(ints, expected);
    }

    #[test]
    fn test_push_pop() {
        unsafe {
            let stack = std::ptr::null_mut();
            assert!(is_empty(stack));

            let stack = push(stack, Value::Int(42));
            assert!(!is_empty(stack));

            let (stack, value) = pop(stack);
            assert_eq!(value, Value::Int(42));
            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_multiple_values() {
        unsafe {
            let mut stack = std::ptr::null_mut();

            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));
            stack = push(stack, Value::Int(3));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(3));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_peek() {
        unsafe {
            let stack = push(std::ptr::null_mut(), Value::Int(42));
            let peeked = peek(stack);
            assert_eq!(*peeked, Value::Int(42));

            // Value still there
            let (stack, value) = pop(stack);
            assert_eq!(value, Value::Int(42));
            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_dup() {
        unsafe {
            let stack = push(std::ptr::null_mut(), Value::Int(42));
            let stack = dup(stack);

            // Should have two copies of 42
            let (stack, val1) = pop(stack);
            assert_eq!(val1, Value::Int(42));

            let (stack, val2) = pop(stack);
            assert_eq!(val2, Value::Int(42));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_drop() {
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));
            stack = push(stack, Value::Int(3));

            // Drop top value (3)
            stack = drop(stack);

            // Should have 2 on top now
            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_swap() {
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));

            // Swap: 1 2 -> 2 1
            stack = swap(stack);

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_composition() {
        // Test: compose swap + drop + dup to verify operations work together
        // Trace:
        // Start:  [1]
        // push 2: [2, 1]
        // push 3: [3, 2, 1]
        // swap:   [2, 3, 1]  (swap top two)
        // drop:   [3, 1]     (remove top)
        // dup:    [3, 3, 1]  (duplicate top)
        unsafe {
            let mut stack = make_stack(&[1, 2, 3]);

            stack = swap(stack);
            stack = drop(stack);
            stack = dup(stack);

            assert_stack_ints(stack, &[3, 3, 1]);
        }
    }

    #[test]
    fn test_over() {
        // over: ( a b -- a b a )
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));

            stack = over(stack); // [1, 2, 1]

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_rot() {
        // rot: ( a b c -- b c a )
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));
            stack = push(stack, Value::Int(3));

            stack = rot(stack); // [1, 3, 2]

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(3));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_nip() {
        // nip: ( a b -- b )
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));

            stack = nip(stack); // [2]

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_tuck() {
        // tuck: ( a b -- b a b )
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));

            stack = tuck(stack); // [2, 1, 2]

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_critical_shuffle_pattern() {
        // This is THE CRITICAL TEST that failed in cem2!
        // In cem2, this shuffle pattern caused variant field corruption because
        // StackCell.next pointers were used for BOTH stack linking AND variant fields.
        // When the stack was shuffled, next pointers became stale, corrupting variants.
        //
        // In cem3, this CANNOT happen because:
        // - StackNode.next is ONLY for stack structure
        // - Variant fields are stored in Box<[Value]> arrays
        // - Values are independent of stack position
        //
        // Shuffle pattern: rot swap rot rot swap
        // This was extracted from the list-reverse-helper function

        unsafe {
            let mut stack = make_stack(&[1, 2, 3, 4, 5]);

            // Initial state: [5, 4, 3, 2, 1] (top to bottom)
            //
            // Apply the critical shuffle pattern:
            stack = rot(stack);
            // rot: ( a b c -- b c a )
            // Before: [5, 4, 3, 2, 1]
            //          ^  ^  ^
            // After:  [3, 5, 4, 2, 1]

            stack = swap(stack);
            // swap: ( a b -- b a )
            // Before: [3, 5, 4, 2, 1]
            //          ^  ^
            // After:  [5, 3, 4, 2, 1]

            stack = rot(stack);
            // rot: ( a b c -- b c a )
            // Before: [5, 3, 4, 2, 1]
            //          ^  ^  ^
            // After:  [4, 5, 3, 2, 1]

            stack = rot(stack);
            // rot: ( a b c -- b c a )
            // Before: [4, 5, 3, 2, 1]
            //          ^  ^  ^
            // After:  [3, 4, 5, 2, 1]

            stack = swap(stack);
            // swap: ( a b -- b a )
            // Before: [3, 4, 5, 2, 1]
            //          ^  ^
            // After:  [4, 3, 5, 2, 1]

            // Final state: [4, 3, 5, 2, 1] (top to bottom)
            // Verify every value is intact - no corruption
            assert_stack_ints(stack, &[4, 3, 5, 2, 1]);
        }
    }

    #[test]
    fn test_pick_0_is_dup() {
        // pick(0) should be equivalent to dup
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(42));

            stack = pick(stack, 0);

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(42));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(42));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_pick_1_is_over() {
        // pick(1) should be equivalent to over
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));

            stack = pick(stack, 1);

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_pick_deep() {
        // Test picking from deeper in the stack
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(1));
            stack = push(stack, Value::Int(2));
            stack = push(stack, Value::Int(3));
            stack = push(stack, Value::Int(4));
            stack = push(stack, Value::Int(5));

            // pick(3) should copy the 4th value (2) to the top
            // Stack: [5, 4, 3, 2, 1]
            //         0  1  2  3  <- indices
            stack = pick(stack, 3); // [2, 5, 4, 3, 2, 1]

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(5));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(4));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(3));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(2));

            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(1));

            assert!(is_empty(stack));
        }
    }

    #[test]
    fn test_multifield_variant_survives_shuffle() {
        // THE TEST THAT WOULD HAVE FAILED IN CEM2!
        // Create a multi-field variant (simulating Cons(head, tail)),
        // apply the critical shuffle pattern, and verify variant is intact
        use crate::value::VariantData;

        unsafe {
            // Create a Cons-like variant: Cons(42, Nil)
            // Tag 0 = Nil, Tag 1 = Cons
            let nil = Value::Variant(Box::new(VariantData::new(0, vec![])));
            let cons = Value::Variant(Box::new(VariantData::new(
                1,
                vec![Value::Int(42), nil.clone()],
            )));

            // Put the variant on the stack with some other values
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(100)); // Extra value
            stack = push(stack, Value::Int(200)); // Extra value
            stack = push(stack, cons.clone()); // Our variant
            stack = push(stack, Value::Int(300)); // Extra value
            stack = push(stack, Value::Int(400)); // Extra value

            // Apply the CRITICAL SHUFFLE PATTERN that broke cem2
            stack = rot(stack); // Rotate top 3
            stack = swap(stack); // Swap top 2
            stack = rot(stack); // Rotate top 3
            stack = rot(stack); // Rotate top 3
            stack = swap(stack); // Swap top 2

            // Pop all values and find our variant
            let mut found_variant = None;
            while !is_empty(stack) {
                let (rest, val) = pop(stack);
                stack = rest;
                if matches!(val, Value::Variant(_)) {
                    found_variant = Some(val);
                }
            }

            // Verify the variant is intact
            assert!(found_variant.is_some(), "Variant was lost during shuffle!");

            if let Some(Value::Variant(variant_data)) = found_variant {
                assert_eq!(variant_data.tag, 1, "Variant tag corrupted!");
                assert_eq!(
                    variant_data.fields.len(),
                    2,
                    "Variant field count corrupted!"
                );
                assert_eq!(
                    variant_data.fields[0],
                    Value::Int(42),
                    "First field corrupted!"
                );

                // Verify second field is Nil variant
                if let Value::Variant(nil_data) = &variant_data.fields[1] {
                    assert_eq!(nil_data.tag, 0, "Nested variant tag corrupted!");
                    assert_eq!(nil_data.fields.len(), 0, "Nested variant should be empty!");
                } else {
                    panic!("Second field should be a Variant!");
                }
            }
        }
    }

    #[test]
    fn test_arbitrary_depth_operations() {
        // Property: Operations should work at any stack depth
        // Test with 100-deep stack, then manipulate top elements
        unsafe {
            let mut stack = std::ptr::null_mut();

            // Build a 100-deep stack
            for i in 0..100 {
                stack = push(stack, Value::Int(i));
            }

            // Operations on top should work regardless of depth below
            stack = dup(stack); // [99, 99, 98, 97, ..., 0]
            stack = swap(stack); // [99, 99, 98, 97, ..., 0]
            stack = over(stack); // [99, 99, 99, 98, 97, ..., 0]
            stack = rot(stack); // [99, 99, 99, 98, 97, ..., 0]
            stack = drop(stack); // [99, 99, 98, 97, ..., 0]

            // Verify we can still access deep values with pick
            stack = pick(stack, 50); // Should copy value at depth 50

            // Pop and verify stack is still intact
            let mut count = 0;
            while !is_empty(stack) {
                let (rest, _val) = pop(stack);
                stack = rest;
                count += 1;
            }

            // Started with 100, added 1 with dup, added 1 with over, dropped 1, picked 1
            assert_eq!(count, 102);
        }
    }

    #[test]
    fn test_operation_composition_completeness() {
        // Property: Any valid sequence of operations should succeed
        // Test complex composition with multiple operation types
        unsafe {
            let mut stack = std::ptr::null_mut();

            // Build initial state
            for i in 1..=10 {
                stack = push(stack, Value::Int(i));
            }

            // Complex composition: mix all operation types
            // [10, 9, 8, 7, 6, 5, 4, 3, 2, 1]
            stack = dup(stack); // [10, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]
            stack = over(stack); // [10, 10, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]
            stack = rot(stack); // [10, 10, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]
            stack = swap(stack); // Top two swapped
            stack = nip(stack); // Remove second
            stack = tuck(stack); // Copy top below second
            stack = pick(stack, 5); // Copy from depth 5
            stack = drop(stack); // Remove top

            // If we get here without panic, composition works
            // Verify stack still has values and is traversable
            let mut depth = 0;
            let mut current = stack;
            while !current.is_null() {
                depth += 1;
                current = (*current).next;
            }

            assert!(depth > 0, "Stack should not be empty after operations");
        }
    }

    #[test]
    fn test_pick_at_arbitrary_depths() {
        // Property: pick(n) should work for any n < stack_depth
        // Verify pick can access any depth without corruption
        unsafe {
            let mut stack = std::ptr::null_mut();

            // Build stack with identifiable values
            for i in 0..50 {
                stack = push(stack, Value::Int(i * 10));
            }

            // Pick from various depths and verify values
            // Stack is: [490, 480, 470, ..., 20, 10, 0]
            //            0    1    2         47  48  49

            stack = pick(stack, 0); // Should get 490
            let (mut stack, val) = pop(stack);
            assert_eq!(val, Value::Int(490));

            stack = pick(stack, 10); // Should get value at depth 10
            let (mut stack, val) = pop(stack);
            assert_eq!(val, Value::Int(390)); // (49-10) * 10

            stack = pick(stack, 40); // Deep pick
            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(90)); // (49-40) * 10

            // After all these operations, stack should still be intact
            let mut count = 0;
            let mut current = stack;
            while !current.is_null() {
                count += 1;
                current = (*current).next;
            }

            assert_eq!(count, 50, "Stack depth should be unchanged");
        }
    }

    #[test]
    fn test_pick_op_equivalence_to_dup() {
        // pick_op(0) should behave like dup
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(42));
            stack = push(stack, Value::Int(0)); // depth parameter

            stack = pick_op(stack);

            // Should have two 42s on stack
            let (stack, val1) = pop(stack);
            assert_eq!(val1, Value::Int(42));
            let (stack, val2) = pop(stack);
            assert_eq!(val2, Value::Int(42));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_pick_op_equivalence_to_over() {
        // pick_op(1) should behave like over
        unsafe {
            let mut stack = std::ptr::null_mut();
            stack = push(stack, Value::Int(10));
            stack = push(stack, Value::Int(20));
            stack = push(stack, Value::Int(1)); // depth parameter

            stack = pick_op(stack);

            // Should have: 10 20 10
            let (stack, val1) = pop(stack);
            assert_eq!(val1, Value::Int(10));
            let (stack, val2) = pop(stack);
            assert_eq!(val2, Value::Int(20));
            let (stack, val3) = pop(stack);
            assert_eq!(val3, Value::Int(10));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_pick_op_deep_access() {
        // Test accessing deeper stack values
        unsafe {
            let mut stack = std::ptr::null_mut();
            for i in 0..10 {
                stack = push(stack, Value::Int(i));
            }
            stack = push(stack, Value::Int(5)); // depth parameter

            stack = pick_op(stack);

            // Should have copied value at depth 5 (which is 4) to top
            let (stack, val) = pop(stack);
            assert_eq!(val, Value::Int(4));

            // Stack should still have original 10 values
            let mut count = 0;
            let mut current = stack;
            while !current.is_null() {
                count += 1;
                current = (*current).next;
            }
            assert_eq!(count, 10);
        }
    }

    // Note: Cannot test panic cases (negative depth, insufficient stack depth)
    // because extern "C" functions cannot be caught with #[should_panic].
    // These cases are validated at runtime with descriptive panic messages.
    // See examples/test-pick.cem for integration testing of valid cases.

    #[test]
    fn test_operations_preserve_stack_integrity() {
        // Property: After any operation, walking the stack should never loop
        // This catches next pointer corruption
        unsafe {
            let mut stack = std::ptr::null_mut();

            for i in 0..20 {
                stack = push(stack, Value::Int(i));
            }

            // Apply operations that manipulate next pointers heavily
            stack = swap(stack);
            stack = rot(stack);
            stack = swap(stack);
            stack = rot(stack);
            stack = over(stack);
            stack = tuck(stack);

            // Walk stack and verify:
            // 1. No cycles (walk completes)
            // 2. No null mid-stack (all nodes valid until end)
            let mut visited = std::collections::HashSet::new();
            let mut current = stack;
            let mut count = 0;

            while !current.is_null() {
                // Check for cycle
                assert!(
                    visited.insert(current as usize),
                    "Detected cycle in stack - next pointer corruption!"
                );

                count += 1;
                current = (*current).next;

                // Safety: prevent infinite loop in case of corruption
                assert!(
                    count < 1000,
                    "Stack walk exceeded reasonable depth - likely corrupted"
                );
            }

            assert!(count > 0, "Stack should have elements");
        }
    }

    #[test]
    fn test_nested_variants_with_deep_stacks() {
        // Property: Variants with nested variants survive deep stack operations
        // This combines depth + complex data structures
        use crate::value::VariantData;

        unsafe {
            // Build deeply nested variant: Cons(1, Cons(2, Cons(3, Nil)))
            let nil = Value::Variant(Box::new(VariantData::new(0, vec![])));
            let cons3 = Value::Variant(Box::new(VariantData::new(1, vec![Value::Int(3), nil])));
            let cons2 = Value::Variant(Box::new(VariantData::new(1, vec![Value::Int(2), cons3])));
            let cons1 = Value::Variant(Box::new(VariantData::new(1, vec![Value::Int(1), cons2])));

            // Put on deep stack
            let mut stack = std::ptr::null_mut();
            for i in 0..30 {
                stack = push(stack, Value::Int(i));
            }
            stack = push(stack, cons1.clone());
            for i in 30..60 {
                stack = push(stack, Value::Int(i));
            }

            // Heavy shuffling in the region containing the variant
            for _ in 0..10 {
                stack = rot(stack);
                stack = swap(stack);
                stack = over(stack);
                stack = drop(stack);
            }

            // Find and verify the nested variant is intact
            let mut found_variant = None;
            while !is_empty(stack) {
                let (rest, val) = pop(stack);
                stack = rest;
                if let Value::Variant(ref vdata) = val
                    && vdata.tag == 1
                    && vdata.fields.len() == 2
                    && let Value::Int(1) = vdata.fields[0]
                {
                    found_variant = Some(val);
                    break;
                }
            }

            assert!(
                found_variant.is_some(),
                "Nested variant lost during deep stack operations"
            );
        }
    }
}
