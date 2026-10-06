//! Work-stealing deque against crossbeam-deque.
//!
//! crossbeam stores values inline and copies them out with a non-atomic read
//! it documents as technically UB; ours stores one atomic pointer per slot and
//! boxes values. `crossbeam<Box<u64>>` pays the same allocation as ours, so it
//! isolates the algorithm; `crossbeam<u64>` shows the cost of boxing.
#![allow(missing_docs)] // criterion_group! generates undocumented items

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

const ITEMS: usize = 1 << 16;

fn owner_push_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("deque_owner");
    group.throughput(Throughput::Elements(ITEMS as u64));
    group.bench_function(BenchmarkId::new("parkring", "u64"), |b| {
        let w = parkring::Worker::<u64>::new();
        b.iter(|| {
            for i in 0..ITEMS as u64 {
                w.push(i);
            }
            while let Some(v) = w.pop() {
                black_box(v);
            }
        });
    });
    group.bench_function(BenchmarkId::new("crossbeam", "Box<u64>"), |b| {
        let w = crossbeam_deque::Worker::<Box<u64>>::new_lifo();
        b.iter(|| {
            for i in 0..ITEMS as u64 {
                w.push(Box::new(i));
            }
            while let Some(v) = w.pop() {
                black_box(v);
            }
        });
    });
    group.bench_function(BenchmarkId::new("crossbeam", "u64"), |b| {
        let w = crossbeam_deque::Worker::<u64>::new_lifo();
        b.iter(|| {
            for i in 0..ITEMS as u64 {
                w.push(i);
            }
            while let Some(v) = w.pop() {
                black_box(v);
            }
        });
    });
    group.finish();
}

/// The owner fills the deque, then `thieves` pre-spawned threads drain it.
/// Only the draining is timed.
fn steal_drain<W, S>(
    c: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    thieves: usize,
    new: impl Fn() -> (W, S),
    push: impl Fn(&W, u64),
    steal: impl Fn(&S) -> Option<Option<u64>> + Send + Sync + 'static,
) where
    S: Clone + Send + 'static,
{
    let (worker, stealer) = new();
    let steal = Arc::new(steal);
    let start = Arc::new(Barrier::new(thieves + 1));
    let done = Arc::new(Barrier::new(thieves + 1));
    let stop = Arc::new(AtomicBool::new(false));
    let taken = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..thieves)
        .map(|_| {
            let (s, steal, start, done, stop, taken) = (
                stealer.clone(),
                Arc::clone(&steal),
                Arc::clone(&start),
                Arc::clone(&done),
                Arc::clone(&stop),
                Arc::clone(&taken),
            );
            thread::spawn(move || {
                loop {
                    start.wait();
                    if stop.load(SeqCst) {
                        return;
                    }
                    let mut n = 0;
                    // None = empty, Some(None) = retry, Some(Some(v)) = stolen.
                    while let Some(r) = steal(&s) {
                        if let Some(v) = r {
                            black_box(v);
                            n += 1;
                        }
                    }
                    taken.fetch_add(n, SeqCst);
                    done.wait();
                }
            })
        })
        .collect();
    c.bench_function(BenchmarkId::new(name, format!("{thieves}_thieves")), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                for i in 0..ITEMS as u64 {
                    push(&worker, i);
                }
                taken.store(0, SeqCst);
                start.wait();
                let t = Instant::now();
                done.wait();
                total += t.elapsed();
                assert_eq!(taken.load(SeqCst), ITEMS);
            }
            total
        });
    });
    stop.store(true, SeqCst);
    start.wait();
    for h in handles {
        h.join().unwrap();
    }
}

fn steal_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("deque_steal");
    group.throughput(Throughput::Elements(ITEMS as u64));
    for thieves in [1, 2, 4] {
        steal_drain(
            &mut group,
            "parkring",
            thieves,
            || {
                let w = parkring::Worker::<u64>::new();
                let s = w.stealer();
                (w, s)
            },
            parkring::Worker::push,
            |s| match s.steal() {
                parkring::Steal::Success(v) => Some(Some(v)),
                parkring::Steal::Retry => Some(None),
                parkring::Steal::Empty => None,
            },
        );
        steal_drain(
            &mut group,
            "crossbeam_box",
            thieves,
            || {
                let w = crossbeam_deque::Worker::<Box<u64>>::new_lifo();
                let s = w.stealer();
                (w, s)
            },
            |w, v| w.push(Box::new(v)),
            |s| match s.steal() {
                crossbeam_deque::Steal::Success(v) => Some(Some(*v)),
                crossbeam_deque::Steal::Retry => Some(None),
                crossbeam_deque::Steal::Empty => None,
            },
        );
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(20);
    targets = owner_push_pop, steal_throughput
}
criterion_main!(benches);
