//! A tiny work-stealing scheduler built from `Worker` and `Stealer`.
//!
//! Each thread owns a deque. It pushes the tasks it creates onto its own deque
//! and pops them newest-first, which keeps related work on one core. When its
//! deque is empty it steals the oldest task from another thread. Here the tasks
//! form a tree: each one splits into two smaller ones until it reaches a leaf.
//!
//! This is the structure `ThreadPool` is built on. Use `ThreadPool` and `join`
//! for real work; this example shows the deque on its own.
//!
//! Run with `cargo run --release --example scheduler`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;

use parkring::{Steal, Stealer, Worker};

/// A task covering `size` leaves.
struct Task {
    size: u64,
}

const THREADS: usize = 4;
const LEAVES: u64 = 1 << 20;

fn main() {
    let workers: Vec<Worker<Task>> = (0..THREADS).map(|_| Worker::new()).collect();
    let stealers: Vec<Stealer<Task>> = workers.iter().map(Worker::stealer).collect();
    // Tasks created but not yet finished. When it reaches zero, everyone stops.
    let pending = AtomicUsize::new(1);
    let leaves_done = AtomicU64::new(0);
    let stolen = AtomicUsize::new(0);

    workers[0].push(Task { size: LEAVES });

    thread::scope(|s| {
        for (index, worker) in workers.into_iter().enumerate() {
            let (stealers, pending, leaves_done, stolen) =
                (&stealers, &pending, &leaves_done, &stolen);
            s.spawn(move || {
                while pending.load(Ordering::Acquire) > 0 {
                    let Some(task) = worker.pop().or_else(|| steal(index, stealers, stolen)) else {
                        thread::yield_now();
                        continue;
                    };
                    if task.size == 1 {
                        leaves_done.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // One task becomes two: count them before the parent is retired.
                        pending.fetch_add(2, Ordering::AcqRel);
                        worker.push(Task {
                            size: task.size / 2,
                        });
                        worker.push(Task {
                            size: task.size - task.size / 2,
                        });
                    }
                    pending.fetch_sub(1, Ordering::AcqRel);
                }
            });
        }
    });

    assert_eq!(leaves_done.load(Ordering::Relaxed), LEAVES);
    println!(
        "{THREADS} threads ran {LEAVES} leaf tasks; {} tasks were stolen",
        stolen.load(Ordering::Relaxed)
    );
}

/// Tries every other thread's deque once, starting with the next one.
fn steal(me: usize, stealers: &[Stealer<Task>], stolen: &AtomicUsize) -> Option<Task> {
    for offset in 1..stealers.len() {
        let victim = &stealers[(me + offset) % stealers.len()];
        loop {
            match victim.steal() {
                Steal::Success(task) => {
                    stolen.fetch_add(1, Ordering::Relaxed);
                    return Some(task);
                }
                Steal::Empty => break,
                Steal::Retry => {}
            }
        }
    }
    None
}
