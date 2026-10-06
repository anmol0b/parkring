# Design

This document explains how `LockFreeQueue` works, why each memory ordering is
what it is, and how the claims are verified. `BlockingQueue` is a textbook
mutex-plus-two-condvars queue and is covered only where it differs.

## 1. Layout

```text
LockFreeQueue<T>
├── head: CachePadded<AtomicUsize>        next position to pop
├── tail: CachePadded<AtomicUsize>        next position to push; top bit = closed
├── consumers: WaitQueue                  consumers parked on "empty"
├── producers: WaitQueue                  producers parked on "full"
└── slots: Box<[Slot<T>]>                 capacity = power of two, at least 2
      └── Slot { sequence: AtomicUsize, value: UnsafeCell<MaybeUninit<T>> }
```

`head` and `tail` sit on separate 128-byte lines. Apple Silicon has 128-byte
cache lines, and on x86-64 the adjacent-line prefetcher pulls lines in pairs,
so 64-byte padding still false-shares on both.

## 2. The sequence protocol

Positions increase forever; position `p` lives in slot `p & (capacity - 1)`.
Each slot's `sequence` says which position it is ready for:

| `sequence` for the slot of position `p` | meaning |
|---|---|
| `p` | empty for this lap: a producer at `p` may write |
| `p + 1` | holds the item pushed at `p`: a consumer at `p` may read |
| `p + capacity` | consumed, and now empty for position `p + capacity` |

### `try_push` decision table

A producer loads `tail = p`, then `seq` of slot `p` (`Acquire`), and compares:

| `seq - p` | situation | action |
|---|---|---|
| `== 0` | slot free for this lap | CAS `tail` `p → p+1`; on success write, then `seq = p+1` (`Release`) |
| `< 0` | slot still holds last lap's item | if `tail` is still `p` and `head` is a full lap behind, return `Full`; otherwise a consumer is mid-recycle, so back off and retry |
| `> 0` | another producer already took `p` | back off, reload `tail`, retry |

`try_pop` mirrors this against `p + 1`, returning `Empty` only when `tail == head`.

The first version collapsed the `< 0` and `> 0` rows into "full", and also
reported "full" whenever it lost the CAS. Under contention, `try_push` failed on
queues with plenty of room.

### Happens-before

```text
Producer (position p)                        Consumer (position p)
  tail.load(Relaxed) == p
  seq.load(Acquire) == p   <── sync ───────  seq.store(p, Release)       [previous lap's consumer]
  tail.CAS(p, p+1, AcqRel)
  slot.write(item)
  seq.store(p+1, Release)  ─── sync ──────>  seq.load(Acquire) == p+1
                                             head.CAS(p, p+1, AcqRel)
                                             item = slot.read()
                                             seq.store(p+cap, Release) ── sync ──> next lap's producer
```

* **Publication.** The consumer reads the slot only after its `Acquire` load sees
  the producer's `Release` store of `p+1`, so the write happens-before the read.
* **Recycling.** The next-lap producer writes only after its `Acquire` load sees
  the consumer's `Release` store of `p+cap`, so the read happens-before the
  overwrite.
* **Exclusivity.** The `tail` CAS is the only way to claim position `p`, and
  `seq` changes only after a successful claim, so exactly one producer ever
  touches the slot for `p`. The same holds for `head` and consumers.

`head` and `tail` guard no data of their own, which is why they can be loaded
`Relaxed`. The CAS is `AcqRel` because of the parking protocol (section 4), not
because of the slot protocol.

## 3. Positions and the closed flag

The top bit of `tail` is the closed flag. Positions therefore live in 63 bits
(31 on 32-bit targets). Every increment masks the flag out (`pos_add`), and
every comparison is a sign-extended 63-bit difference (`pos_diff`), so
positions wrap correctly. Capacity is a power of two, so it divides `2^63` and
`p & mask` stays consistent across the wrap. A unit test starts the queue three
positions before the wrap and runs five full laps across it.

**Why a mark bit rather than an `AtomicBool`?** With a separate flag, a producer
can read `closed == false`, a closer can set it, a consumer can observe "closed
and `head == tail`" and return `Closed`, and only then does the producer's CAS
succeed. That item is stranded until `Drop`. With the flag inside `tail`, the
producer's CAS expects a value without the bit, so every CAS after
`fetch_or(CLOSED_BIT)` in `tail`'s modification order fails, reloads, and sees
the flag. Close is linearizable against push using nothing but single-word
coherence. crossbeam-channel's array flavor uses the same trick.

## 4. Spin, then park

`push` and `pop` retry with exponential backoff: 127 spin hints, then up to four
`yield_now` calls, then they park on a `Condvar`. Inside `try_*`, a thread
waiting for a peer that is mid-operation spins, because that peer finishes
within nanoseconds. A thread holding a stale position snoozes. This matches
crossbeam, and measuring the reverse cost about 25% at 4+4. The original code yielded
forever, so a consumer waiting on an idle queue kept a core busy indefinitely.
Now it measures at about 50 µs of CPU over 300 ms; `tests/cpu_burn.rs` checks
this with `getrusage` against a spinning control thread.

