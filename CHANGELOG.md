# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

Toward 1.0: the API-freeze decisions. See the README's "Stability" section.

### Changed
- **Breaking:** `BoundedQueue` is sealed. Only parkring's queues implement it,
  so methods can be added without a major release.
- `Worker::with_capacity_and_start`, the hidden test constructor, now exists
  only with the `__test-hooks` feature and is gone from normal builds.
- Capacity is documented as "at least the requested capacity"; the exact
  rounding is no longer part of the contract.
- `Steal` is `#[must_use]`.
- The version is `1.0.0-rc.1` while the release is prepared.

### Added
- A default `std` feature. It is currently required (building without it is a
  compile error), so a future `no_std` mode will be purely additive.
- `public-api.txt`, a snapshot of the public API and its auto traits; CI fails
  when the API changes without the snapshot.
- docs.rs marks `ScqQueue` as 64-bit only.
- A "Stability" section in the README: semver, sealed trait, exhaustive
  enums, capacity, MSRV policy and platforms.
- `parkring::channel`: bounded multi-producer multi-consumer channels.
  `channel::bounded(n)` returns a cloneable `Sender` and `Receiver` sharing a
  `LockFreeQueue`. Dropping the last handle on one side disconnects the
  channel: receivers drain what was sent and then get `RecvError`; senders get
  their message back in `SendError`. Errors, `try_*` and `*_timeout` variants,
  and iterators follow `std::sync::mpsc` and `crossbeam-channel` naming.
- loom models for disconnect races (`tests/loom_channel.rs`), run under both
  parkers, and a `channel_no_disconnect` mutant that CI requires loom to catch.
- The channel tests also run under Miri in CI.

### Changed
- The `pipeline` example uses channels, so shutdown follows from dropping
  senders instead of a hand-written counter.

## [0.4.0] - 2026-10-06

The first release prepared for crates.io. No behaviour changes; the public
API is trimmed and documented so it can be kept stable.

### Changed
- **Breaking:** `Backoff` is no longer exported. It was an internal spin
  helper, not part of the queue API. If you used it, copy the 30 lines from
  `src/sync/backoff.rs` or use `crossbeam_utils::Backoff`.
- License: parkring is now licensed under MIT only. Earlier
  releases were MIT OR Apache-2.0.
- Constructors and pure accessors (`new`, `len`, `is_empty`, `stealer`,
  `steal`, `Worker::pop`, `ThreadPool::threads`) are `#[must_use]`.
- `Worker::with_capacity_and_start` stays hidden and is now documented as
  test-only; it may change in any release.
- Missing documentation on a public item is a compile error.

### Added
- Runnable examples in `examples/`: `pipeline`, `quicksort`, `scheduler` and
  `shutdown`. CI runs all four on Linux, macOS and Windows.
- Doc examples for every error type and for `BoundedQueue`.
- README sections on installing, choosing a type, and when to use
  crossbeam, Rayon or a channel instead.
- The README's idle-CPU comparison now includes `std::sync::mpsc::sync_channel`,
  which parks like parkring does, next to the never-parking crossbeam queue.
- docs.rs builds for Linux, macOS and Windows.

## [0.3.0] - 2026-09-29

Renamed to `parkring` and extended from two queues to a small set of
verified concurrency primitives. See the README for what the verification
found along the way.

### Changed
- **Breaking:** the crate is renamed from `bounded_mpmc_queue` to `parkring`.
  Replace `use bounded_mpmc_queue::…` with `use parkring::…`.
- The repository is a Cargo workspace; benchmarks and chart generation live in
  the unpublished `crates/parkring-bench`.
- Parked threads sleep on a futex: `futex(2)` on Linux and Android,
  `__ulock_wait`/`__ulock_wake` on macOS. Other platforms, and builds with
  `--cfg parkring_force_condvar`, keep the `Mutex` + `Condvar` parker. The
  crate now depends on `libc` on those three targets.

