//! A three-stage pipeline joined by bounded queues.
//!
//! A reader produces lines, a pool of parser threads turns them into numbers,
//! and one aggregator sums them. The queues are bounded, so a slow stage pushes
//! back on the stages before it instead of letting memory grow. Shutdown flows
//! downstream: each stage closes its output queue once its input is closed and
//! drained.
//!
//! Run with `cargo run --release --example pipeline`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use parkring::LockFreeQueue;

const LINES: u64 = 200_000;
const PARSERS: usize = 4;

fn main() {
    let lines = LockFreeQueue::<String>::new(1024);
    let numbers = LockFreeQueue::<u64>::new(1024);
    let parsers_left = AtomicUsize::new(PARSERS);

    let total = thread::scope(|s| {
        // Stage 1: read. Here the "input" is generated text.
        s.spawn(|| {
            for i in 0..LINES {
                lines
                    .push(format!("value={i}"))
                    .expect("lines closed early");
            }
            lines.close();
        });

        // Stage 2: parse. Several consumers share one input queue.
        for _ in 0..PARSERS {
            s.spawn(|| {
                while let Ok(line) = lines.pop() {
                    let n = line["value=".len()..].parse().expect("malformed line");
                    numbers.push(n).expect("numbers closed early");
                }
                // The last parser to finish closes the next stage's input.
                if parsers_left.fetch_sub(1, Ordering::AcqRel) == 1 {
                    numbers.close();
                }
            });
        }

        // Stage 3: aggregate. `pop` returns `Err` once `numbers` is closed and empty.
        let aggregator = s.spawn(|| {
            let mut sum = 0u64;
            while let Ok(n) = numbers.pop() {
                sum += n;
            }
            sum
        });
        aggregator.join().unwrap()
    });

    assert_eq!(total, LINES * (LINES - 1) / 2);
    println!("parsed and summed {LINES} lines with {PARSERS} parsers: {total}");
}
