//! Shared test harness: every behaviour is written once, generic over
//! `BoundedQueue`, and instantiated for each implementation by `queue_tests!`.
#![allow(
    dead_code,
    unused_macros,
    unused_imports,
    clippy::cast_possible_truncation
)]

pub mod drop_counter;

use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use parkring::{
    BlockingQueue, BoundedQueue, LockFreeQueue, PopError, PopTimeoutError, PushError,
    PushTimeoutError, TryPopError, TryPushError,
};

/// Constructor hook the trait deliberately leaves out.
pub trait TestQueue<T: Send>: BoundedQueue<T> + Sized + 'static {
    fn with_capacity(capacity: usize) -> Self;
}

impl<T: Send + 'static> TestQueue<T> for LockFreeQueue<T> {
    fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }
}

#[cfg(target_pointer_width = "64")]
impl<T: Send + 'static> TestQueue<T> for parkring::ScqQueue<T> {
    fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }
}

impl<T: Send + 'static> TestQueue<T> for BlockingQueue<T> {
    fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }
}

/// Scales iteration counts down under Miri, which is ~1000x slower.
pub const fn scale(n: usize) -> usize {
    if cfg!(miri) { n / 100 + 1 } else { n }
}

/// Long enough for a blocked thread to exhaust its spin budget and park.
pub const PARK_DELAY: Duration = Duration::from_millis(if cfg!(miri) { 2 } else { 30 });

pub fn single_item_roundtrip<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    q.push(7).unwrap();
    assert_eq!(q.pop(), Ok(7));
}

pub fn fifo_up_to_capacity<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    let cap = q.capacity() as u32;
    for i in 0..cap {
        q.try_push(i).unwrap();
    }
    assert!(q.is_full());
    for i in 0..cap {
        assert_eq!(q.try_pop(), Ok(i));
    }
    assert!(q.is_empty());
}

pub fn fifo_interleaved<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    q.push(1).unwrap();
    q.push(2).unwrap();
    assert_eq!(q.pop(), Ok(1));
    q.push(3).unwrap();
    q.push(4).unwrap();
    assert_eq!(q.pop(), Ok(2));
    assert_eq!(q.pop(), Ok(3));
    assert_eq!(q.pop(), Ok(4));
}

/// Many laps with the ring partially full, so head and tail land on every slot.
pub fn fifo_many_laps<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    let mut next_in = 0;
    let mut next_out = 0;
    for round in 0..scale(1000) {
        for _ in 0..=(round % 4) {
            if q.try_push(next_in).is_ok() {
                next_in += 1;
            }
        }
        for _ in 0..=(round % 3) {
            if let Ok(v) = q.try_pop() {
                assert_eq!(v, next_out);
                next_out += 1;
            }
        }
        assert_eq!(q.len(), (next_in - next_out) as usize);
    }
}

pub fn try_push_full_returns_item<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    for i in 0..q.capacity() as u32 {
        q.try_push(i).unwrap();
    }
    assert_eq!(q.try_push(99), Err(TryPushError::Full(99)));
    assert_eq!(q.len(), q.capacity());
}

pub fn try_pop_empty<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    assert_eq!(q.try_pop(), Err(TryPopError::Empty));
    q.push(1).unwrap();
    q.pop().unwrap();
    assert_eq!(q.try_pop(), Err(TryPopError::Empty));
}

pub fn close_rejects_pushes_and_drains<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    q.push(1).unwrap();
    q.push(2).unwrap();
    assert!(!q.is_closed());
    assert!(q.close());
    assert!(!q.close(), "second close must report already closed");
    assert!(q.is_closed());
    assert_eq!(q.try_push(3), Err(TryPushError::Closed(3)));
    assert_eq!(q.push(4), Err(PushError(4)));
    assert_eq!(
        q.push_timeout(5, Duration::from_millis(1)),
        Err(PushTimeoutError::Closed(5))
    );
    assert_eq!(q.pop(), Ok(1));
    assert_eq!(q.try_pop(), Ok(2));
    assert_eq!(q.pop(), Err(PopError));
    assert_eq!(q.try_pop(), Err(TryPopError::Closed));
    assert_eq!(
        q.pop_timeout(Duration::from_millis(1)),
        Err(PopTimeoutError::Closed)
    );
}

pub fn blocked_consumer_wakes_on_push<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    thread::scope(|s| {
        let consumer = s.spawn(|| q.pop());
        thread::sleep(PARK_DELAY);
        q.push(7).unwrap();
        assert_eq!(consumer.join().unwrap(), Ok(7));
    });
}

