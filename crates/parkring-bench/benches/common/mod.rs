//! Adapters giving four bounded queues one blocking push/pop interface.
#![allow(dead_code)]

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use parkring::{BlockingQueue, LockFreeQueue, ScqQueue};

/// The spin-then-yield wait parkring's queues use before parking (2^0..2^6 spin
/// hints, then `yield_now`), so the never-parking baselines wait the same way.
struct Backoff {
    step: u32,
}

impl Backoff {
    fn new() -> Self {
        Self { step: 0 }
    }

    fn snooze(&mut self) {
        if self.step <= 6 {
            for _ in 0..1u32 << self.step {
                std::hint::spin_loop();
            }
            self.step += 1;
        } else {
            std::thread::yield_now();
        }
    }
}

pub trait BenchQueue: Send + Sync + 'static {
    const NAME: &'static str;
    fn with_capacity(capacity: usize) -> Self;
    /// Whether this queue can serve `producers` x `consumers` honestly.
    fn supports(_producers: usize, _consumers: usize) -> bool {
        true
    }
    fn push(&self, value: u64);
    fn pop(&self) -> u64;
}

impl BenchQueue for LockFreeQueue<u64> {
    const NAME: &'static str = "lockfree";
    fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }
    fn push(&self, value: u64) {
        LockFreeQueue::push(self, value).unwrap();
    }
    fn pop(&self) -> u64 {
        LockFreeQueue::pop(self).unwrap()
    }
}

impl BenchQueue for ScqQueue<u64> {
    const NAME: &'static str = "scq";
    fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }
    fn push(&self, value: u64) {
        ScqQueue::push(self, value).unwrap();
    }
    fn pop(&self) -> u64 {
        ScqQueue::pop(self).unwrap()
    }
}

impl BenchQueue for BlockingQueue<u64> {
    const NAME: &'static str = "blocking";
    fn with_capacity(capacity: usize) -> Self {
        Self::new(capacity)
    }
    fn push(&self, value: u64) {
        BlockingQueue::push(self, value).unwrap();
    }
    fn pop(&self) -> u64 {
        BlockingQueue::pop(self).unwrap()
    }
}

/// crossbeam's `ArrayQueue` is non-blocking only; wait with the same
/// spin-then-yield backoff our queue uses before it parks. It never parks.
pub struct Crossbeam(crossbeam_queue::ArrayQueue<u64>);

impl BenchQueue for Crossbeam {
    const NAME: &'static str = "crossbeam";
    fn with_capacity(capacity: usize) -> Self {
        Self(crossbeam_queue::ArrayQueue::new(capacity))
    }
    fn push(&self, mut value: u64) {
        let mut backoff = Backoff::new();
        while let Err(v) = self.0.push(value) {
            value = v;
            backoff.snooze();
        }
    }
    fn pop(&self) -> u64 {
        let mut backoff = Backoff::new();
        loop {
            if let Some(v) = self.0.pop() {
                return v;
            }
            backoff.snooze();
        }
    }
}

/// `std::sync::mpsc::sync_channel`. The `Receiver` is `!Sync`, so it sits
/// behind a mutex and only single-consumer shapes are benchmarked, where that
/// mutex is uncontended.
pub struct StdChannel {
    tx: SyncSender<u64>,
    rx: Mutex<Receiver<u64>>,
}

impl BenchQueue for StdChannel {
    const NAME: &'static str = "std_sync_channel";
    fn with_capacity(capacity: usize) -> Self {
        let (tx, rx) = sync_channel(capacity);
        Self {
            tx,
            rx: Mutex::new(rx),
        }
    }
    fn supports(_producers: usize, consumers: usize) -> bool {
        consumers == 1
    }
    fn push(&self, value: u64) {
        self.tx.send(value).unwrap();
    }
    fn pop(&self) -> u64 {
        self.rx.lock().unwrap().recv().unwrap()
    }
}
