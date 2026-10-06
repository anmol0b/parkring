//! Parallel quicksort with `join` on a work-stealing pool.
//!
//! `join(a, b)` runs `a` on the current worker and offers `b` to idle workers;
//! if nobody steals it, the current worker runs it too. Recursing with `join`
//! therefore spreads the work across the pool without any explicit queues.
//!
//! Run with `cargo run --release --example quicksort`.

use std::time::Instant;

use parkring::{ThreadPool, join};

/// Below this length, sorting in parallel costs more than it saves.
const SEQUENTIAL_CUTOFF: usize = 4096;

fn quicksort<T: Ord + Send>(v: &mut [T]) {
    if v.len() <= SEQUENTIAL_CUTOFF {
        v.sort_unstable();
        return;
    }
    let mid = partition(v);
    let (left, right) = v.split_at_mut(mid);
    join(|| quicksort(left), || quicksort(&mut right[1..]));
}

/// Lomuto partition around the middle element. Returns the pivot's final index.
fn partition<T: Ord>(v: &mut [T]) -> usize {
    let last = v.len() - 1;
    v.swap(v.len() / 2, last);
    let mut store = 0;
    for i in 0..last {
        if v[i] <= v[last] {
            v.swap(i, store);
            store += 1;
        }
    }
    v.swap(store, last);
    store
}

/// A small xorshift generator, so the example needs no dependencies.
fn random_values(n: usize) -> Vec<u64> {
    let mut x = 0x9E37_79B9_7F4A_7C15_u64;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        })
        .collect()
}

fn main() {
    let threads = std::thread::available_parallelism().map_or(4, usize::from);
    let pool = ThreadPool::new(threads);
    let original = random_values(4_000_000);

    let mut sequential = original.clone();
    let start = Instant::now();
    sequential.sort_unstable();
    let sequential_time = start.elapsed();

    let mut parallel = original;
    let start = Instant::now();
    pool.install(|| quicksort(&mut parallel));
    let parallel_time = start.elapsed();

    assert_eq!(parallel, sequential);
    println!("sorted {} values", parallel.len());
    println!("  slice::sort_unstable:           {sequential_time:?}");
    println!("  quicksort with join, {threads:>2} threads: {parallel_time:?}");
}
