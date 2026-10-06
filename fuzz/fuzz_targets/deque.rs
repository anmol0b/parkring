//! Drives a work-stealing deque from one thread: the owner pushes and pops at
//! the bottom, the stealer takes from the top. Checked against a `VecDeque`,
//! including indices that start next to `usize::MAX` and wrap.
#![no_main]

use std::collections::VecDeque;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use parkring::{Steal, Worker};

#[derive(Arbitrary, Debug)]
enum Op {
    Push(u16),
    Pop,
    Steal,
    /// Steal through a fresh clone of the stealer.
    StealViaClone,
}

#[derive(Arbitrary, Debug)]
struct Input {
    capacity: u8,
    near_wrap: bool,
    ops: Vec<Op>,
}

fuzz_target!(|input: Input| {
    let capacity = usize::from(input.capacity % 16) + 1;
    let start = if input.near_wrap { usize::MAX - 8 } else { 0 };
    let worker = Worker::with_capacity_and_start(capacity, start);
    let stealer = worker.stealer();
    let mut model = VecDeque::new();
    for op in input.ops {
        match op {
            Op::Push(v) => {
                worker.push(v);
                model.push_back(v);
            }
            Op::Pop => assert_eq!(worker.pop(), model.pop_back()),
            Op::Steal | Op::StealViaClone => {
                let got = match op {
                    Op::Steal => stealer.steal(),
                    _ => stealer.clone().steal(),
                };
                // With no concurrent thread there is no race to lose.
                let expected = model.pop_front().map_or(Steal::Empty, Steal::Success);
                assert_eq!(got, expected);
            }
        }
        assert_eq!(worker.len(), model.len());
        assert_eq!(worker.is_empty(), model.is_empty());
        assert_eq!(stealer.is_empty(), model.is_empty());
    }
});
