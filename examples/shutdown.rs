//! Backpressure, timeouts and graceful shutdown.
//!
//! A fast producer feeds a slow consumer through a small queue. The producer
//! uses `push_timeout`, so it notices when the consumer has fallen behind
//! instead of blocking forever. The consumer uses `pop_timeout`, so it can do
//! housekeeping while the queue is idle. `close` stops new work, and the
//! consumer still drains everything that was accepted before it exits.
//!
//! Run with `cargo run --release --example shutdown`.

use std::thread;
use std::time::Duration;

use parkring::{LockFreeQueue, PopTimeoutError, PushTimeoutError};

fn main() {
    let jobs = LockFreeQueue::<u32>::new(8);

    let (processed, idle_ticks) = thread::scope(|s| {
        let consumer = s.spawn(|| {
            let (mut processed, mut idle_ticks) = (0u32, 0u32);
            loop {
                match jobs.pop_timeout(Duration::from_millis(20)) {
                    Ok(_job) => {
                        thread::sleep(Duration::from_millis(5)); // slow work
                        processed += 1;
                    }
                    Err(PopTimeoutError::Timeout) => idle_ticks += 1, // housekeeping goes here
                    Err(PopTimeoutError::Closed) => return (processed, idle_ticks),
                }
            }
        });

        let mut accepted = 0u32;
        let mut shed = 0u32;
        for job in 0..100 {
            match jobs.push_timeout(job, Duration::from_millis(1)) {
                Ok(()) => accepted += 1,
                // The consumer is behind: drop the job (or retry, or log).
                Err(PushTimeoutError::Timeout(_job)) => shed += 1,
                Err(PushTimeoutError::Closed(_)) => unreachable!("only closed below"),
            }
        }
        println!("producer: {accepted} accepted, {shed} shed under backpressure");

        // Let the consumer go idle for a moment, then shut down.
        thread::sleep(Duration::from_millis(100));
        jobs.close();
        assert!(jobs.push(999).is_err(), "pushes fail once closed");

        let (processed, idle_ticks) = consumer.join().unwrap();
        assert_eq!(processed, accepted, "every accepted job is processed");
        (processed, idle_ticks)
    });

    println!("consumer: processed {processed}, idle {idle_ticks} times, exited after close");
}