pub fn blocked_producer_wakes_on_pop<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(1);
    let cap = q.capacity() as u32;
    for i in 0..cap {
        q.push(i).unwrap();
    }
    thread::scope(|s| {
        let producer = s.spawn(|| q.push(100));
        thread::sleep(PARK_DELAY);
        assert_eq!(q.pop(), Ok(0));
        assert_eq!(producer.join().unwrap(), Ok(()));
    });
    for i in 1..cap {
        assert_eq!(q.pop(), Ok(i));
    }
    assert_eq!(q.pop(), Ok(100));
}

pub fn close_wakes_blocked_consumers<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    thread::scope(|s| {
        let consumers: Vec<_> = (0..3).map(|_| s.spawn(|| q.pop())).collect();
        thread::sleep(PARK_DELAY);
        q.close();
        for c in consumers {
            assert_eq!(c.join().unwrap(), Err(PopError));
        }
    });
}

pub fn close_wakes_blocked_producers<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(1);
    for i in 0..q.capacity() as u32 {
        q.push(i).unwrap();
    }
    thread::scope(|s| {
        let producers: Vec<_> = (0..3u32)
            .map(|i| {
                let q = &q;
                s.spawn(move || q.push(100 + i))
            })
            .collect();
        thread::sleep(PARK_DELAY);
        q.close();
        let mut rejected: Vec<_> = producers
            .into_iter()
            .map(|p| p.join().unwrap().unwrap_err().into_inner())
            .collect();
        rejected.sort_unstable();
        assert_eq!(rejected, vec![100, 101, 102]);
    });
}

pub fn pop_timeout_elapses<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    let timeout = Duration::from_millis(20);
    let start = Instant::now();
    assert_eq!(q.pop_timeout(timeout), Err(PopTimeoutError::Timeout));
    assert!(start.elapsed() >= timeout);
}

pub fn push_timeout_elapses<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(1);
    for i in 0..q.capacity() as u32 {
        q.push(i).unwrap();
    }
    let timeout = Duration::from_millis(20);
    let start = Instant::now();
    assert_eq!(
        q.push_timeout(9, timeout),
        Err(PushTimeoutError::Timeout(9))
    );
    assert!(start.elapsed() >= timeout);
}

pub fn pop_timeout_returns_item_that_arrives<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(4);
    thread::scope(|s| {
        let consumer = s.spawn(|| q.pop_timeout(Duration::from_secs(10)));
        thread::sleep(PARK_DELAY);
        q.push(5).unwrap();
        assert_eq!(consumer.join().unwrap(), Ok(5));
    });
}

pub fn zero_timeout_does_not_block<Q: TestQueue<u32>>() {
    let q = Q::with_capacity(1);
    assert_eq!(q.pop_timeout(Duration::ZERO), Err(PopTimeoutError::Timeout));
    q.push(1).unwrap();
    assert_eq!(q.pop_timeout(Duration::ZERO), Ok(1));
}

/// Encodes producer id and per-producer sequence number in one item.
pub const fn tag(producer: usize, seq: usize) -> u64 {
    ((producer as u64) << 32) | seq as u64
}

/// P producers push `per_producer` tagged items each, C consumers pop until
/// the queue is closed and drained. Checks two properties that hold for any
/// correct FIFO queue under any interleaving:
///
/// 1. every item is received exactly once;
/// 2. within one consumer's log, each producer's items appear in the order
///    that producer pushed them.
pub fn mpmc_exactly_once_and_ordered<Q: TestQueue<u64>>(
    producers: usize,
    consumers: usize,
    capacity: usize,
    per_producer: usize,
    nonblocking: bool,
) {
    let q = Q::with_capacity(capacity);
    let start = Barrier::new(producers + consumers);
    let logs: Vec<Vec<u64>> = thread::scope(|s| {
        let consumer_handles: Vec<_> = (0..consumers)
            .map(|_| {
                s.spawn(|| {
                    start.wait();
                    let mut log = Vec::new();
                    if nonblocking {
                        loop {
                            match q.try_pop() {
                                Ok(v) => log.push(v),
                                Err(TryPopError::Empty) => thread::yield_now(),
                                Err(TryPopError::Closed) => break,
                            }
                        }
                    } else {
                        while let Ok(v) = q.pop() {
                            log.push(v);
                        }
                    }
                    log
                })
            })
            .collect();
        thread::scope(|ps| {
            for p in 0..producers {
                let (q, start) = (&q, &start);
                ps.spawn(move || {
                    start.wait();
                    for i in 0..per_producer {
                        let mut item = tag(p, i);
                        if nonblocking {
                            loop {
                                match q.try_push(item) {
                                    Ok(()) => break,
                                    Err(TryPushError::Full(v)) => {
                                        item = v;
                                        thread::yield_now();
                                    }
                                    Err(TryPushError::Closed(_)) => panic!("closed early"),
                                }
                            }
                        } else {
                            q.push(item).unwrap();
                        }
                    }
                });
            }
        });
        q.close();
        consumer_handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    });

    let mut seen = vec![false; producers * per_producer];
    for log in &logs {
        let mut last = vec![None::<usize>; producers];
        for &item in log {
            let (p, i) = ((item >> 32) as usize, (item & 0xFFFF_FFFF) as usize);
            let idx = p * per_producer + i;
            assert!(!seen[idx], "item {p}:{i} received twice");
            seen[idx] = true;
            if let Some(prev) = last[p] {
                assert!(i > prev, "producer {p} reordered: {i} after {prev}");
            }
            last[p] = Some(i);
        }
    }
    let missing = seen.iter().filter(|s| !**s).count();
    assert_eq!(missing, 0, "{missing} items lost");
    assert!(q.is_empty());
}

