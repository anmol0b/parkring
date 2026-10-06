//! Every item is dropped exactly once: by the consumer that pops it, by the
//! caller that gets it back in an error, or by the queue's own `Drop`.
#![allow(clippy::cast_possible_truncation)] // small test counts

mod common;

use std::sync::Arc;
use std::thread;

use common::drop_counter::{DropStats, Tracked};
use common::{TestQueue, scale};
#[cfg(target_pointer_width = "64")]
use parkring::ScqQueue;
use parkring::{BlockingQueue, LockFreeQueue};

fn empty_queue_drops_nothing<Q: TestQueue<Tracked>>() {
    let stats = DropStats::new();
    drop(Q::with_capacity(8));
    assert_eq!(stats.created(), 0);
}

fn partially_full_queue_drops_its_items<Q: TestQueue<Tracked>>() {
    let stats = DropStats::new();
    let q = Q::with_capacity(8);
    for i in 0..5 {
        q.push(Tracked::new(i, &stats)).unwrap();
    }
    drop(q.pop().unwrap());
    assert_eq!(stats.live(), 4);
    drop(q);
    assert_eq!(stats.live(), 0);
    assert_eq!(stats.dropped(), 5);
}

/// After several laps the live items straddle the end of the ring, so `Drop`
/// must follow head..tail rather than scanning slots.
fn wrapped_queue_drops_exactly_live_items<Q: TestQueue<Tracked>>() {
    let stats = DropStats::new();
    let q = Q::with_capacity(4);
    let cap = q.capacity() as u64;
    for lap in 0..3 {
        for i in 0..cap {
            q.push(Tracked::new(lap * 100 + i, &stats)).unwrap();
        }
        for _ in 0..cap {
            drop(q.pop().unwrap());
        }
    }
    for i in 0..cap - 1 {
        q.push(Tracked::new(i, &stats)).unwrap();
    }
    drop(q.pop().unwrap());
    q.push(Tracked::new(999, &stats)).unwrap();
    assert_eq!(stats.live(), cap as usize - 1);
    drop(q);
    assert_eq!(stats.live(), 0);
}

fn rejected_items_are_returned_not_dropped<Q: TestQueue<Tracked>>() {
    let stats = DropStats::new();
    let q = Q::with_capacity(1);
    for i in 0..q.capacity() as u64 {
        q.push(Tracked::new(i, &stats)).unwrap();
    }
    let rejected = q
        .try_push(Tracked::new(42, &stats))
        .unwrap_err()
        .into_inner();
    assert_eq!(rejected.id, 42);
    assert_eq!(stats.dropped(), 0);
    drop(rejected);
    q.close();
    let rejected = q.push(Tracked::new(43, &stats)).unwrap_err().into_inner();
    assert_eq!(rejected.id, 43);
    assert_eq!(stats.dropped(), 1);
    drop((rejected, q));
    assert_eq!(stats.live(), 0);
}

fn closed_queue_with_items_drops_them<Q: TestQueue<Tracked>>() {
    let stats = DropStats::new();
    let q = Q::with_capacity(8);
    for i in 0..6 {
        q.push(Tracked::new(i, &stats)).unwrap();
    }
    q.close();
    drop(q.pop().unwrap());
    drop(q);
    assert_eq!(stats.live(), 0);
    assert_eq!(stats.dropped(), 6);
}

fn concurrent_run_leaves_nothing_behind<Q: TestQueue<Tracked>>() {
    let stats = DropStats::new();
    let q = Arc::new(Q::with_capacity(8));
    let per = scale(2000) as u64;
    thread::scope(|s| {
        for p in 0..3 {
            let (q, stats) = (&q, &stats);
            s.spawn(move || {
                for i in 0..per {
                    q.push(Tracked::new(p * per + i, stats)).unwrap();
                }
            });
        }
        for _ in 0..2 {
            let q = &q;
            s.spawn(move || {
                // Leave a few items in the queue for Drop to clean up.
                for _ in 0..(3 * per - 5) / 2 {
                    drop(q.pop().unwrap());
                }
            });
        }
    });
    assert_eq!(stats.created(), 3 * per as usize);
    assert_eq!(stats.live(), q.len());
    drop(q);
    assert_eq!(stats.live(), 0);
}

macro_rules! drop_tests {
    ($module:ident, $Q:ident) => {
        mod $module {
            use super::*;
            #[test]
            fn empty_queue_drops_nothing() {
                super::empty_queue_drops_nothing::<$Q<Tracked>>();
            }
            #[test]
            fn partially_full_queue_drops_its_items() {
                super::partially_full_queue_drops_its_items::<$Q<Tracked>>();
            }
            #[test]
            fn wrapped_queue_drops_exactly_live_items() {
                super::wrapped_queue_drops_exactly_live_items::<$Q<Tracked>>();
            }
            #[test]
            fn rejected_items_are_returned_not_dropped() {
                super::rejected_items_are_returned_not_dropped::<$Q<Tracked>>();
            }
            #[test]
            fn closed_queue_with_items_drops_them() {
                super::closed_queue_with_items_drops_them::<$Q<Tracked>>();
            }
            #[test]
            fn concurrent_run_leaves_nothing_behind() {
                super::concurrent_run_leaves_nothing_behind::<$Q<Tracked>>();
            }
        }
    };
}

drop_tests!(lockfree, LockFreeQueue);
drop_tests!(blocking, BlockingQueue);
#[cfg(target_pointer_width = "64")]
drop_tests!(scq, ScqQueue);