### Performance
- Median wake latency of a parked consumer on an Apple M4 drops about 10%
  (8.5–8.8 µs against 9.4–10.0 µs), with a lower p99.
- Benchmarks for the deque against crossbeam-deque and the pool against Rayon;
  `docs/BENCHMARKS.md` reports every result, including the losses.

### Added
- `ScqQueue`: a lock-free bounded MPMC queue after Nikolaev's SCQ (DISC 2019).
  Claims are `fetch_add`s, so contended threads never retry them, and no
  operation waits on a particular other thread. 64-bit targets only. On a
  10-core Apple M4 it is 5–8× slower than `LockFreeQueue`; `docs/SCQ.md`
  explains why, with profiling, and documents a case where the paper's
  threshold bound does not hold (more threads than capacity).
- `Worker` / `Stealer`: a Chase-Lev work-stealing deque with the Lê et al.
  (PPoPP 2013) orderings adapted to C++20. Slots are atomic pointers, so a
  thief's racing read is sound (values are boxed). Loom reproduces the classic
  double take when either `SeqCst` fence is removed; CI requires it. Miri found
  and the retire list now avoids an aliasing violation on grown buffers. See
  `docs/DEQUE.md`.
- `ThreadPool` with `join`, `spawn` and `install`: a work-stealing pool built
  from the crate's own pieces (a Chase-Lev deque per worker, `LockFreeQueue`
  as the injector, futex parking for idle workers). A `join` never allocates;
  dropping the pool runs every spawned job first. See `docs/POOL.md`.
- Loom models for `close` after one of two parked consumers is woken, and for
  a timed pop racing a push. CI runs the loom suite against both parkers.
- CI tests on Windows and type-checks FreeBSD, both on the portable parker.

## [0.2.0] - 2026-09-28

A production-hardening pass over the original take-home submission. See
[docs/DESIGN.md](docs/DESIGN.md#9-what-the-original-submission-got-wrong) for
the audit that motivated it.

### Added
- Spin-then-park waiting in `LockFreeQueue`: blocked `push`/`pop` spin, yield,
  then park on a condition variable instead of burning a core.
- `close()` on both queues. Pushes fail and return the item; pops drain the
  remaining items, then report `PopError`. All blocked threads are woken.
- `push_timeout` and `pop_timeout` on both queues.
- `len`, `is_empty`, `is_full`, `capacity`, `is_closed`, and `Debug`.
- Error types per operation (`PushError`, `TryPushError`, `PushTimeoutError`,
  `PopError`, `TryPopError`, `PopTimeoutError`). Every push error returns the item.
- `BoundedQueue` covers the full API, is object-safe, and both queues implement it.
- Verification: 12 loom models, Miri over the unsafe code, a proptest model
  against `VecDeque`, exactly-once and per-producer FIFO checks, drop
  accounting, and a CPU-usage test for parked threads.
- Benchmarks against crossbeam's `ArrayQueue` and `std::sync::mpsc::sync_channel`,
  a wake-latency benchmark, and an example that regenerates every chart.
- CI: fmt, clippy, tests on Linux and macOS, MSRV, docs, loom, Miri.

### Changed
- **Breaking:** `push`/`pop` return `Result`; `try_pop` returns `Result`.
- **Breaking:** types are re-exported at the crate root; internal modules are private.
- **Breaking:** `BoundedQueue::new` removed; construct through the concrete type.
- `LockFreeQueue` capacity is rounded up to a power of two, minimum 2.
- Slots store `MaybeUninit<T>`; `Drop` releases exactly the unconsumed items.
- Cache padding is 128 bytes on x86-64 and AArch64.
- `BlockingQueue` notifies after releasing its lock.

### Fixed
- `try_push`/`try_pop` no longer fail spuriously under contention.
- Non-power-of-two capacities no longer lose items or deadlock.
- Capacity 1 no longer overwrites a live item.
- Blocked threads no longer spin indefinitely on an idle queue.

## [0.1.0]

Original take-home submission: `BlockingQueue` and `LockFreeQueue` with
criterion benchmarks.
