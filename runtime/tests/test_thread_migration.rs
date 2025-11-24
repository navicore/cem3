//! Test whether May coroutines migrate between OS threads
//!
//! This test logs OS thread IDs before and after yields to determine
//! if May's scheduler actually migrates coroutines or keeps them pinned.

use seq_runtime::{Stack, scheduler_init, scheduler_run, strand_spawn};
use std::sync::Mutex;

static THREAD_LOG: Mutex<Vec<(i64, std::thread::ThreadId, &'static str)>> = Mutex::new(Vec::new());

fn log_thread(strand_id: i64, event: &'static str) {
    let tid = std::thread::current().id();
    THREAD_LOG.lock().unwrap().push((strand_id, tid, event));
}

extern "C" fn yielding_strand(stack: Stack) -> Stack {
    let strand_id = 1;

    log_thread(strand_id, "before_yield_1");
    may::coroutine::yield_now();
    log_thread(strand_id, "after_yield_1");

    may::coroutine::yield_now();
    log_thread(strand_id, "after_yield_2");

    may::coroutine::yield_now();
    log_thread(strand_id, "after_yield_3");

    stack
}

extern "C" fn spawning_strand(stack: Stack) -> Stack {
    let strand_id = 2;

    log_thread(strand_id, "before_spawn");

    // Spawn another coroutine to create scheduling pressure
    unsafe {
        strand_spawn(yielding_strand, std::ptr::null_mut());
    }

    log_thread(strand_id, "after_spawn");
    may::coroutine::yield_now();
    log_thread(strand_id, "after_yield");

    stack
}

#[test]
fn test_coroutine_thread_migration() {
    unsafe {
        scheduler_init();

        // Spawn multiple strands to create scheduling pressure
        for _ in 0..10 {
            strand_spawn(yielding_strand, std::ptr::null_mut());
        }
        strand_spawn(spawning_strand, std::ptr::null_mut());

        scheduler_run();
    }

    // Analyze thread IDs
    let log = THREAD_LOG.lock().unwrap();
    println!("\nThread Migration Analysis:");
    println!("{:<10} {:<20} {:<20}", "Strand", "Thread ID", "Event");
    println!("{:-<50}", "");

    for (strand_id, tid, event) in log.iter() {
        println!("{:<10} {:?} {:<20}", strand_id, tid, event);
    }

    // Check if any strand executed on different threads
    let mut strand_threads: std::collections::HashMap<i64, Vec<std::thread::ThreadId>> =
        std::collections::HashMap::new();

    for (strand_id, tid, _) in log.iter() {
        strand_threads.entry(*strand_id).or_default().push(*tid);
    }

    let mut migration_detected = false;
    for (strand_id, tids) in strand_threads.iter() {
        let unique_threads: std::collections::HashSet<_> = tids.iter().collect();
        if unique_threads.len() > 1 {
            println!(
                "\n⚠️  MIGRATION DETECTED: Strand {} ran on {} different OS threads!",
                strand_id,
                unique_threads.len()
            );
            migration_detected = true;
        }
    }

    if !migration_detected {
        println!("\n✅ NO MIGRATION: All coroutines stayed on their original OS threads");
    }
}
