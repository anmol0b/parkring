//! The work-stealing pool against Rayon.
//!
//! * `pool_fib_cutoff`: fib(32), sequential below 20. Compute-bound; this is
//!   the scaling measurement.
//! * `pool_fib`: fib(25) with a `join` at every level: pure `join` overhead.
//!   Sequential code wins; the question is by how much.
//! * `pool_sum`: a chunked sum of 32 MB, bound by memory bandwidth.
#![allow(missing_docs)] // criterion_group! generates undocumented items

use std::time::Duration;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const FIB_N: u64 = 25;
const FIB_CUTOFF_N: u64 = 32;
const FIB_CUTOFF: u64 = 20;
const SUM_LEN: usize = 1 << 22;
const SUM_CUTOFF: usize = 1 << 12;

fn fib_parkring(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let (a, b) = parkring::join(|| fib_parkring(n - 1), || fib_parkring(n - 2));
    a + b
}

fn fib_rayon(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let (a, b) = rayon::join(|| fib_rayon(n - 1), || fib_rayon(n - 2));
    a + b
}

fn fib_cutoff_parkring(n: u64) -> u64 {
    if n < FIB_CUTOFF {
        return fib_sequential(n);
    }
    let (a, b) = parkring::join(|| fib_cutoff_parkring(n - 1), || fib_cutoff_parkring(n - 2));
    a + b
}

fn fib_cutoff_rayon(n: u64) -> u64 {
    if n < FIB_CUTOFF {
        return fib_sequential(n);
    }
    let (a, b) = rayon::join(|| fib_cutoff_rayon(n - 1), || fib_cutoff_rayon(n - 2));
    a + b
}

fn fib_sequential(n: u64) -> u64 {
    if n < 2 {
        n
    } else {
        fib_sequential(n - 1) + fib_sequential(n - 2)
    }
}

fn sum_parkring(values: &[u64]) -> u64 {
    if values.len() <= SUM_CUTOFF {
        return values.iter().sum();
    }
    let (left, right) = values.split_at(values.len() / 2);
    let (sum_left, sum_right) = parkring::join(|| sum_parkring(left), || sum_parkring(right));
    sum_left + sum_right
}

fn sum_rayon(values: &[u64]) -> u64 {
    if values.len() <= SUM_CUTOFF {
        return values.iter().sum();
    }
    let (left, right) = values.split_at(values.len() / 2);
    let (sum_left, sum_right) = rayon::join(|| sum_rayon(left), || sum_rayon(right));
    sum_left + sum_right
}

fn fib(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool_fib");
    group.bench_function(BenchmarkId::new("sequential", 1), |b| {
        b.iter(|| fib_sequential(black_box(FIB_N)));
    });
    for threads in [1, 4, 8] {
        let ours = parkring::ThreadPool::new(threads);
        group.bench_function(BenchmarkId::new("parkring", threads), |b| {
            b.iter(|| ours.install(|| fib_parkring(black_box(FIB_N))));
        });
        let theirs = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        group.bench_function(BenchmarkId::new("rayon", threads), |b| {
            b.iter(|| theirs.install(|| fib_rayon(black_box(FIB_N))));
        });
    }
    group.finish();
}

fn fib_cutoff(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool_fib_cutoff");
    group.bench_function(BenchmarkId::new("sequential", 1), |b| {
        b.iter(|| fib_sequential(black_box(FIB_CUTOFF_N)));
    });
    for threads in [1, 2, 4, 8] {
        let ours = parkring::ThreadPool::new(threads);
        group.bench_function(BenchmarkId::new("parkring", threads), |b| {
            b.iter(|| ours.install(|| fib_cutoff_parkring(black_box(FIB_CUTOFF_N))));
        });
        let theirs = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        group.bench_function(BenchmarkId::new("rayon", threads), |b| {
            b.iter(|| theirs.install(|| fib_cutoff_rayon(black_box(FIB_CUTOFF_N))));
        });
    }
    group.finish();
}

fn sum(c: &mut Criterion) {
    let data: Vec<u64> = (0..SUM_LEN as u64).collect();
    let mut group = c.benchmark_group("pool_sum");
    group.bench_function(BenchmarkId::new("sequential", 1), |b| {
        b.iter(|| black_box(&data).iter().sum::<u64>());
    });
    for threads in [1, 4, 8] {
        let ours = parkring::ThreadPool::new(threads);
        group.bench_function(BenchmarkId::new("parkring", threads), |b| {
            b.iter(|| ours.install(|| sum_parkring(black_box(&data))));
        });
        let theirs = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        group.bench_function(BenchmarkId::new("rayon", threads), |b| {
            b.iter(|| theirs.install(|| sum_rayon(black_box(&data))));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(20);
    targets = fib_cutoff, fib, sum
}
criterion_main!(benches);
