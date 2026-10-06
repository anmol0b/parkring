//! A three-stage pipeline joined by bounded channels.
//!
//! A reader produces lines, a pool of parser threads turns them into numbers,
//! and one aggregator sums them. The channels are bounded, so a slow stage
//! pushes back on the stages before it instead of letting memory grow.
//!
//! Shutdown needs no bookkeeping: each stage owns the senders for its output,
//! and when a stage's threads finish they drop them. Once the last sender of
//! a channel is gone, the next stage drains what is left and its `for` loop
//! ends.
//!
//! Run with `cargo run --release --example pipeline`.

use std::thread;

use parkring::channel;

const LINES: u64 = 200_000;
const PARSERS: usize = 4;

fn main() {
    let (line_tx, line_rx) = channel::bounded::<String>(1024);
    let (number_tx, number_rx) = channel::bounded::<u64>(1024);

    // Stage 1: read. Here the "input" is generated text.
    let reader = thread::spawn(move || {
        for i in 0..LINES {
            line_tx
                .send(format!("value={i}"))
                .expect("parsers stopped early");
        }
        // `line_tx` drops here, which tells the parsers no more lines are coming.
    });

    // Stage 2: parse. Every parser holds a receiver clone and a sender clone.
    let parsers: Vec<_> = (0..PARSERS)
        .map(|_| {
            let (lines, numbers) = (line_rx.clone(), number_tx.clone());
            thread::spawn(move || {
                for line in lines {
                    let n = line["value=".len()..].parse().expect("malformed line");
                    numbers.send(n).expect("aggregator stopped early");
                }
            })
        })
        .collect();
    // Only the parsers' clones should keep these channels alive.
    drop((line_rx, number_tx));

    // Stage 3: aggregate. The loop ends when the last parser drops its sender.
    let total: u64 = number_rx.iter().sum();

    reader.join().unwrap();
    for p in parsers {
        p.join().unwrap();
    }
    assert_eq!(total, LINES * (LINES - 1) / 2);
    println!("parsed and summed {LINES} lines with {PARSERS} parsers: {total}");
}
