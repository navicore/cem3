//! Profile stack operations overhead
//! Measures: heap allocation costs of push/pop/pick operations
//!
//! Run with: rustc -O stack_ops_profile.rs && ./stack_ops_profile

use std::time::Instant;

/// Simulates a stack node
struct StackNode {
    value: i64,
    next: *mut StackNode,
}

type Stack = *mut StackNode;

/// Push a value onto the stack (allocates)
fn push(stack: Stack, value: i64) -> Stack {
    let node = Box::new(StackNode {
        value,
        next: stack,
    });
    Box::into_raw(node)
}

/// Pop a value from the stack (deallocates)
fn pop(stack: Stack) -> (Stack, i64) {
    assert!(!stack.is_null());
    unsafe {
        let node = Box::from_raw(stack);
        (node.next, node.value)
    }
}

/// Peek at top value (no allocation)
fn peek(stack: Stack) -> i64 {
    unsafe { (*stack).value }
}

/// Duplicate top value (allocates)
fn dup(stack: Stack) -> Stack {
    let value = peek(stack);
    push(stack, value)
}

/// Drop top value (deallocates)
fn drop_op(stack: Stack) -> Stack {
    let (rest, _) = pop(stack);
    rest
}

/// Swap top two values (2 alloc + 2 dealloc)
fn swap(stack: Stack) -> Stack {
    let (stack, a) = pop(stack);
    let (stack, b) = pop(stack);
    let stack = push(stack, a);
    push(stack, b)
}

/// Over: copy second element to top (1 alloc)
fn over(stack: Stack) -> Stack {
    let (stack, a) = pop(stack);
    let b = peek(stack);
    let stack = push(stack, a);
    push(stack, b)
}

/// Pick: copy nth element to top (1 alloc)
fn pick(stack: Stack, n: usize) -> Stack {
    let mut current = stack;
    for _ in 0..n {
        current = unsafe { (*current).next };
    }
    let value = unsafe { (*current).value };
    push(stack, value)
}

fn free_stack(mut stack: Stack) {
    while !stack.is_null() {
        let (next, _) = pop(stack);
        stack = next;
    }
}

fn main() {
    println!("=== Stack Operations Profile ===\n");

    // Count operations in skynet per branch node:
    // Looking at skynet.seq, each branch:
    // - Creates channel (1 push)
    // - 10x child spawns, each doing:
    //   - `3 pick 10 multiply 0 add` = 1 pick + several ops
    //   - `2 pick` = 1 pick
    //   - `swap 2 pick` = swap + pick
    //   - `drop drop drop` = 3 drops
    // - 10x receives with `over chan.receive add`
    // - Final cleanup

    // Conservative estimate: ~50 stack ops per branch node
    // With ~111K branch nodes = ~5.5M stack operations
    // Plus ~1M leaf nodes with ~5 ops each = ~5M more
    // Total: ~10M stack operations

    let n = 10_000_000;

    // Test 1: Push/Pop pairs (2 heap ops each)
    let start = Instant::now();
    for _ in 0..n {
        let stack = push(std::ptr::null_mut(), 42);
        drop_op(stack);
    }
    let push_pop_time = start.elapsed();
    println!("Push/Pop pairs ({}): {:?}", n, push_pop_time);
    println!("Per pair: {:?}", push_pop_time / n as u32);

    // Test 2: Pick operation (1 alloc + traverse)
    let mut stack: Stack = std::ptr::null_mut();
    for i in 0..10 {
        stack = push(stack, i);
    }

    let start = Instant::now();
    for _ in 0..n {
        let picked = pick(stack, 5);
        stack = drop_op(picked); // Remove the picked value
        stack = push(stack, 0); // Restore to 10 elements
    }
    let pick_time = start.elapsed();
    println!("\nPick(5) + drop + push ({}): {:?}", n, pick_time);
    println!("Per operation: {:?}", pick_time / n as u32);
    free_stack(stack);

    // Test 3: Swap operation (2 alloc + 2 dealloc)
    let mut stack = push(std::ptr::null_mut(), 1);
    stack = push(stack, 2);

    let start = Instant::now();
    for _ in 0..n {
        stack = swap(stack);
    }
    let swap_time = start.elapsed();
    println!("\nSwap ({}): {:?}", n, swap_time);
    println!("Per swap: {:?}", swap_time / n as u32);
    free_stack(stack);

    // Test 4: Over operation (1 alloc)
    let mut stack = push(std::ptr::null_mut(), 1);
    stack = push(stack, 2);

    let start = Instant::now();
    for _ in 0..n {
        stack = over(stack);
        stack = drop_op(stack); // Keep stack size constant
    }
    let over_time = start.elapsed();
    println!("\nOver + drop ({}): {:?}", n, over_time);
    println!("Per operation: {:?}", over_time / n as u32);
    free_stack(stack);

    // Test 5: Dup operation (1 alloc)
    let mut stack = push(std::ptr::null_mut(), 42);

    let start = Instant::now();
    for _ in 0..n {
        stack = dup(stack);
        stack = drop_op(stack);
    }
    let dup_time = start.elapsed();
    println!("\nDup + drop ({}): {:?}", n, dup_time);
    println!("Per operation: {:?}", dup_time / n as u32);
    free_stack(stack);

    // Estimate for skynet (10M ops, mix of types)
    // Weighted average assuming mix of operations
    let avg_time = (push_pop_time + pick_time + swap_time + over_time + dup_time) / 5;
    println!("\n=== Skynet Estimates (10M stack ops) ===");
    println!("Average per op: {:?}", avg_time / n as u32);
    println!("Estimated total: {:?}", avg_time);
    println!("Skynet actual: 4779ms");
    println!("Stack ops are {:.1}% of skynet", avg_time.as_secs_f64() / 4.779 * 100.0);
}
