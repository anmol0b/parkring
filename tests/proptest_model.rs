//! Model-based test: random operation sequences must behave exactly like a
//! `VecDeque` bounded at the queue's capacity. Single-threaded, so every
//! `Full`/`Empty` answer must be exact.

mod common;

use std::collections::VecDeque;

use common::TestQueue;
#[cfg(target_pointer_width = "64")]
use parkring::ScqQueue;
use parkring::{BlockingQueue, LockFreeQueue, TryPopError, TryPushError};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Op {
    TryPush(u32),
    TryPop,
    Len,
    Close,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        10 => any::<u32>().prop_map(Op::TryPush),
        10 => Just(Op::TryPop),
        3 => Just(Op::Len),
        1 => Just(Op::Close),
    ]
}

fn run<Q: TestQueue<u32>>(capacity: usize, ops: &[Op]) -> Result<(), TestCaseError> {
    let q = Q::with_capacity(capacity);
    let cap = q.capacity();
    let mut model = VecDeque::with_capacity(cap);
    let mut closed = false;
    for op in ops {
        match *op {
            Op::TryPush(v) => {
                let expected = if closed {
                    Err(TryPushError::Closed(v))
                } else if model.len() == cap {
                    Err(TryPushError::Full(v))
                } else {
                    model.push_back(v);
                    Ok(())
                };
                prop_assert_eq!(q.try_push(v), expected);
            }
            Op::TryPop => {
                let expected = match model.pop_front() {
                    Some(v) => Ok(v),
                    None if closed => Err(TryPopError::Closed),
                    None => Err(TryPopError::Empty),
                };
                prop_assert_eq!(q.try_pop(), expected);
            }
            Op::Len => {
                prop_assert_eq!(q.len(), model.len());
                prop_assert_eq!(q.is_empty(), model.is_empty());
                prop_assert_eq!(q.is_full(), model.len() == cap);
            }
            Op::Close => {
                prop_assert_eq!(q.close(), !closed);
                closed = true;
            }
        }
        prop_assert_eq!(q.is_closed(), closed);
    }
    while let Some(v) = model.pop_front() {
        prop_assert_eq!(q.try_pop(), Ok(v));
    }
    prop_assert!(q.try_pop().is_err());
    Ok(())
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: if cfg!(miri) { 8 } else { 512 },
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// Capacities 1..=17 include every non-power-of-two the original code
    /// mishandled.
    #[test]
    fn lockfree_matches_vecdeque(cap in 1usize..=17, ops in prop::collection::vec(op(), 0..300)) {
        run::<LockFreeQueue<u32>>(cap, &ops)?;
    }

    /// SCQ's single-threaded Full/Empty answers are exact, so the same model
    /// applies. Long sequences exercise the threshold and catchup paths.
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn scq_matches_vecdeque(cap in 1usize..=17, ops in prop::collection::vec(op(), 0..300)) {
        run::<ScqQueue<u32>>(cap, &ops)?;
    }

    #[test]
    fn blocking_matches_vecdeque(cap in 1usize..=17, ops in prop::collection::vec(op(), 0..300)) {
        run::<BlockingQueue<u32>>(cap, &ops)?;
    }

    #[test]
    fn lockfree_capacity_is_next_power_of_two(cap in 1usize..=1 << 16) {
        prop_assert_eq!(LockFreeQueue::<u8>::new(cap).capacity(), cap.next_power_of_two().max(2));
    }
}
