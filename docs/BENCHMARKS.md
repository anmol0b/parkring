# Benchmarks

How parkring compares with the crates you would otherwise use, measured on an
Apple M4 (4 performance + 6 efficiency cores, 16 GB), macOS 27, Rust 1.92,
with the machine otherwise in normal desktop use. The losses are reported as
plainly as the wins.

```sh
cargo bench -p parkring-bench            # throughput, latency, deque, pool
cargo run -p parkring-bench --release --example plot   # rewrites assets/*.svg, prints the tables below
```

## Summary

Million items per second, higher is better, unless the row says otherwise.

| workload | parkring | best alternative | result |
|---|---|---|---|
| queue, 1 producer + 1 consumer, capacity 256 | 91 | crossbeam `ArrayQueue`: 80 to 85 | **parkring faster**, 7 to 14% |
| queue, 1 + 1, capacity 4096 | 127 | crossbeam: 121 | parkring faster, 5% |
| queue, 2 + 2 up to 16 + 16 | 46 to 62 | crossbeam: 53 to 73 | **crossbeam faster**, 10 to 17% |
| queue, 8 producers + 1 consumer | 7.4 | crossbeam: 12.8 | **crossbeam faster**, about 70% |
| `ScqQueue` against `LockFreeQueue` | 7 to 13 | `LockFreeQueue`: 51 to 127 | **5 to 8 times slower** |
| deque, owner push + pop | 51 | crossbeam-deque: 48 | level |
| deque, one thief draining | 85 | crossbeam-deque: 70 | **parkring faster**, 21% |
| deque, 2 or 4 thieves | 10.5 / 5.5 | crossbeam-deque: 10.5 / 5.7 | level |
| pool, `fib(32)` on 8 threads (time, lower is better) | 1.36 ms | Rayon: 1.36 ms | **level** |
| pool, `join` at every level, 8 threads (time) | 582 µs | Rayon: 374 µs | **Rayon faster** |
| waiting consumer: wake latency / CPU while idle | 9.4 µs / 1.7% | `std::sync::mpsc`: 8.8 µs / 1.3% | level |
| | | crossbeam `ArrayQueue` (spins): 0.3 µs / 100% | crossbeam wakes faster, but burns a core |

### Where parkring wins

* **One producer, one consumer.** `LockFreeQueue` moves 91 million items/s
  against crossbeam's 80 to 85 at capacity 256 (two runs), and 127 against
  121 at capacity 4096.
* **A single thief draining a deque:** 85 against 70 million items/s.
* **Against a mutex queue** (`BlockingQueue`), `LockFreeQueue` moves 7 to 12
  times as many items in every contended shape.

### Where it is level

* **The pool and Rayon** on compute-bound work: `fib(32)` with a sequential
  cutoff takes the same time at 1, 2 and 8 threads, and parkring is faster at
  4. Both reach 5.3 times sequential speed on 8 threads.
* **The deque's owner path** (51 against 48), and **2 or 4 thieves**, where
  both are limited by contention on the same index.
* **Idle cost against a channel.** A parked parkring consumer wakes in about
  9 µs and uses under 2% of a core, like `std::sync::mpsc::sync_channel`.

### Where it loses

