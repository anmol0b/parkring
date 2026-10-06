# parkring

[![CI](https://github.com/anmol0b/parkring/actions/workflows/ci.yml/badge.svg)](https://github.com/anmol0b/parkring/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/parkring.svg)](https://crates.io/crates/parkring)
[![docs.rs](https://img.shields.io/docsrs/parkring)](https://docs.rs/parkring)
![MSRV 1.85](https://img.shields.io/badge/MSRV-1.85-blue)
![License: MIT](https://img.shields.io/badge/license-MIT-blue)

Bounded channels and queues, a work-stealing deque and a work-stealing thread
pool for Rust. Each one is implemented from its paper, checked with the
[loom](https://docs.rs/loom) model checker, Miri, fuzzing and
ThreadSanitizer, and [benchmarked](docs/BENCHMARKS.md) against the crate you
would otherwise use.

A thread that has to wait (for an empty queue, a full channel, or work in the
pool) spins briefly and then sleeps on a futex (`futex(2)` on Linux,
`__ulock` on macOS, `Condvar` elsewhere), so an idle thread uses no CPU. The
only dependency is `libc`.

* [Install](#install)
* [Quick start](#quick-start)
* [Using parkring](#using-parkring): [channels](#channels),
  [queues](#queues), [shutdown and timeouts](#shutdown-and-timeouts),
  [the work-stealing deque](#the-work-stealing-deque),
  [the thread pool](#the-thread-pool)
* [Choosing a type](#choosing-a-type)
* [Performance](#performance)
* [How it is verified](#what-the-verification-found)

## Install

```toml
[dependencies]
parkring = "1"
```

Rust 1.85 or newer. Works on any target with `std`; see
[Stability](#stability) for the platforms CI tests.

## Quick start

A bounded channel between worker threads. When every sender is dropped, the
receivers finish what was sent and their loops end:

```rust
use std::thread;
use parkring::channel;

let (tx, rx) = channel::bounded(64);

// Four producers, each with its own clone of the sender.
let producers: Vec<_> = (0..4u64)
    .map(|id| {
        let tx = tx.clone();
        thread::spawn(move || {
            for i in 0..1_000 {
                tx.send(id * 1_000 + i).unwrap(); // sleeps while the channel is full
            }
        })
    })
    .collect();
drop(tx); // only the producers' clones keep the channel open now

// `iter()` ends once all senders are gone and the channel is empty.
let total: u64 = rx.iter().sum();
assert_eq!(total, (0..4_000).sum());
for p in producers {
    p.join().unwrap();
}
```

## Using parkring

### Channels

`channel::bounded(n)` returns a `Sender` and a `Receiver`. Clone either one to
send or receive from more threads; each message goes to exactly one receiver.

| method | when the channel is full / empty | after the other side is gone |
|---|---|---|
| `send(msg)` / `recv()` | sleeps until there is room / a message | `Err`; `send` hands the message back |
| `try_send(msg)` / `try_recv()` | returns `Full` / `Empty` at once | returns `Disconnected` |
| `send_timeout` / `recv_timeout` | waits up to the timeout, then `Timeout` | returns `Disconnected` |
| `iter()` / `try_iter()` | blocks / stops | ends after the last message |

Disconnection is driven by dropping handles, so there is no `close` to call:

* drop the **last `Sender`**: receivers still get every message that was
  sent, then `recv` returns `Err(RecvError)`;
* drop the **last `Receiver`**: `send` fails immediately and returns your
  message inside `SendError`.

```rust
use parkring::channel::{self, TrySendError};

let (tx, rx) = channel::bounded(2);
tx.try_send("a").unwrap();
tx.try_send("b").unwrap();
assert_eq!(tx.try_send("c"), Err(TrySendError::Full("c"))); // full: nothing waits

assert_eq!(rx.recv(), Ok("a"));
drop(rx); // the only receiver
assert_eq!(tx.send("d").unwrap_err().into_inner(), "d"); // you get "d" back
```

A channel holds *at least* the capacity you ask for; `capacity()` tells you
the real size. It is bounded only, and there is no `select`; for those, see
[when to use something else](#when-to-use-something-else).

### Queues

The queues are the layer below the channel: one value that all threads share
by reference, with an explicit `close`. Use them when you want that shared
object (in an `Arc`, a `static`, or borrowed by scoped threads) rather than
sender and receiver handles.

```rust
use std::sync::Arc;
use std::thread;
use parkring::LockFreeQueue;

let queue = Arc::new(LockFreeQueue::new(256));

let consumer = {
    let queue = Arc::clone(&queue);
    thread::spawn(move || {
        let mut sum = 0u64;
        while let Ok(v) = queue.pop() { // sleeps while empty
            sum += v;
        }
        sum // `pop` fails once the queue is closed *and* drained
    })
};

for i in 0..10_000 {
    queue.push(i).unwrap(); // sleeps while full
}
queue.close();
assert_eq!(consumer.join().unwrap(), (0..10_000).sum());
```

There are three queues with the same API:

* **`LockFreeQueue`**: the one to use. Dmitry Vyukov's bounded ring: no locks
  on the fast path, and waiting threads park.
* **`ScqQueue`**: Nikolaev's SCQ, with a stronger progress guarantee (some
  thread always completes its operation, even if others are suspended). It is
  several times slower here; use it only if you need that guarantee. 64-bit
  targets only.
* **`BlockingQueue`**: one mutex and two condition variables. The simple
  reference version.

All three implement the `BoundedQueue` trait, so code can be written once for
any of them (`fn run<Q: BoundedQueue<Job>>(queue: &Q)`), or take a
`&dyn BoundedQueue<Job>`.

| method | queue full / empty | queue closed |
|---|---|---|
| `push(item)` / `pop()` | sleeps | `push`: `Err(PushError(item))`. `pop`: drains what is left, then `Err(PopError)` |
| `try_push` / `try_pop` | `Err(TryPushError::Full(item))` / `Err(TryPopError::Empty)` | `Closed(item)` / `Closed` once drained |
| `push_timeout` / `pop_timeout` | waits up to the timeout, then `Timeout` | `Closed` |
| `close()` | wakes every waiting thread; returns `true` the first time | |

Every error from a push gives the item back (`into_inner()`), so nothing is
ever dropped silently.

### Shutdown and timeouts

Timeouts let a producer notice that consumers have fallen behind, and let a
consumer do other work while the queue is idle. `close` (or dropping the last
sender) stops new work without losing what was already accepted:

```rust
use std::time::Duration;
use parkring::{LockFreeQueue, PopTimeoutError, PushTimeoutError};

let jobs = LockFreeQueue::new(2);
jobs.push(1).unwrap();
jobs.push(2).unwrap();

// Full: give up after 10 ms instead of blocking, and keep the job.
match jobs.push_timeout(3, Duration::from_millis(10)) {
    Err(PushTimeoutError::Timeout(job)) => assert_eq!(job, 3), // shed, retry or log it
    other => panic!("unexpected {other:?}"),
}

jobs.close();
// Items accepted before `close` are still delivered...
assert_eq!(jobs.pop_timeout(Duration::from_millis(10)), Ok(1));
assert_eq!(jobs.pop_timeout(Duration::from_millis(10)), Ok(2));
// ...then the queue reports that it is closed and empty.
assert_eq!(jobs.pop_timeout(Duration::from_millis(10)), Err(PopTimeoutError::Closed));
```

[`examples/shutdown.rs`](examples/shutdown.rs) shows a full producer and
consumer doing this with load shedding.

### The work-stealing deque

A `Worker` is a deque owned by one thread: it pushes and pops at one end,
newest first. Any number of `Stealer` handles take from the other end, oldest
first. This is the building block of work-stealing schedulers: each thread
works through its own tasks and steals from the others when it runs out.

```rust
use parkring::{Steal, Worker};

let worker = Worker::new();
let stealer = worker.stealer(); // `Clone`; send it to other threads

worker.push(1);
worker.push(2);
worker.push(3);

assert_eq!(worker.pop(), Some(3));            // owner: newest first
assert_eq!(stealer.steal(), Steal::Success(1)); // thief: oldest first
```

`steal` can return `Steal::Retry` when it loses a race with another thread;
the deque may still have work, so try again. A `Worker` can be moved to
another thread but not shared (`Send`, not `Sync`); a `Stealer` can be both.
[`examples/scheduler.rs`](examples/scheduler.rs) is a small scheduler built
this way.

### The thread pool

`ThreadPool` runs closures on a fixed set of worker threads that steal work
from each other. Inside the pool, `join(a, b)` runs `a` and `b`, possibly in
parallel: `b` is offered to idle workers, and if nobody takes it, the current
thread runs it too. Recursive `join` spreads divide-and-conquer work across
the pool without any queues of your own.

```rust
use parkring::{ThreadPool, join};

fn sum(values: &[u64]) -> u64 {
    if values.len() <= 1_000 {
        return values.iter().sum(); // small enough: do it here
    }
    let (left, right) = values.split_at(values.len() / 2);
    let (a, b) = join(|| sum(left), || sum(right));
    a + b
}

let pool = ThreadPool::new(4);
let values: Vec<u64> = (0..100_000).collect();
let total = pool.install(|| sum(&values)); // run on the pool and wait
assert_eq!(total, (0..100_000).sum());
```

* `install(f)` runs `f` on the pool and returns its result. A panic inside
  `f` is passed back to the caller.
* `join(a, b)` outside any pool simply runs `a` and then `b`.
* `spawn(f)` runs `f` in the background without waiting. Dropping the pool
  waits for spawned jobs. A panic in a spawned job aborts the process, since
  there is nobody to return it to.

[`examples/quicksort.rs`](examples/quicksort.rs) sorts in parallel with
`join`.

## Examples

```sh
cargo run --release --example pipeline    # three stages joined by channels
cargo run --release --example quicksort   # parallel sort with join
cargo run --release --example scheduler   # a scheduler on Worker/Stealer
cargo run --release --example shutdown    # backpressure, timeouts, close
```

## Choosing a type

| you want | use |
|---|---|
| to pass messages between threads, with shutdown when one side goes away | `channel::bounded` |
| one shared bounded queue whose waiting threads sleep instead of spinning | `LockFreeQueue` |
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
  `crossbeam-channel` or `flume`.
* **Parallel iterators, or fine-grained `join` at scale:** use Rayon. It
  matches parkring's pool on coarse work and is faster on very fine-grained
  joins at 8 threads.
* **`async` code:** parkring blocks threads; it has no `async` API yet.

## Performance

Measured on an Apple M4; the method, every table and the charts are in
[docs/BENCHMARKS.md](docs/BENCHMARKS.md). In short:

* **Faster than crossbeam** with one producer and one consumer (91 against 80
  million items/s), and **faster than crossbeam-deque** with one thief
  draining (85 against 70).
* **Level with Rayon** on compute-bound fork-join work (`fib(32)` on 8
  threads: 1.36 ms each).
* **Slower than crossbeam** under contention (10 to 17% from 2 + 2 threads,
  about 70% with 8 producers and 1 consumer). `ScqQueue` is 5 to 8 times
  slower than `LockFreeQueue`.
* **Idle threads sleep:** a waiting consumer wakes in about 9 µs and uses
  under 2% of a core, like `std::sync::mpsc`. A queue that spins instead
  (crossbeam's `ArrayQueue` has no blocking API) wakes in 0.3 µs but uses a
  whole core.

## What the verification found

The tests were written to fail on real bugs, and they did. Each item links to
the write-up.

* **The first version (0.1)** failed spuriously in `try_push`, lost
  items at non-power-of-two capacities, and spun forever when idle
  ([DESIGN.md §9](docs/DESIGN.md#9-what-the-first-version-got-wrong)).
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

## Verification

| | covers |
|---|---|
| loom | every interleaving (up to a preemption bound) of the parking protocol, close and channel-disconnect races, both queues' claims, the deque's pop/steal races, and the pool's sleep/wake; four deliberately broken builds must fail |
| Miri | the `unsafe` code in every component: uninitialised reads, double drops, leaks, aliasing and data races, under both stacked and tree borrows, with both parkers (Linux's `futex` through Miri's emulation of it), and 16 schedules per test for the channel and the Vyukov queue |
| fuzzing | the queues, the deque (including index wraparound) and the channel against sequential models; one minute per target on every pull request, fifteen minutes nightly |
| ThreadSanitizer | the queue and channel tests with both parkers (TSan cannot model the deque's standalone fences; loom and Miri cover those) |
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

CI runs all of it on Linux, macOS and Windows, and on 32-bit and AArch64
Linux, plus the MSRV, docs, a FreeBSD check, both parkers under loom, the loom
mutants, cargo-deny, semver checks and the public API snapshot. The
`unsafe` code and its invariants are listed in [UNSAFE.md](docs/UNSAFE.md).

## Stability

parkring follows [semver](https://semver.org/). From 1.0, these are promises:

* **The public API** is exactly what `public-api.txt` lists, including which
  types are `Send`, `Sync`, `Unpin` and unwind-safe. CI fails if it changes
  without the file being updated, and a change that breaks it needs a major
  release.
* **`BoundedQueue` is sealed.** Only parkring's queues implement it, so methods
  can be added in minor releases.
* **Error enums and `Steal` are exhaustive**, like `std::sync::mpsc`'s and
  crossbeam's: you can match every variant. A new kind of failure would get a
  new type, not a new variant.
* **Capacity:** a queue or channel holds *at least* the capacity you ask for;
  `capacity()` reports the real number. How much it rounds up is not part of
  the contract.
* **`std` feature:** on by default and currently required. It exists so that
  a future `no_std` mode can be added without breaking anyone.
* **MSRV:** Rust 1.85. Raising it is not a breaking change, but only happens
  in a minor release, never a patch, and is noted in the changelog. parkring
  supports at least the last four stable Rust releases.
* **Platforms:** tested on x86-64 Linux and Windows, AArch64 Linux and
  macOS, and 32-bit i686 Linux;
  `ScqQueue` exists only on 64-bit targets. Other targets with `std` use the
  portable `Mutex` + `Condvar` parker.
* Items marked `#[doc(hidden)]` or behind features whose names start with `__`
  are not public API.

## Documentation

* [DESIGN.md](docs/DESIGN.md): the Vyukov queue, parking, and closing.
* [SCQ.md](docs/SCQ.md): the fetch-add queue, its departures from the paper,
  and why it is slower here.
* [DEQUE.md](docs/DEQUE.md): the work-stealing deque and its memory orderings.
* [POOL.md](docs/POOL.md): the thread pool.
* [BENCHMARKS.md](docs/BENCHMARKS.md): methodology and every measurement.
* [UNSAFE.md](docs/UNSAFE.md): every `unsafe` block, the invariant it relies
  on, and which tool checks it.
* [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md): how to
  run the checks, and how to report a soundness bug privately.

## Project history

This crate began as `bounded_mpmc_queue` 0.1: a mutex queue and a Vyukov
queue. Version 0.2 audited that first version, fixed its bugs and added
parking, shutdown and the verification suite. Version 0.3
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
