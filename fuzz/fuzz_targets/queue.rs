//! Drives each queue with an arbitrary single-threaded operation sequence and
//! checks every result against a `VecDeque` model. With one thread the queues'
//! `Full` and `Empty` answers are exact, so the model must agree everywhere.
#![no_main]

use std::collections::VecDeque;
use std::time::Duration;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use parkring::{
    BlockingQueue, BoundedQueue, LockFreeQueue, PopError, PopTimeoutError, PushError,
    PushTimeoutError, TryPopError, TryPushError,
};

#[derive(Arbitrary, Debug)]
enum Op {
    TryPush(u16),
    TryPop,
    /// Blocking push, issued only when it cannot block.
    Push(u16),
    /// Blocking pop, issued only when it cannot block.
    Pop,
    PushTimeout(u16),
    PopTimeout,
    Close,
}

#[derive(Arbitrary, Debug)]
struct Input {
    kind: u8,
    capacity: u8,
    ops: Vec<Op>,
}

fuzz_target!(|input: Input| {
    let capacity = usize::from(input.capacity % 64) + 1;
    match input.kind % 3 {
        0 => run(&LockFreeQueue::new(capacity), capacity, &input.ops),
        1 => run(&BlockingQueue::new(capacity), capacity, &input.ops),
        #[cfg(target_pointer_width = "64")]
        _ => run(&parkring::ScqQueue::new(capacity), capacity, &input.ops),
        #[cfg(not(target_pointer_width = "64"))]
        _ => {}
    }
});

fn run<Q: BoundedQueue<u16>>(q: &Q, requested: usize, ops: &[Op]) {
    let cap = q.capacity();
    assert!(cap >= requested, "capacity {cap} below the requested {requested}");
    let mut model = VecDeque::new();
    let mut closed = false;
    let zero = Duration::ZERO;
    for op in ops {
        let full = model.len() == cap;
        match *op {
            Op::TryPush(v) => {
                let expected = if closed {
                    Err(TryPushError::Closed(v))
                } else if full {
                    Err(TryPushError::Full(v))
                } else {
                    Ok(())
                };
                assert_eq!(q.try_push(v), expected);
                if expected.is_ok() {
                    model.push_back(v);
                }
            }
            Op::Push(v) if closed || !full => {
                let expected = if closed { Err(PushError(v)) } else { Ok(()) };
                assert_eq!(q.push(v), expected);
                if expected.is_ok() {
                    model.push_back(v);
                }
            }
            Op::PushTimeout(v) => {
                let expected = if closed {
                    Err(PushTimeoutError::Closed(v))
                } else if full {
                    Err(PushTimeoutError::Timeout(v))
                } else {
                    Ok(())
                };
                assert_eq!(q.push_timeout(v, zero), expected);
                if expected.is_ok() {
                    model.push_back(v);
                }
            }
            Op::TryPop => {
                let expected = match model.pop_front() {
                    Some(v) => Ok(v),
                    None if closed => Err(TryPopError::Closed),
                    None => Err(TryPopError::Empty),
                };
                assert_eq!(q.try_pop(), expected);
            }
            Op::Pop if closed || !model.is_empty() => {
                let expected = model.pop_front().ok_or(PopError);
                assert_eq!(q.pop(), expected);
            }
            Op::PopTimeout => {
                let expected = match model.pop_front() {
                    Some(v) => Ok(v),
                    None if closed => Err(PopTimeoutError::Closed),
                    None => Err(PopTimeoutError::Timeout),
                };
                assert_eq!(q.pop_timeout(zero), expected);
            }
            Op::Close => {
                assert_eq!(q.close(), !closed, "close reports whether it closed");
                closed = true;
            }
            // A blocking call that would block forever with one thread.
            Op::Push(_) | Op::Pop => {}
        }
        assert_eq!(q.len(), model.len());
        assert_eq!(q.is_empty(), model.is_empty());
        assert_eq!(q.is_full(), model.len() == cap);
        assert_eq!(q.is_closed(), closed);
    }
}
