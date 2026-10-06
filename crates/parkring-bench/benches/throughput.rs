//! Throughput: items moved per second through a queue by P producers and C
//! consumers.
//!
//! Worker threads are spawned once per configuration and reused for every
//! iteration; each iteration releases them through a barrier and moves
//! `ITEMS` items. The original benchmark spawned threads inside the timed
//! loop and moved 100 items per thread, so thread creation dominated.
#![allow(missing_docs)] // criterion_group! generates undocumented items

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use common::{BenchQueue, Crossbeam, StdChannel};
use std::hint::black_box;

use criterion::measurement::WallTime;
use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
};
use parkring::{BlockingQueue, LockFreeQueue, ScqQueue};

/// Items per timed iteration. Divisible by every producer/consumer count used.
const ITEMS: u64 = 1 << 18;

struct Pool {
    start: Arc<Barrier>,
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

impl Pool {
    fn new<Q: BenchQueue>(queue: &Arc<Q>, producers: usize, consumers: usize) -> Self {
        let start = Arc::new(Barrier::new(producers + consumers + 1));
        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::new();
        let per_producer = ITEMS / producers as u64;
        let per_consumer = ITEMS / consumers as u64;
        for _ in 0..producers {
            let (q, start, stop) = (Arc::clone(queue), Arc::clone(&start), Arc::clone(&stop));
            workers.push(thread::spawn(move || {
                loop {
                    start.wait();
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    for i in 0..per_producer {
                        q.push(i);
                    }
                    start.wait();
                }
            }));
        }
        for _ in 0..consumers {
            let (q, start, stop) = (Arc::clone(queue), Arc::clone(&start), Arc::clone(&stop));
            workers.push(thread::spawn(move || {
                loop {
                    start.wait();
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    for _ in 0..per_consumer {
                        black_box(q.pop());
                    }
                    start.wait();
                }
            }));
        }
        Self {
            start,
            stop,
            workers,
        }
    }

    /// Releases the workers and times until all of them finish.
    fn run_once(&self) -> Duration {
        self.start.wait();
        let t = Instant::now();
        self.start.wait();
        t.elapsed()
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.start.wait();
        for w in self.workers.drain(..) {
            w.join().unwrap();
        }
    }
}

fn bench<Q: BenchQueue>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    producers: usize,
    consumers: usize,
    capacity: usize,
) {
    if !Q::supports(producers, consumers) {
        return;
    }
    let queue = Arc::new(Q::with_capacity(capacity));
    let pool = Pool::new(&queue, producers, consumers);
    group.throughput(Throughput::Elements(ITEMS));
    group.bench_function(
        BenchmarkId::new(Q::NAME, format!("{producers}p{consumers}c_cap{capacity}")),
        |b| b.iter_custom(|iters| (0..iters).map(|_| pool.run_once()).sum()),
    );
}

fn all_queues(
    group: &mut BenchmarkGroup<'_, WallTime>,
    producers: usize,
    consumers: usize,
    capacity: usize,
) {
    bench::<LockFreeQueue<u64>>(group, producers, consumers, capacity);
    bench::<ScqQueue<u64>>(group, producers, consumers, capacity);
    bench::<Crossbeam>(group, producers, consumers, capacity);
    bench::<BlockingQueue<u64>>(group, producers, consumers, capacity);
    bench::<StdChannel>(group, producers, consumers, capacity);
}

fn spsc(c: &mut Criterion) {
    let mut group = c.benchmark_group("spsc");
    for capacity in [16, 256, 4096] {
        all_queues(&mut group, 1, 1, capacity);
    }
    group.finish();
}

/// N producers + N consumers. 8+8 and 16+16 oversubscribe a 10-core machine,
/// which is where a CAS retry loop and a preempted producer hurt most.
fn mpmc(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpmc");
    for n in [1, 2, 4, 8, 16] {
        all_queues(&mut group, n, n, 256);
    }
    group.finish();
}

fn asymmetric(c: &mut Criterion) {
    let mut group = c.benchmark_group("asymmetric");
    all_queues(&mut group, 8, 1, 256);
    all_queues(&mut group, 8, 2, 256);
    all_queues(&mut group, 2, 8, 256);
    group.finish();
}

fn capacity(c: &mut Criterion) {
    let mut group = c.benchmark_group("capacity");
    for capacity in [16, 256, 4096] {
        all_queues(&mut group, 4, 4, capacity);
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(4))
        .sample_size(20);
    targets = spsc, mpmc, asymmetric, capacity
}
criterion_main!(benches);