### The lost-wakeup problem

The dangerous interleaving is:

1. the consumer sees an empty queue;
2. a producer pushes and checks for parked consumers, finding none;
3. the consumer parks, and nobody will ever wake it.

`WaitQueue` rules this out in two steps.

**Registration versus the notifier's check.** The waiter increments `waiters`,
then re-checks the queue with an `AcqRel` read-modify-write on `tail`
(`fetch_add(0)`) rather than a load. The producer advances `tail` with an
`AcqRel` CAS, then loads `waiters`. Both are RMWs on `tail`, so one precedes
the other in `tail`'s modification order:

* If the producer's CAS is first, the waiter's RMW reads it, because an RMW
  always reads the latest value. The waiter sees the item and does not park.
* If the waiter's RMW is first, the producer's CAS reads from that RMW's release
  sequence and synchronises with it. The registration was sequenced before the
  RMW, so it happens-before the producer's `waiters` load, which must see it.
  The producer then notifies.

**Re-check versus blocking.** On Linux, Android and macOS the waiter sleeps on
a futex word, `epoch`, that every notification increments. The waiter reads
`seen = epoch` *before* its re-check and then asks the kernel to sleep only
while `epoch == seen`. A notifier increments `epoch` after its CAS, so a
notification that lands between the re-check and the sleep has already changed
the word, and the kernel's compare-and-sleep refuses to block. The waiter's
read of `seen` happens-before the notifier's increment whenever the second
case above applies, so by coherence `seen` is older.

On every other platform, and under `--cfg parkring_force_condvar`, a
`Mutex<()>` + `Condvar` does the same job: the waiter holds the mutex from
registration until `Condvar::wait` atomically releases it, and `notify_one`
takes the same mutex. That mutex protects no data; it only makes "re-check,
then block" atomic with respect to notify.

**Skipping the wake system call.** A registered waiter is often still awake:
spinning, re-checking, or just woken by an earlier notification. The first
futex version called `futex_wake` whenever any thread was registered, which
cost one system call per queue operation under churn and made throughput worse
than the condvar version. The fix counts threads actually about to sleep
(`sleepers`) and wakes only if that count is non-zero. That opens a second
store-buffering race (the notifier writes `epoch` then reads `sleepers`; the
waiter writes `sleepers` then the kernel reads `epoch`), closed by one
`fence(SeqCst)` on each side. Both fences are on the slow path. Removing
either one makes loom report a deadlock.

**Hot-path cost.** When nobody is parked, a successful push or pop pays one
`Relaxed` load of a read-mostly counter on its own cache line. When a thread is
registered, the notifier pays an increment and a fence, and a system call only
if someone is asleep.

**Measured effect.** On an Apple M4, parking on `__ulock` instead of a pthread
condvar lowers the median wake latency of a parked consumer by about 10%
(8.5–8.8 µs against 9.4–10.0 µs over three interleaved runs), with a lower p99
in every run. The floor is the kernel waking an idle core, which no parking
scheme avoids. Contended throughput is unchanged within noise.

**Why `__ulock` on macOS.** It is a private but ABI-stable libSystem call
(libc++ implements `std::atomic::wait` on it). Rust's standard library avoids it
only because App Store review rejects private symbols; a crate user building an
App Store app can pass `--cfg parkring_force_condvar`. Miri has no `__ulock`
shim, so Miri on macOS uses the condvar fallback; CI runs Miri on Linux, which
interprets the real `futex` system call.

### How loom changed this design

The first version used a textbook store-buffering argument: `SeqCst` CAS then
`SeqCst` load of `waiters` on one side, `SeqCst` increment then `SeqCst` load
of `tail` on the other. All four sit in the single `SeqCst` total order, which
forbids the lost wakeup. That argument is correct in C11, but loom treats
`SeqCst` accesses as `AcqRel` (a documented limitation) and reported a
deadlock. Rather than suppress the report, the protocol was restructured so its
correctness rests on RMW and release-sequence semantics that loom does model.
It also moved `SeqCst` off the hot path entirely.

### Thundering herd

The fast path uses `notify_one`: each push wakes at most one consumer. A woken
consumer that loses the item to a spinning one re-checks and parks again. A
woken thread stays counted in `waiters` until it leaves `wait_until`, so a
second push still notifies and reaches the second parked thread. Loom checks
this with two parked consumers and two pushes, and checks that `close` still
serves the second consumer after a push woke the first. `close` uses
`notify_all`.

### Timeouts

`push_timeout` and `pop_timeout` pass a deadline to `wait_until`, which
re-evaluates the condition before the deadline check on every wakeup. After a
timed-out park the queue makes one last `try_*` attempt, so an item that
arrives exactly at the deadline is delivered rather than reported as a timeout.

## 5. Capacity