/// Instantiates every behaviour above as a named test for one queue type.
macro_rules! queue_tests {
    ($module:ident, $Q:ident) => {
        mod $module {
            use super::common::*;
            use parkring::$Q;

            #[test]
            fn single_item_roundtrip() {
                super::common::single_item_roundtrip::<$Q<u32>>();
            }
            #[test]
            fn fifo_up_to_capacity() {
                super::common::fifo_up_to_capacity::<$Q<u32>>();
            }
            #[test]
            fn fifo_interleaved() {
                super::common::fifo_interleaved::<$Q<u32>>();
            }
            #[test]
            fn fifo_many_laps() {
                super::common::fifo_many_laps::<$Q<u32>>();
            }
            #[test]
            fn try_push_full_returns_item() {
                super::common::try_push_full_returns_item::<$Q<u32>>();
            }
            #[test]
            fn try_pop_empty() {
                super::common::try_pop_empty::<$Q<u32>>();
            }
            #[test]
            fn close_rejects_pushes_and_drains() {
                super::common::close_rejects_pushes_and_drains::<$Q<u32>>();
            }
            #[test]
            fn blocked_consumer_wakes_on_push() {
                super::common::blocked_consumer_wakes_on_push::<$Q<u32>>();
            }
            #[test]
            fn blocked_producer_wakes_on_pop() {
                super::common::blocked_producer_wakes_on_pop::<$Q<u32>>();
            }
            #[test]
            fn close_wakes_blocked_consumers() {
                super::common::close_wakes_blocked_consumers::<$Q<u32>>();
            }
            #[test]
            fn close_wakes_blocked_producers() {
                super::common::close_wakes_blocked_producers::<$Q<u32>>();
            }
            #[test]
            fn pop_timeout_elapses() {
                super::common::pop_timeout_elapses::<$Q<u32>>();
            }
            #[test]
            fn push_timeout_elapses() {
                super::common::push_timeout_elapses::<$Q<u32>>();
            }
            #[test]
            fn pop_timeout_returns_item_that_arrives() {
                super::common::pop_timeout_returns_item_that_arrives::<$Q<u32>>();
            }
            #[test]
            fn zero_timeout_does_not_block() {
                super::common::zero_timeout_does_not_block::<$Q<u32>>();
            }
            #[test]
            #[should_panic(expected = "capacity must be non-zero")]
            fn zero_capacity_panics() {
                let _ = $Q::<u32>::new(0);
            }
            #[test]
            fn mpsc_4p_1c() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(4, 1, 64, scale(2000), false);
            }
            #[test]
            fn spmc_1p_4c() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(1, 4, 64, scale(8000), false);
            }
            #[test]
            fn mpmc_4p_4c() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(4, 4, 64, scale(2000), false);
            }
            #[test]
            fn asymmetric_8p_2c() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(8, 2, 1024, scale(1000), false);
            }
            #[test]
            fn asymmetric_2p_8c() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(2, 8, 1024, scale(4000), false);
            }
            #[test]
            fn capacity_one_handoff() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(3, 3, 1, scale(1000), false);
            }
            #[test]
            fn nonblocking_mpmc_4p_4c() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(4, 4, 16, scale(2000), true);
            }
            #[test]
            #[cfg_attr(miri, ignore = "too slow under Miri")]
            fn stress_16p_16c() {
                mpmc_exactly_once_and_ordered::<$Q<u64>>(16, 16, 1024, 5000, false);
            }
        }
    };
}
pub(crate) use queue_tests;
