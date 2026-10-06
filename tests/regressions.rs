//! Regression tests for bugs in the original submission. Each one failed (or
//! hung) against commit 76feaa4.

mod common;

use std::sync::Barrier;
use std::thread;

use common::scale;
#[cfg(target_pointer_width = "64")]
use parkring::ScqQueue;
use parkring::{BlockingQueue, LockFreeQueue, TryPushError};

/// Capacity 3 used to map positions 0,1,2 onto slots 0,1,0: three pushes were
/// accepted and none could be popped. Capacity 6 deadlocked on the 3rd push.
/// Capacity 1 is raised to 2 because a one-slot sequence ring cannot tell
/// "full" from "empty for the next lap".
#[test]
fn non_power_of_two_capacity_is_rounded_up() {
    for requested in [1, 3, 5, 6, 7, 100] {
        let q = LockFreeQueue::new(requested);
        let cap = q.capacity();
        assert_eq!(cap, requested.next_power_of_two().max(2));
        for lap in 0..3 {
            for i in 0..cap {
                q.try_push(lap * 1000 + i).unwrap();
            }
            assert!(q.try_push(0).unwrap_err().is_full());
            for i in 0..cap {
                assert_eq!(q.try_pop(), Ok(lap * 1000 + i));
            }
        }
    }
}

#[test]
fn blocking_capacity_is_exact() {
    let q = BlockingQueue::new(3);
    assert_eq!(q.capacity(), 3);
    for i in 0..3 {
        q.try_push(i).unwrap();
    }
    assert_eq!(q.try_push(3), Err(TryPushError::Full(3)));
}

/// `try_push` used to return `Err` whenever it lost the tail CAS or read a
/// stale position, even though the queue had room. The original code produced
/// ~65 spurious failures in this test; now there must be none.
#[test]
fn concurrent_try_push_never_fails_on_a_non_full_queue() {
    const THREADS: usize = 4;
    const PER_THREAD: usize = 8;
    for _ in 0..scale(2000) {
        let q = LockFreeQueue::new(THREADS * PER_THREAD * 2);
        let start = Barrier::new(THREADS);
        thread::scope(|s| {
            for _ in 0..THREADS {
                s.spawn(|| {
                    start.wait();
                    for i in 0..PER_THREAD {
                        q.try_push(i)
                            .expect("spurious failure: queue is never full");
                    }
                });
            }
        });
        assert_eq!(q.len(), THREADS * PER_THREAD);
    }
}

/// The mirror image for `try_pop` on a queue that never runs dry.
#[test]
fn concurrent_try_pop_never_fails_on_a_non_empty_queue() {
    const THREADS: usize = 4;
    const PER_THREAD: usize = 8;
    for _ in 0..scale(2000) {
        let q = LockFreeQueue::new(THREADS * PER_THREAD * 2);
        for i in 0..THREADS * PER_THREAD * 2 {
            q.try_push(i).unwrap();
        }
        let start = Barrier::new(THREADS);
        thread::scope(|s| {
            for _ in 0..THREADS {
                s.spawn(|| {
                    start.wait();
                    for _ in 0..PER_THREAD {
                        q.try_pop().expect("spurious failure: queue is never empty");
                    }
                });
            }
        });
        assert_eq!(q.len(), THREADS * PER_THREAD);
    }
}

/// SCQ's threshold could go negative while an item sat at `head` when more
/// threads than slots were active, stranding it: every dequeuer returned
/// Empty without claiming a position, and parked threads never woke. Hung in
/// about 1 run in 4 with capacity 1 and 3 producers + 3 consumers. A watchdog
/// turns a hang into a failure with the queue's state.
#[cfg(target_pointer_width = "64")]
#[test]
#[cfg_attr(miri, ignore = "needs many rounds to hit the race")]
fn scq_more_threads_than_capacity_never_strands_an_item() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::time::{Duration, Instant};

    for round in 0..200 {
        let q = ScqQueue::<u64>::new(1);
        let popped = AtomicUsize::new(0);
        let done = AtomicBool::new(false);
        thread::scope(|s| {
            let consumers: Vec<_> = (0..3)
                .map(|_| {
                    s.spawn(|| {
                        while q.pop().is_ok() {
                            popped.fetch_add(1, SeqCst);
                        }
                    })
                })
                .collect();
            s.spawn(|| {
                let start = Instant::now();
                while !done.load(SeqCst) {
                    assert!(
                        start.elapsed() < Duration::from_secs(20),
                        "round {round} hung: popped {} of 3000, {q:?}",
                        popped.load(SeqCst)
                    );
                    thread::sleep(Duration::from_millis(10));
                }
            });
            thread::scope(|ps| {
                for _ in 0..3 {
                    ps.spawn(|| (0..1000).for_each(|i| q.push(i).unwrap()));
                }
            });
            q.close();
            for c in consumers {
                c.join().unwrap();
            }
            done.store(true, SeqCst);
        });
        assert_eq!(popped.load(SeqCst), 3000);
    }
}
