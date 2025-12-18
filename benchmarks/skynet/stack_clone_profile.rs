//! Profile stack cloning overhead
//! Measures: heap allocation costs of cloning stack nodes
//!
//! Run with: rustc -O stack_clone_profile.rs && ./stack_clone_profile

use std::time::Instant;

/// Simulates a stack node (matches StackNode structure)
struct StackNode {
    value: i64,
    next: *mut StackNode,
}

/// Clone a stack using Box allocations (simulates clone_stack)
fn clone_stack(stack: *mut StackNode) -> *mut StackNode {
    if stack.is_null() {
        return std::ptr::null_mut();
    }

    // Collect values
    let mut values = Vec::new();
    let mut current = stack;
    while !current.is_null() {
        unsafe {
            values.push((*current).value);
            current = (*current).next;
        }
    }

    // Build new stack using Box allocation
    let mut new_stack: *mut StackNode = std::ptr::null_mut();
    for value in values.into_iter().rev() {
        let node = Box::new(StackNode {
            value,
            next: new_stack,
        });
        new_stack = Box::into_raw(node);
    }

    new_stack
}

/// Free a stack
fn free_stack(mut stack: *mut StackNode) {
    while !stack.is_null() {
        unsafe {
            let next = (*stack).next;
            drop(Box::from_raw(stack));
            stack = next;
        }
    }
}

/// Build a stack of given depth
fn build_stack(depth: usize) -> *mut StackNode {
    let mut stack: *mut StackNode = std::ptr::null_mut();
    for i in 0..depth {
        let node = Box::new(StackNode {
            value: i as i64,
            next: stack,
        });
        stack = Box::into_raw(node);
    }
    stack
}

fn main() {
    let n = 1_000_000; // Number of spawns to simulate
    let stack_depth = 8; // Typical stack depth at spawn in skynet

    println!("=== Stack Cloning Overhead Profile ===\n");

    // Test 1: Just heap allocations (what clone_stack does internally)
    let start = Instant::now();
    for _ in 0..n {
        // Simulate allocating 8 stack nodes (typical skynet spawn)
        let stack = build_stack(stack_depth);
        free_stack(stack);
    }
    let alloc_time = start.elapsed();
    println!("Heap alloc/free ({} stacks, {} nodes each): {:?}",
             n, stack_depth, alloc_time);
    println!("  = {} allocations total", n * stack_depth);
    println!("  = {:?} per allocation", alloc_time / (n * stack_depth) as u32);

    // Test 2: Full clone_stack (what spawn actually does)
    let source_stack = build_stack(stack_depth);
    let start = Instant::now();
    for _ in 0..n {
        let cloned = clone_stack(source_stack);
        free_stack(cloned);
    }
    let clone_time = start.elapsed();
    free_stack(source_stack);
    println!("\nclone_stack ({} clones, {} nodes each): {:?}",
             n, stack_depth, clone_time);
    println!("  = {:?} per spawn", clone_time / n as u32);

    // Test 3: Just Vec::new + push (collection overhead)
    let start = Instant::now();
    for _ in 0..n {
        let mut v: Vec<i64> = Vec::new();
        for i in 0..stack_depth {
            v.push(i as i64);
        }
        drop(v);
    }
    let vec_time = start.elapsed();
    println!("\nVec allocation ({} vecs, {} items each): {:?}",
             n, stack_depth, vec_time);

    // Compare to skynet total time
    let skynet_total = std::time::Duration::from_millis(4779); // from benchmark
    println!("\n=== Comparison to Skynet Benchmark ===");
    println!("Skynet total time: {:?}", skynet_total);
    println!("clone_stack time: {:?} ({:.1}% of skynet)",
             clone_time,
             clone_time.as_secs_f64() / skynet_total.as_secs_f64() * 100.0);

    // Test 4: What if we didn't clone at all? (baseline)
    let start = Instant::now();
    for _ in 0..n {
        // Just the overhead of calling a function
        std::hint::black_box(0);
    }
    let baseline = start.elapsed();
    println!("\nBaseline (1M function calls): {:?}", baseline);
}