* **Contention.** crossbeam's `ArrayQueue` is 10 to 17% faster from 2 + 2
  threads up, and about 70% faster with 8 producers feeding 1 consumer. This
  is tracked in [issue #12](https://github.com/anmol0b/parkring/issues/12).
* **`ScqQueue` is slow.** 7 times slower than `LockFreeQueue` with one producer
  and one consumer, 8 times at 4 + 4. Each item touches two rings and a data
  cell, and on 10 ARM cores a retried CAS is cheap, so the Vyukov queue's
  retries cost less than SCQ's extra work. [SCQ.md §7](SCQ.md) has the
  profile. It is here for its progress guarantee, not for speed.
* **Very fine-grained `join`.** With a `join` at every level of `fib(25)`,
  parkring's pool is faster than Rayon at 1 and 4 threads but slower at 8,
  where its waiting threads spin instead of sleeping. Both are slower than
  sequential code here: a `join` costs more than a two-instruction leaf.
* **Raw wake-up speed against a spinning queue.** crossbeam's `ArrayQueue`
  has no blocking API, so a consumer waiting on it spins. It notices a new
  item in 0.3 µs, 30 times faster than a parked thread, at the cost of a
  whole core while idle. Which matters more depends on the application.

### Not measured yet

* `parkring::channel` against `crossbeam-channel` and `flume`.
* Any x86-64 or Linux machine: every number here is from one Apple M4.
* Latency percentiles under steady load.

All three are tracked in [issue #13](https://github.com/anmol0b/parkring/issues/13).

## Charts

![MPMC scaling](../assets/mpmc_scaling.svg)
![Wake latency vs idle CPU](../assets/wake_latency.svg)
![Pool scaling](../assets/pool_scaling.svg)
![Capacity sweep](../assets/capacity_sweep.svg)
![Asymmetric](../assets/asymmetric.svg)
![SPSC](../assets/spsc.svg)

## What is measured

**Queue throughput** (`crates/parkring-bench/benches/throughput.rs`). For each
queue and shape, P producer and C consumer threads are spawned once and reused.
Each timed iteration releases them through a `Barrier`, moves 262,144 items,
and waits at a second barrier. Criterion reports the median time per
iteration, converted to million items per second (Melem/s) with a 95%
confidence interval. The original benchmark spawned up to 32 threads inside
every timed iteration and moved 100 items per thread, so it mostly measured
`thread::spawn`.

**Wake latency** (`latency.rs`, custom harness). A consumer blocks on an empty
queue; every 2 ms the producer pushes one item and times how long until `pop`
returns. The gap is long enough for parking queues to park, so this is the cost
of waking a parked thread; process CPU time shows what each strategy spends
while idle.

**Deque** (`deque.rs`). Owner-only push then pop of 65,536 values; and
pre-spawned thieves draining 65,536 values (only the draining is timed).

**Pool** (`pool.rs`). `fib(32)` with a sequential cutoff below 20
(compute-bound: the scaling measurement), `fib(25)` with a `join` at every
level (pure `join` overhead), and a chunked sum of 32 MB (memory-bound).

| queue | what it is |
|---|---|
| `lockfree` | this crate's `LockFreeQueue` (Vyukov) |
| `scq` | this crate's `ScqQueue` (Nikolaev's SCQ) |
| `crossbeam` | `crossbeam_queue::ArrayQueue`, waited on with the same spin/yield backoff; **never parks** |
| `blocking` | this crate's `BlockingQueue` |
| `std_sync_channel` | `std::sync::mpsc::sync_channel`, single-consumer shapes only (its receiver is `!Sync`) |

One more note on the deque numbers: crossbeam-deque with unboxed `u64` measured
slower than with `Box<u64>` in every run (28 against 48 million items/s); we
have not investigated why. The 32 MB parallel sum is limited by memory
bandwidth, so no pool speeds it up.

## Full results

### SPSC

| queue | producers | consumers | capacity | Melem/s (median) | 95% CI |
|---|---|---|---|---|---|
| lockfree | 1 | 1 | 16 | 16.0 | 15.9–16.1 |
| crossbeam | 1 | 1 | 16 | 16.9 | 16.7–17.0 |
| blocking | 1 | 1 | 16 | 2.3 | 2.3–2.3 |
| std_sync_channel | 1 | 1 | 16 | 4.1 | 4.1–4.1 |
| scq | 1 | 1 | 16 | 9.0 | 8.9–9.2 |
| lockfree | 1 | 1 | 256 | 91.1 | 90.4–92.0 |
| crossbeam | 1 | 1 | 256 | 84.8 | 83.9–85.6 |
| blocking | 1 | 1 | 256 | 12.2 | 12.1–12.5 |
| std_sync_channel | 1 | 1 | 256 | 29.3 | 28.4–29.5 |
| scq | 1 | 1 | 256 | 11.9 | 11.8–12.0 |
| lockfree | 1 | 1 | 4096 | 127.2 | 125.1–128.5 |
| crossbeam | 1 | 1 | 4096 | 120.7 | 119.7–121.8 |
| blocking | 1 | 1 | 4096 | 14.0 | 13.2–16.2 |
| std_sync_channel | 1 | 1 | 4096 | 64.5 | 60.4–64.8 |
| scq | 1 | 1 | 4096 | 12.8 | 12.5–13.3 |

### MPMC scaling

| queue | producers | consumers | capacity | Melem/s (median) | 95% CI |
|---|---|---|---|---|---|
| lockfree | 1 | 1 | 256 | 91.1 | 90.8–91.5 |
| crossbeam | 1 | 1 | 256 | 79.8 | 79.2–80.0 |
| blocking | 1 | 1 | 256 | 14.6 | 14.3–15.2 |
| std_sync_channel | 1 | 1 | 256 | 29.4 | 29.2–29.5 |
| scq | 1 | 1 | 256 | 12.6 | 12.3–12.8 |
| lockfree | 2 | 2 | 256 | 62.2 | 59.8–64.0 |
| crossbeam | 2 | 2 | 256 | 72.7 | 59.9–75.3 |
| blocking | 2 | 2 | 256 | 7.9 | 7.7–8.4 |
| scq | 2 | 2 | 256 | 7.3 | 7.2–7.4 |
| lockfree | 4 | 4 | 256 | 48.7 | 46.8–50.2 |
| crossbeam | 4 | 4 | 256 | 56.7 | 55.4–57.4 |
| blocking | 4 | 4 | 256 | 6.4 | 6.4–6.5 |
| scq | 4 | 4 | 256 | 6.1 | 6.0–6.3 |
| lockfree | 8 | 8 | 256 | 51.4 | 50.3–52.2 |
| crossbeam | 8 | 8 | 256 | 56.5 | 55.0–57.2 |
| blocking | 8 | 8 | 256 | 6.1 | 5.9–6.6 |
| scq | 8 | 8 | 256 | 6.8 | 6.6–7.0 |
| lockfree | 16 | 16 | 256 | 46.0 | 39.3–48.3 |
| crossbeam | 16 | 16 | 256 | 53.2 | 46.5–55.0 |
| blocking | 16 | 16 | 256 | 4.3 | 4.1–4.6 |
| scq | 16 | 16 | 256 | 6.9 | 6.6–7.2 |

### Asymmetric

| queue | producers | consumers | capacity | Melem/s (median) | 95% CI |
|---|---|---|---|---|---|
| lockfree | 2 | 8 | 256 | 17.5 | 15.8–18.6 |
| crossbeam | 2 | 8 | 256 | 19.3 | 18.3–23.5 |
| blocking | 2 | 8 | 256 | 2.3 | 2.3–2.4 |
| scq | 2 | 8 | 256 | 2.0 | 1.9–2.1 |
| lockfree | 8 | 1 | 256 | 7.4 | 7.1–7.9 |
| crossbeam | 8 | 1 | 256 | 12.8 | 12.0–14.9 |
| blocking | 8 | 1 | 256 | 0.6 | 0.6–0.6 |
| std_sync_channel | 8 | 1 | 256 | 2.5 | 2.4–2.5 |
| scq | 8 | 1 | 256 | 1.5 | 1.5–1.6 |
| lockfree | 8 | 2 | 256 | 18.3 | 17.5–19.5 |
| crossbeam | 8 | 2 | 256 | 22.4 | 21.4–24.1 |
| blocking | 8 | 2 | 256 | 2.3 | 2.2–2.3 |
| scq | 8 | 2 | 256 | 2.1 | 2.1–2.1 |

### Capacity sweep

| queue | producers | consumers | capacity | Melem/s (median) | 95% CI |
|---|---|---|---|---|---|
| lockfree | 4 | 4 | 16 | 19.5 | 19.2–19.7 |
| crossbeam | 4 | 4 | 16 | 19.2 | 19.1–19.4 |
| blocking | 4 | 4 | 16 | 1.0 | 1.0–1.0 |
| scq | 4 | 4 | 16 | 5.4 | 5.4–5.7 |
| lockfree | 4 | 4 | 256 | 48.1 | 46.4–49.1 |
| crossbeam | 4 | 4 | 256 | 55.4 | 53.8–56.0 |
| blocking | 4 | 4 | 256 | 6.6 | 6.5–6.6 |
| scq | 4 | 4 | 256 | 6.4 | 6.2–6.4 |
| lockfree | 4 | 4 | 4096 | 83.2 | 81.7–86.2 |
| crossbeam | 4 | 4 | 4096 | 76.8 | 75.5–78.3 |
| blocking | 4 | 4 | 4096 | 9.6 | 9.4–9.8 |
| scq | 4 | 4 | 4096 | 6.2 | 6.1–6.3 |

### Deque: owner push + pop

| implementation | parameter | median | throughput |
|---|---|---|---|
| crossbeam | Box<u64> | 1.37 ms | 47.9 Melem/s |
| crossbeam | u64 | 2.37 ms | 27.6 Melem/s |
| parkring | u64 | 1.29 ms | 50.8 Melem/s |

### Deque: thieves draining 65,536 items

| implementation | parameter | median | throughput |
|---|---|---|---|
| crossbeam_box | 1_thieves | 943.2 µs | 69.5 Melem/s |
| crossbeam_box | 2_thieves | 6.24 ms | 10.5 Melem/s |
| crossbeam_box | 4_thieves | 11.43 ms | 5.7 Melem/s |
| parkring | 1_thieves | 767.9 µs | 85.3 Melem/s |
| parkring | 2_thieves | 6.23 ms | 10.5 Melem/s |
| parkring | 4_thieves | 11.92 ms | 5.5 Melem/s |

### Pool: fib(32), sequential below 20

| implementation | parameter | median | throughput |
|---|---|---|---|
| parkring | 1 | 7.10 ms |  |
| parkring | 2 | 3.58 ms |  |
| parkring | 4 | 2.07 ms |  |
| parkring | 8 | 1.36 ms |  |
| rayon | 1 | 7.09 ms |  |
| rayon | 2 | 3.58 ms |  |
| rayon | 4 | 2.42 ms |  |
| rayon | 8 | 1.36 ms |  |
| sequential | 1 | 7.22 ms |  |

### Pool: fib(25) with join at every level (overhead)

| implementation | parameter | median | throughput |
|---|---|---|---|
| parkring | 1 | 1.13 ms |  |
| parkring | 4 | 446.8 µs |  |
| parkring | 8 | 581.5 µs |  |
| rayon | 1 | 1.29 ms |  |
| rayon | 4 | 538.0 µs |  |
| rayon | 8 | 374.0 µs |  |
| sequential | 1 | 283.5 µs |  |

### Pool: parallel sum of 4 M u64s (memory-bound)

| implementation | parameter | median | throughput |
|---|---|---|---|
| parkring | 1 | 640.8 µs |  |
| parkring | 4 | 602.3 µs |  |
| parkring | 8 | 658.0 µs |  |
| rayon | 1 | 654.2 µs |  |
| rayon | 4 | 591.8 µs |  |
| rayon | 8 | 687.0 µs |  |
| sequential | 1 | 613.9 µs |  |

### Wake latency

| queue | p50 (µs) | p90 (µs) | p99 (µs) | CPU while idle |
|---|---|---|---|---|
| lockfree | 9.4 | 13.1 | 19.5 | 1.7% |
| scq | 9.9 | 13.7 | 22.0 | 1.7% |
| crossbeam | 0.3 | 3.4 | 5.4 | 100.0% |
| blocking | 9.8 | 14.0 | 24.8 | 1.4% |
| std_sync_channel | 8.8 | 12.8 | 18.4 | 1.3% |

## Caveats

* macOS offers no thread pinning, and the scheduler moves threads between
  performance and efficiency cores. Variance shows in the confidence
  intervals, especially above 8 threads (16 + 16 is 32 threads on 10 cores).
* Absolute numbers depend on the machine. Compare within one run, and use
  `--save-baseline` / `--baseline` for before-and-after measurements.
* Queue items are `u64`; larger items shift cost toward copying.
