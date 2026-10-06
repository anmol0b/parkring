# parkring

[![CI](https://github.com/anmol0b/parkring/actions/workflows/ci.yml/badge.svg)](https://github.com/anmol0b/parkring/actions/workflows/ci.yml)
![MSRV 1.85](https://img.shields.io/badge/MSRV-1.85-blue)
![License: MIT](https://img.shields.io/badge/license-MIT-blue)

Concurrency primitives in Rust, each implemented from its paper, checked with
the [loom](https://docs.rs/loom) model checker and Miri, and benchmarked
against the established crate for the job.

| | what it is | compared with |
|---|---|---|
| `LockFreeQueue` | Vyukov's bounded MPMC ring with spin-then-park waiting | crossbeam `ArrayQueue` |
| `channel::bounded` | `Sender`/`Receiver` on that queue, disconnecting when either side drops | |
| `ScqQueue` | Nikolaev's SCQ (DISC 2019): fetch-add claims, genuinely lock-free | the Vyukov queue |
| `BlockingQueue` | mutex + two condvars, the reference implementation | |
| `Worker` / `Stealer` | Chase-Lev work-stealing deque with weak-memory-correct fences | crossbeam-deque |
| `ThreadPool`, `join` | a work-stealing pool built from the pieces above | Rayon |

Waiting threads spin briefly, then park on a futex (`futex(2)` on Linux,
`__ulock` on macOS; `Condvar` elsewhere), so an idle thread uses no CPU. The
only dependency is `libc`.

```rust
use parkring::{LockFreeQueue, ThreadPool, join};

// A bounded queue with shutdown: consumers drain, then stop.
let queue = LockFreeQueue::new(1024);
std::thread::scope(|s| {
    let consumer = s.spawn(|| {
        let mut sum = 0u64;
        while let Ok(v) = queue.pop() {   // parks while empty
            sum += v;
        }
        sum                               // Err(PopError) once closed and drained
    });
    for i in 0..10_000 {
        queue.push(i).unwrap();           // parks while full
    }
    queue.close();
    assert_eq!(consumer.join().unwrap(), (0..10_000).sum());
});

// Fork-join parallelism on a work-stealing pool.
fn fib(n: u64) -> u64 {
    if n < 20 {
        return if n < 2 { n } else { fib(n - 1) + fib(n - 2) };
    }
    let (a, b) = join(|| fib(n - 1), || fib(n - 2));
    a + b
}
let pool = ThreadPool::new(4);
assert_eq!(pool.install(|| fib(25)), 75_025);
```

## Install

```toml
[dependencies]
parkring = "0.4"
```

Runnable examples are in [`examples/`](examples): a multi-stage pipeline,
parallel quicksort with `join`, a small work-stealing scheduler, and
backpressure with graceful shutdown. Run one with
`cargo run --release --example pipeline`.

## Choosing a type

| you want | use |
|---|---|
| a bounded MPMC channel that disconnects when one side drops | `channel::bounded` |
| a bounded MPMC queue whose waiting threads sleep instead of spinning | `LockFreeQueue` |
| a queue with a lock-free progress guarantee, and you accept lower throughput | `ScqQueue` (64-bit targets) |
| the simplest correct queue, for reference or low traffic | `BlockingQueue` |
| per-thread task deques for your own scheduler | `Worker` / `Stealer` |
| fork-join parallelism (`join`, `install`, `spawn`) | `ThreadPool` |

### When to use something else

parkring is small and heavily verified, but the established crates are better
in several places, and the [benchmarks](docs/BENCHMARKS.md) show where:

* **Many producers or consumers at maximum throughput:** crossbeam's
  `ArrayQueue` is 10 to 17% faster from 2 + 2 threads up, and much faster with
  many producers feeding one consumer.
* **`select`, zero-capacity (rendezvous) or unbounded channels:** use
  `crossbeam-channel` or `flume`. `parkring::channel` is bounded only, with no
  `select`.
* **Parallel iterators, or fine-grained `join` at scale:** use Rayon. It
  matches parkring's pool on coarse work and is faster on very fine-grained
  joins at 8 threads.
* **`async` code:** parkring blocks threads; it has no `async` API yet.

## What the verification found

The tests were written to fail on real bugs, and they did. Each item links to
the write-up.

* **The original take-home submission** failed spuriously in `try_push`, lost
  items at non-power-of-two capacities, and spun forever when idle
  ([DESIGN.md §9](docs/DESIGN.md#9-what-the-original-submission-got-wrong)).
* **A capacity-1 overwrite** in the Vyukov ring, found by drop accounting and
  independently by proptest, which shrank it to capacity 1 (DESIGN.md §5).
* **A lost wakeup loom could not verify**, because loom treats `SeqCst`
  accesses as `AcqRel`. The parking protocol was re-derived on
  read-modify-writes and release sequences, which loom does model, and the hot
  path got cheaper (DESIGN.md §4).
* **A hole in the SCQ paper's threshold bound**: with more threads than
  capacity, an item could be stranded forever. A stress test hung 11 times in
  40; the fix and the reasoning are in [SCQ.md §4](docs/SCQ.md).
* **The classic Chase-Lev double take**: remove either `SeqCst` fence and loom
  produces `an element was taken twice: [0, 1, 1]`. CI builds each mutant and
  requires that failure ([DEQUE.md §3](docs/DEQUE.md)).
* **Two aliasing violations Miri caught and loom could not**: retiring a
  deque buffer through `Box::from_raw` retags memory a thief may still be
  reading ([DEQUE.md §6](docs/DEQUE.md)), and a latch's `&self` argument stayed
  protected while the waiting thread freed it ([POOL.md](docs/POOL.md)).

## Results

Apple M4, million items per second (higher is better) unless stated.
Full tables and methodology: [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

| | parkring | reference |
|---|---|---|
| queue, 1 producer + 1 consumer | 91 (`LockFreeQueue`) | 80 (crossbeam) |
| queue, 8 + 8 | 51 | 57 (crossbeam) |
| queue, 8 + 8, `ScqQueue` | 7 | 51 (`LockFreeQueue`) |
| deque, one thief draining | 85 | 70 (crossbeam-deque) |
| pool, `fib(32)` on 8 threads | 1.36 ms | 1.36 ms (Rayon) |
| parked consumer: wake latency / idle CPU | 9.4 µs / 1.7% | 8.8 µs / 1.3% (std `sync_channel`); 0.3 µs / 100% (crossbeam `ArrayQueue`, which never parks) |

The losses are reported as plainly as the wins: crossbeam's queue is faster
under contention, and SCQ, despite its stronger progress guarantee, is 5–8×
slower than the Vyukov queue on this hardware.
[SCQ.md §7](docs/SCQ.md) profiles why.

![Throughput scaling](assets/mpmc_scaling.svg)
![Wake latency against idle CPU](assets/wake_latency.svg)
![Pool scaling](assets/pool_scaling.svg)

## Verification

| | covers |
|---|---|
| loom | every interleaving (up to a preemption bound) of the parking protocol, close and channel-disconnect races, both queues' claims, the deque's pop/steal races, and the pool's sleep/wake; four deliberately broken builds must fail |
| Miri | the `unsafe` code in every component: uninitialised reads, double drops, leaks, aliasing, data races, and the real `futex` system call on Linux |
| proptest | each queue against a `VecDeque` model (capacities 1–17), the deque against a `VecDeque` with wrapping indices |
| concurrency tests | exactly-once delivery and per-producer FIFO across many shapes; per-thief ordering for the deque; repeated runs to flush out rare schedules |
| `getrusage` | parked queues and an idle pool use about 35–55 µs of CPU over 300 ms |

```sh
cargo test --workspace
RUSTFLAGS="--cfg loom" cargo test -p parkring --release --test loom --test loom_scq --test loom_pool --test loom_channel
RUSTFLAGS="--cfg loom" cargo test -p parkring --release --lib deque
cargo +nightly miri test -p parkring --target x86_64-unknown-linux-gnu
cargo bench -p parkring-bench && cargo run -p parkring-bench --release --example plot
```

CI runs all of it on Linux, macOS and Windows, plus the MSRV, docs, a FreeBSD
check, both parkers under loom, and the loom mutants.

## Documentation

* [DESIGN.md](docs/DESIGN.md): the Vyukov queue, parking, and closing.
* [SCQ.md](docs/SCQ.md): the fetch-add queue, its departures from the paper,
  and why it is slower here.
* [DEQUE.md](docs/DEQUE.md): the work-stealing deque and its memory orderings.
* [POOL.md](docs/POOL.md): the thread pool.
* [BENCHMARKS.md](docs/BENCHMARKS.md): methodology and every measurement.

## Project history

This crate began as a take-home assignment, published as `bounded_mpmc_queue`:
a mutex queue and a Vyukov queue. Version 0.2 audited that submission, fixed
its bugs and added parking, shutdown and the verification suite. Version 0.3
renamed it to `parkring` and added futex parking, SCQ, the work-stealing deque
and the pool. The git history shows each step.

## Layout

```text
src/
  queue/lockfree.rs     LockFreeQueue          queue/scq/     ScqQueue
  queue/blocking.rs     BlockingQueue          deque/         Worker, Stealer
  pool/                 ThreadPool, join       sync/futex/    futex backends
  sync/wait_queue/      parking                sync/primitives.rs  std/loom shim
tests/                  loom, proptest, drop accounting, regressions, CPU checks
crates/parkring-bench/  benchmarks and chart generation (unpublished)
```

## License

Licensed under the [MIT license](LICENSE).