Capacity is rounded up to a power of two so that the slot index is
`pos & mask`, and **the minimum is 2**. With one slot, the "full" sequence for
lap `n` (`p + 1`) equals the "empty" sequence for lap `n + 1`, so a producer
would overwrite a live item. Vyukov's original asserts `buffer_size >= 2` for
this reason. The drop-accounting tests found it: capacity 1 accepted a second
push on a full queue. The proptest model finds it too, and shrinks the failing
case to capacity 1.

## 6. Memory safety

Slots hold `MaybeUninit<T>`. A value is written only after winning the tail CAS,
read (moved out) only after winning the head CAS, and dropped in place only in
`Drop`. `Drop` has `&mut self`, so no other thread can reach the queue, and it
walks exactly `[head, tail)`: the positions that were published and never
consumed. `MaybeUninit::write` never drops old contents, so each `T` is
destroyed exactly once. Miri checks this with a drop-counting type that owns a
heap allocation, so a double drop is a double free and a missed drop is a leak.

`Slot<T>` is `Sync` when `T: Send`. Items move between threads but no `&T` is
ever shared, so `T: Sync` is not required. `compile_fail` doctests assert that
`Rc<T>` is rejected.

## 7. Progress guarantee

The fast path takes no lock, but the queue is **not lock-free** in the formal
sense. A producer preempted between its CAS and its `Release` publish leaves
that slot claimed but unpublished, and consumers reaching it must wait. This is
inherent to per-slot sequence designs, and crossbeam's `ArrayQueue` has the same
window. Waiting threads back off and then park, so the cost is latency, not a
burned core.

## 8. Verification

| Tool | What it checks | Where |
|---|---|---|
| **loom** | Every interleaving of 2–3 threads, up to a preemption bound, against the C11 model: lost wakeups, close races, spurious `try_*` failures, data races on slot contents | `tests/loom.rs` |
| **Miri** | Undefined behaviour in the `unsafe` code: uninitialised reads, double drops, leaks, aliasing violations, data races | unit, drop, regression, Send/Sync, small concurrent tests |
| **proptest** | Random operation sequences against a `VecDeque` model, capacities 1–17 | `tests/proptest_model.rs` |
| **Concurrent checks** | Exactly-once delivery and per-producer FIFO across a grid of shapes and capacities, blocking and non-blocking | `tests/linearizability.rs`, `tests/sequential_and_concurrent.rs` |
| **Drop accounting** | Constructions equal destructions, including after wraparound, close, and rejection | `tests/drop_semantics.rs` |
| **CPU accounting** | A parked consumer uses under 5% of a core; a spinning control is detected | `tests/cpu_burn.rs` (`--ignored`) |

```sh
cargo test
cargo test --release --test cpu_burn -- --ignored
RUSTFLAGS="--cfg loom" cargo test --release --test loom
MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check" \
  cargo +nightly miri test --lib --test drop_semantics --test send_sync \
  --test regressions --test linearizability
```

Every atomic, mutex, condvar and `UnsafeCell` in the crate goes through
`src/sync/primitives.rs`. Under `--cfg loom` those resolve to loom's versions,
so the code that is model-checked is the code that ships. CI rejects any use of
the std primitives outside that file. Under loom, `Backoff` yields on every
step and completes immediately, so blocked threads reach the parking path at
once.

**What loom cannot tell us.** Loom treats `SeqCst` accesses as `AcqRel`, does
not model `Instant` (so timeouts are unreachable under loom and tested
natively), and does not explore every load-buffering execution. Loom also
cannot bound a model in which one thread spins waiting for a preempted peer
while others keep yielding. That is the non-lock-free window from section 7,
and it forces a lower preemption bound for the three-thread scenario; see the
comment on that test. The native concurrent tests and Miri's weak-memory
emulation cover what loom cannot.

## 9. What the first version got wrong

The first version (commit `76feaa4`, published as `bounded_mpmc_queue` 0.1)
passed its own tests. The audit that led to this rewrite found:

| Problem | Evidence | Fix |
|---|---|---|
| `try_push`/`try_pop` failed spuriously under contention | 65 rejected pushes on a 64-slot queue that never held more than 32 items | three-way comparison (section 2); regression test and loom test |
| Capacity was not rounded, despite the README | capacity 3 accepted three pushes and returned none of them; capacity 6 hung | round to a power of two; proptest over capacities 1–17 |
| Capacity 1 overwrote live items | found by the new drop-accounting tests | minimum capacity 2 (section 5) |
| Blocked threads spun forever | a waiting consumer kept a core busy | spin, then park (section 4) |
| No way to shut down | blocked consumers could never exit | `close()` with drain semantics |
| "`AcqRel` beats `SeqCst` on Apple Silicon" | both compile to the same instructions for a CAS on AArch64 and x86-64 | claim removed; the measured gain came from `&` indexing |
| Benchmarks timed `thread::spawn` | each iteration spawned up to 32 threads to move 100 items each | pre-spawned worker pool, 262,144 items per iteration |
| 64-byte padding on a 128-byte-line machine | `head` and `tail` could share a line on Apple Silicon | 128-byte alignment on x86-64 and AArch64 |
