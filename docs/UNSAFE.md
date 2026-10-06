# `unsafe` inventory

> Line numbers are as of `origin/main` at `9e8af5d` and will drift. Regenerate
> the site list with `rg -n "unsafe" src`. (The `ci/hardening` branch does not
> touch `src/`, so merging it does not move them.)

This file lists every `unsafe` block, `unsafe fn`, `unsafe impl`, `unsafe
trait` and `extern` block in `src/`, what each one relies on, and which tool
exercises it. It is the reviewer's map until an external audit happens. The
workspace lints deny `clippy::undocumented_unsafe_blocks` (so every block and
`unsafe impl` has a `// SAFETY:` comment), `clippy::missing_safety_doc` and
`unsafe_op_in_unsafe_fn`. Loom (`--cfg loom`) checks orderings and data races on
every atomic and `UnsafeCell`, all of which go through
`src/sync/primitives.rs`. Miri checks the real code for UB, aliasing violations
(under both stacked and tree borrows) and leaks, with both parkers, on Linux
only. ThreadSanitizer runs the queue and channel tests natively; it does not
model standalone fences, so the deque and the pool are left to loom and Miri.
cargo-fuzz drives the queues, the deque and the channel from one thread
against a `VecDeque` model. No tool except the native macOS tests runs the
macOS `__ulock` FFI. The coverage table below has the details.

**Counting rules.** A "block" is an `unsafe { .. }` expression. An "`unsafe
fn`" is a function or trait-method declaration with the `unsafe` qualifier
(the `execute: unsafe fn(..)` field type at `pool/job.rs:16` is not counted).
`src/deque/loom_tests.rs` (`unsafe impl Element for Token`, line 21, and its
`from_raw`, line 25) is test code and is excluded. The four hits in
`src/queue/scq/ring.rs` (lines 101, 108, 187, 209) are comments about SCQ's
"unsafe" (IsSafe) mark. They are not Rust `unsafe`.

## Summary

| module | blocks | `unsafe fn` | `unsafe impl` | other | what the unsafe is for |
|---|---:|---:|---:|---|---|
| `queue/slot.rs` | 3 | 3 | 2 (`Send`, `Sync`) | | `MaybeUninit<T>` slot storage in the Vyukov ring; `Sync` without `T: Sync` |
| `queue/lockfree.rs` | 3 | 0 | 0 | | Calls `Slot::write/read/drop_in_place` after winning a CAS, and from `Drop` |
| `queue/blocking.rs`, `queue/ring_buffer.rs` | 0 | 0 | 0 | | none (mutex-protected, safe code) |
| `queue/scq/data.rs` | 3 | 3 | 2 (`Send`, `Sync`) | | `MaybeUninit<T>` data cells owned through an index |
| `queue/scq/mod.rs` | 4 | 0 | 0 | | Cell write/read after taking an index from a ring, and `Drop` |
| `queue/scq/ring.rs`, `entry.rs` | 0 | 0 | 0 | | none, but the soundness of `data.rs` depends on this safe code |
| `deque/mod.rs` | 16 | 4 | 3 (`Element for Box<T>`, `Send`, `Sync` for `Inner`) | `unsafe trait Element` | Elements as raw pointers in atomic slots; buffers freed only in `Drop` |
| `pool/job.rs` | 7 | 7 | 2 (`Send`, `Element` for `JobRef`) | | Type-erased jobs; stack-allocated jobs with an erased lifetime |
| `pool/latch.rs` | 2 | 3 | 0 | | `Latch::set(*const Self)`: the waiter may free the latch during `set` |
| `pool/mod.rs` | 9 | 0 | 0 | | Thread-local `&'static WorkerThread`; creating, running and collecting `StackJob`s. The guard types `ClearCurrent` and `AbortOnUnwind` contain no `unsafe` |
| `sync/futex/linux.rs` | 2 | 0 | 0 | | `futex(2)` via `libc::syscall` |
| `sync/futex/macos.rs` | 2 | 0 | 0 | `unsafe extern "C"` (2 fns) | Private `__ulock_wait` / `__ulock_wake` |
| `sync/primitives.rs` | 0 | 0 | 0 | | none. The `UnsafeCell` wrapper's `with_mut` is a safe fn that returns a raw pointer |
| `sync/` other (`futex/mod.rs`, `wait_queue/*`, `backoff.rs`, `pos.rs`) | 0 | 0 | 0 | | none |
| `channel/` | 0 | 0 | 0 | | none (confirmed: safe wrapper over `LockFreeQueue`) |
| `utils/`, `traits/`, `error.rs`, `lib.rs` | 0 | 0 | 0 | | none |
| **total** | **51** | **20** | **9** | 1 trait, 1 extern block | |

## Tool coverage at a glance

| tool | how CI runs it | what it covers here |
|---|---|---|
| loom | `--test loom --test loom_scq --test loom_pool --test loom_channel` and `--lib deque`, with both the futex and the condvar parker, `LOOM_MAX_PREEMPTIONS=3`. Four mutant builds must fail | Races on `Slot`, `DataCell`, `StackJob.func/result` (all loom `UnsafeCell`s). Deque index protocol (with `Token`, not `Box`). Not the FFI: loom compiles it out. Loom treats SeqCst accesses as AcqRel and models SeqCst fences more strongly than C++20 (DEQUE.md §7) |
| Miri | Linux only, `-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check`, one seed, three jobs: stacked borrows, tree borrows (`-Zmiri-tree-borrows`), and the condvar parker (`--cfg parkring_force_condvar`). Each runs `--lib`, `drop_semantics`, `send_sync`, `regressions`, `linearizability`, `proptest_model`, `deque`, `pool`, `channel` | UB, aliasing, leaks and double drops in all of the above, plus `futex/linux.rs` through Miri's futex emulation (the kernel is never called), and the portable `Mutex` + `Condvar` parker. Not `sequential_and_concurrent.rs`. `regressions::scq_more_threads_than_capacity_never_strands_an_item` is `#[ignore]`d under Miri |
| Miri, many schedules | `-Zmiri-strict-provenance -Zmiri-many-seeds=0..16`, `cargo miri test --test channel --test sequential_and_concurrent lockfree` | 16 schedules of `sequential_and_concurrent`'s `lockfree::*` tests (the Vyukov queue). The trailing `lockfree` is a name filter that cargo passes to both binaries, and no test in `tests/channel.rs` has `lockfree` in its name, so as written this job runs no channel tests. Not the pool, the deque or SCQ |
| ThreadSanitizer | x86_64 Linux, nightly, `-Zbuild-std`, both parkers, `continue-on-error` (informational), `PROPTEST_CASES=32`: `channel`, `sequential_and_concurrent`, `regressions`, `linearizability`, `drop_semantics` | Data races in the Vyukov queue, SCQ, `BlockingQueue` and the channel, running natively (with the futex parker, the real `futex` syscall). TSan does not model standalone fences, so the deque's and the pool's `fence(SeqCst)` pairs are left to loom and Miri, and neither has a TSan job |
| cargo-fuzz | `queue`, `deque` and `channel` targets, 1 min each per PR, 15 min each nightly with a cached corpus. cargo-fuzz's default AddressSanitizer build | Single-threaded operation sequences checked against a `VecDeque` model: `LockFreeQueue`, `BlockingQueue`, `ScqQueue`; the deque's owner and stealer paths from one thread, including indices that start near `usize::MAX` (`with_capacity_and_start`, behind `__test-hooks`); channel handle clones and drops. No concurrency, so it checks index arithmetic and `Box` round-trips, not orderings |
| native tests | `cargo test --workspace --all-targets` on ubuntu, macos and windows (stable and beta); `cargo test -p parkring --all-targets` on i686 Linux and AArch64 Linux | The only tool that runs `futex/macos.rs`. i686 runs `futex/linux.rs` with 32-bit `time_t`; SCQ is compiled out on 32-bit targets, so that leg does not cover it |

---

## `queue/slot.rs` and `queue/lockfree.rs` (Vyukov ring)

Exercised by loom (`tests/loom.rs`: 13 of its 14 models (the other is `BlockingQueue`), notably
`concurrent_try_pop_never_fails_spuriously`, `fifo_across_laps` and
`drop_on_another_thread_sees_published_items`; `tests/loom_channel.rs`;
`tests/loom_pool.rs`, which uses it as the injector). Miri runs
`tests/drop_semantics.rs`, `tests/linearizability.rs`
(`lockfree_small_under_miri`), `tests/proptest_model.rs`,
`tests/regressions.rs`, `tests/channel.rs` and the inline tests
`positions_wrap_around_the_closed_bit` and
`drop_after_wrap_releases_exactly_the_live_items`, and runs
`sequential_and_concurrent`'s `lockfree::*` tests under 16 seeds. TSan runs it
through the queue and channel tests. The `queue` fuzz target drives it from
one thread.

| site | operation | relies on |
|---|---|---|
| `slot.rs:35` `unsafe fn write` | Contract: the caller won the tail CAS after observing `sequence == pos` | Only the claiming producer touches the storage, and the slot is logically empty |
| `slot.rs:37` block | `MaybeUninit::write` through `UnsafeCell` | Exclusive access per the contract. `write` never drops old contents |
| `slot.rs:48` `unsafe fn read` | Contract: the caller won the head CAS after an `Acquire` load saw `sequence == pos + 1` | That load synchronises with the producer's `Release` publish |
| `slot.rs:50` block | `assume_init_read` | Value initialised and visible; caller is the sole reader |
| `slot.rs:59` `unsafe fn drop_in_place` | Contract: initialised, and the caller has exclusive access to the queue (from `Drop` only) | |
| `slot.rs:61` block | `assume_init_drop` | as above |
| `slot.rs:66` `unsafe impl<T: Send> Send for Slot<T>` | A slot owns at most one `T` | |
| `slot.rs:74` `unsafe impl<T: Send> Sync for Slot<T>` | Access to the storage is serialised by the sequence protocol: only the CAS winner touches it. Release/Acquire on `sequence` orders the write before the read, and the read before the next lap's write. No `&T` is ever shared, so `T: Sync` is not required | Positive checks in `tests/send_sync.rs`; `compile_fail` doctests in `lib.rs` reject `Rc` |
| `lockfree.rs:155` `slot.write(item)` in `try_push` | Won the tail CAS (AcqRel) having observed `seq == tail` (Acquire) | The Acquire on `sequence` also orders the previous lap's consumer read before this write |
| `lockfree.rs:213` `slot.read()` in `try_pop` | Won the head CAS having observed `seq == head + 1` with Acquire | |
| `lockfree.rs:422` `slot.drop_in_place()` in `Drop` | `&mut self`. Exactly the positions in `[head, tail)` were published and not consumed. The comment at 410–413 states the assumption that whatever released the last shared reference (an `Arc` drop, a scope join) synchronises with this thread, which justifies the `Relaxed` loads | `debug_assert_eq!(sequence, pos + 1)` at 419, in debug builds only |

Notes. `MaybeUninit::write` and a move cannot panic, so nothing can leave a
claimed slot unpublished while the queue is still reachable. Capacity is
capped at `MAX_CAPACITY` (`lockfree.rs:92`) so that `pos_diff` stays
unambiguous across wraparound. Index correctness depends on that cap and on
the minimum capacity of 2 (DESIGN.md §5).

## `queue/scq/data.rs` and `queue/scq/mod.rs` (SCQ)

Exercised by loom (`tests/loom_scq.rs`, 11 models including
`concurrent_try_pop_takes_distinct_items`, `fifo_across_laps`,
`exhausted_threshold_does_not_strand_an_item` and
`drop_on_another_thread_releases_items`; capacities 1–2). Miri runs
`tests/drop_semantics.rs`, `tests/linearizability.rs` (`scq_small_under_miri`),
`tests/proptest_model.rs` (`scq_matches_vecdeque`), `tests/regressions.rs`
(the SCQ stranding test is ignored under Miri) and the inline tests
`empty_pops_then_push_pop_round_trips`, `full_pushes_then_pop_frees_a_cell`
and `huge_positions_do_not_overflow`, with one seed. TSan runs it through
`sequential_and_concurrent`, `regressions`, `linearizability` and
`drop_semantics`. The `queue` fuzz target drives it from one thread. 64-bit
targets only (`queue/mod.rs:6`).

| site | operation | relies on |
|---|---|---|
| `data.rs:25` `unsafe fn write` | Contract: the caller holds the cell's index, dequeued from the free ring, and the cell is logically empty | |
| `data.rs:27` block | `MaybeUninit::write` | exclusive access per the contract |
| `data.rs:34` `unsafe fn read` | Contract: the caller holds the index, dequeued from the allocated ring (whose Acquire synchronises with the producer's Release publish), and the cell is initialised | |
| `data.rs:36` block | `assume_init_read` | as above |
| `data.rs:42` `unsafe fn drop_in_place` | Contract: initialised, plus exclusive access (from `Drop`) | |
| `data.rs:44` block | `assume_init_drop` | as above |
| `data.rs:49` `unsafe impl<T: Send> Send for DataCell<T>` | A cell owns at most one `T` | |
| `data.rs:55` `unsafe impl<T: Send> Sync for DataCell<T>` | Access is serialised by index ownership. An index moves from writer to reader through an `AcqRel` publish in `aq` and an `Acquire` load, and back through `fq` the same way. No `&T` is shared | |
| `mod.rs:127` `data[index].write(item)` in `try_push` | The index was dequeued from `fq`. `fq`'s Acquire makes the previous reader's read happen-before this write | |
| `mod.rs:136` `data[index].read()` in the `try_push` close path | `aq.enqueue` failed. It fails only when E1's `fetch_add` sees the closed bit, which happens before any entry is written, so the index was never published and the cell is still the caller's | The index is then returned to `fq` with `let _ =`. `fq` is never closable, so that enqueue cannot fail |
| `mod.rs:160` `data[index].read()` in `try_pop` | The index was dequeued from `aq`, whose Acquire load (`ring.rs:181/201`, or a failed CAS's Acquire) synchronises with the enqueuer's AcqRel CAS (E4) | |
| `mod.rs:303` `data[index].drop_in_place()` in `Drop` | `&mut self`: every index is in `fq` or `aq`, and consumed `aq` entries read ⊥. Every non-⊥ entry in `aq` is a live, initialised, distinct cell. `fq` is not walked because its indices are free cells | Unlike `lockfree.rs`, the comment does not state the "last reference synchronised" assumption behind the `Relaxed` loads in `for_each_index` (`ring.rs:296`) |

`ring.rs` and `entry.rs` contain no `unsafe`, but every SCQ safety argument
assumes that the ring hands each index to exactly one thread at a time. That
property is the whole correctness argument of SCQ.md, including the two
departures from the paper (always claim when `tail > head`; always CAS at D5).
`ring.rs:185` guards against a double consume with `debug_assert_ne!` in debug
builds only.

## `deque/mod.rs` (Chase-Lev)

Exercised by loom (`src/deque/loom_tests.rs`: `pop_racing_two_steals_never_double_takes`,
`last_element_goes_to_exactly_one_side`, `growth_concurrent_with_steal`,
`slot_reuse_across_laps`, `two_thieves_and_the_owner`,
`stealer_outlives_worker`; both fence mutants must fail). These models use a
`Token` element carried in the pointer bits, so loom checks the index protocol,
not `Box::from_raw`. Miri runs `tests/deque.rs` (`matches_vecdeque` proptest,
`exactly_once_one_thief`, `exactly_once_many_thieves`,
`leftover_items_are_dropped_exactly_once`,
`concurrent_run_leaves_nothing_behind`), the inline tests
`owner_is_lifo_and_thief_is_fifo`, `grows_and_keeps_order`,
`indices_wrap_around` and `zero_sized_values_work`, and `tests/pool.rs`
(with `E = JobRef`), with one seed. Miri found the retired-buffer retag bug
described in DEQUE.md §6. The `deque` fuzz target drives `push`, `pop` and
`steal` from one thread (with ASan), including wraparound. TSan does not run
the deque.

| site | operation | relies on |
|---|---|---|
| `:27` `unsafe trait Element: Send` | Implementors promise that `from_raw(into_raw(x))` gives back `x` and that `from_raw` is called at most once per `into_raw`, because it transfers ownership | |
| `:31` `unsafe fn from_raw` (trait method) | `ptr` came from `into_raw` and has not been consumed | |
| `:36` `unsafe impl<T: Send> Element for Box<T>` | `Box::leak` / `Box::from_raw` round-trip. Never null (ZSTs get a dangling non-null pointer) | `zero_sized_values_work` |
| `:40` `from_raw` / `:42` block | `Box::from_raw` | the trait contract |
| `:107`, `:109` `unsafe impl Send/Sync for Inner<E>` | Elements are `Send` and move through slots. `retired` (a `UnsafeCell<Vec<*mut Buffer>>`) is touched only by the single owner, which `RawWorker` enforces by being `!Sync` (`PhantomData<Cell<()>>`, `:340`) and not `Clone`, and by `Drop` | `E: Sync` is deliberately not required. A `compile_fail` doctest (`lib.rs:92–95`) asserts `parkring::Worker<u32>` is not `Sync` |
| `:135` `unsafe fn owner_buffer` | Owner only. The pointer is valid until a grow, and retired buffers stay valid until `Drop` | |
| `:138` block | `&*self.buffer.load(Relaxed)` | Buffers are freed only in `Drop` (`&mut self`), so a `&self`-bounded reference cannot dangle. `Relaxed` is enough because the owner is the only writer |
| `:149`, `:152`, `:164`, `:242`, `:254` blocks | Calls to `owner_buffer` / `grow` | "we are the owner": `push`, `pop` and `grow` are reached only through `RawWorker` |
| `:188` `E::from_raw(NonNull::new_unchecked(ptr))` in `pop`, `len > 0` | Index `b` was published by the owner's own push. After the pop fence, with more than one element no thief can reach `b` | Non-null: every published slot holds a pointer from `E::into_raw`, which returns a `NonNull` (stated in the comment) |
| `:198` same, last element | Winning the SeqCst CAS on `top` from `t == b` makes `b` the owner's | as above |
| `:217` `&*self.buffer.load(Acquire)` in `steal` | Buffers are freed only in `Drop`. Acquire pairs with the Release `swap` in `grow`, so the buffer header and copied slots are visible | A stale (retired) buffer still holds correct pointers for `[t, b)`, because grow copies and never clears |
| `:232` `E::from_raw(NonNull::new(ptr).expect(..))` in `steal` | The CAS on `top` gave the thief index `t`. The Acquire loads of `top` and `bottom`, plus the steal fence, make the slot visible | Null is checked and panics |
| `:240` `unsafe fn grow` | Owner only | |
| `:252` block | `(*retired).push(old_ptr)` through `UnsafeCell` | Only the owner touches `retired` outside `Drop` |
| `:270` block | `Box::from_raw(current buffer)` in `Drop` | Allocated by `Box::into_raw`, freed only here |
| `:277` block | `E::from_raw(new_unchecked(ptr))` for `[top, bottom)` in `Drop` | Those indices were pushed and never taken, and the current buffer holds all of them (grow copies `[t, b)`), each a pointer from `E::into_raw` |
| `:284` block | `&mut *retired` | Exclusive access in `Drop` |
| `:288` block | `Box::from_raw(old)` for each retired buffer | Each came from `Box::into_raw`, was retired once, and no thief remains. Retired buffers hold copies of element pointers, so only the arrays are freed |

## `pool/job.rs`, `pool/latch.rs`, `pool/mod.rs` (thread pool)

Exercised by loom (`tests/loom_pool.rs`: `install_wakes_a_sleeping_worker`
and `spawn_then_drop_runs_the_job_once` at preemption bound 2,
`join_with_a_possible_steal` at bound 1). Loom checks the
`StackJob.func`/`result` `UnsafeCell` accesses for races, but not lifetimes or
frame deallocation. Miri runs `tests/pool.rs`
(`fib_matches_sequential_at_several_thread_counts` at n = 8,
`join_outside_a_pool_runs_sequentially`, `drop_runs_every_spawned_job`,
`jobs_spawned_by_jobs_also_run_before_drop_returns`,
`deep_recursion_of_joins` (shallower under Miri),
`panic_in_either_side_of_join_propagates_after_both_finish`,
`concurrent_install_from_outside_threads`,
`install_from_inside_runs_inline`), with one seed, under stacked borrows, tree
borrows and the condvar parker. Miri found the `Latch::set(&self)` protector
violation (POOL.md, "Latches take raw pointers"). TSan and fuzzing do not run
the pool. There are no inline unit tests in `src/pool/`.

### `job.rs`

| site | operation | relies on |
|---|---|---|
| `:16` field `execute: unsafe fn(NonNull<JobHeader>)` | The type-erased entry point; `#[repr(C)]` puts the header first | The field itself has no contract doc; each `execute` it points to has a `# Safety` section |
| `:29` `unsafe impl Send for JobRef` | Every job type requires it: `HeapJob` needs `F: Send + 'static`; `StackJob` needs `F: Send`, `R: Send` and `L: Latch`, which is `Sync`. Each `JobRef` is handed to exactly one thread (deque pop or steal, or injector pop) | `JobRef` is `Copy`, so "runs once" is a scheduler property, not a type property |
| `:32` `unsafe impl Element for JobRef` | The pointer round-trips unchanged | `JobRef` is `Copy`, so "transfers ownership" is notional |
| `:36` `unsafe fn from_raw` | the trait contract | |
| `:44` `unsafe fn JobRef::execute` / `:46` block | Contract: the job is alive and has not run. Calls the job's own `execute` | |
| `:71` `unsafe fn HeapJob::execute` | Contract (`# Safety`): `this` came from `new_ref` for this `F`, and the job has not run | |
| `:73` block | `Box::from_raw(this.cast::<Self>())` | Per the contract. Nothing detects a second run; it would be a double free |
| `:125` `unsafe fn as_job_ref` | Contract: the job stays alive and in place until its latch is set or it is taken back with `run_inline` | This is where the closure's lifetime is erased (`F` need not be `'static`) |
| `:132` `unsafe fn StackJob::execute` | Contract (`# Safety`): `this` points to a live `StackJob` of this type, reached through a `JobRef` that exactly one thread is executing. Works through `*const Self`, never `&Self`, because the waiter may free the frame once the latch is set | |
| `:139` block | `(*job).func.with_mut(take)` | The job is live (the waiter is blocked on the latch) and only one thread executes it. `expect("job ran twice")` catches a sequential double run, not a concurrent one |
| `:146` block | Write `result` | Still live. The waiter reads `result` only after an Acquire that observes the latch's Release |
| `:148` block | `L::set(&raw const (*job).latch)` | Still live. This is the last access to the job |
| `:156` `unsafe fn run_inline` / `:160` block | Contract: the `JobRef` was popped back by the pushing thread, so no other thread can run it | |
| `:171` `unsafe fn into_result(self)` / `:176` block | Contract (`# Safety`): the caller observed the latch set with Acquire (`SpinLatch::probe`, `LockLatch::wait`). `mem::replace` on `result` | Both call sites (`mod.rs:202`, `:347`) state which observation they rely on |

### `latch.rs`

| site | operation | relies on |
|---|---|---|
| `:11` `trait Latch: Sync` | Not `unsafe`, but `JobRef: Send` relies on it: `set` runs on the executing thread while the waiter reads the latch | |
| `:24` `unsafe fn set(this: *const Self)` (trait) | Contract: `this` is live, and after the flag is published `set` must not touch `*this` | |
| `:47` `SpinLatch::set` / `:50` block | `(*this).state.store(1, Release)` | Live until this store, which is the last access |
| `:81` `LockLatch::set` / `:84` block | `Arc::clone(&(*this).shared)` | The flag is not set yet, so the waiter cannot have freed `*this`. After the clone only the cloned `Arc` is used, so unlocking and notifying never go through the latch |

### `mod.rs`

| site | operation | relies on |
|---|---|---|
| `:86` block in `current_worker() -> Option<&'static WorkerThread>` | `&*ptr` from the `CURRENT` thread-local | `CURRENT` is non-null only while `worker_main` runs, and `ClearCurrent` (`:91–97`, armed at `:237`) resets it on return or unwind. That frame owns the `WorkerThread`. Callers run inside a job on that thread and do not keep the reference (doc comment `:77–79`). `WorkerThread` is `!Sync` (it holds `Cell` and `RawWorker`), so the reference cannot cross threads |
| `:191` `job_b.as_job_ref()` in `join` | `join` does not return or unwind until `job_b` has run: `AbortOnUnwind` (`:104–111`) is armed at `:187`, before the push, and forgotten at `:224`, after the result is in hand | |
| `:202` `job_b.into_result()` | `probe` observed the latch set with Acquire | |
| `:208` `job_b.run_inline()` | Popped back by this thread, so no thief has it | `JobRef` equality compares pointers. `job_b` is live, so no other job can share the address |
| `:212`, `:218` `job.execute()` while waiting | Jobs in our deque, or found by `find_work`, are alive and have not run | |
| `:248` `job.execute()` in `worker_main` | Deques and the injector hand each job to exactly one thread while it is alive | |
| `:342` `job.as_job_ref()` in `install` | The caller blocks on the `LockLatch`, and `AbortOnUnwind` (armed `:339`, forgotten `:344`) stops it unwinding before the latch is set | |
| `:347` `job.into_result()` in `install` | `wait` returned, so it saw the flag under the latch's mutex, which orders the result write before this read | |

## `sync/futex/linux.rs` and `sync/futex/macos.rs` (FFI)

Compiled only when `cfg_futex` holds (`sync/mod.rs:6–19`): Linux, Android, or
macOS outside Miri, and not under `--cfg parkring_force_condvar`. Never under
loom (`futex/mod.rs:29–35` require `not(loom)`). Under loom the futex is a
Mutex + Condvar model, which adds a waker-to-woken happens-before edge that a
real futex does not provide (`futex/mod.rs:39–43`). Exercised on Linux by Miri
(`--lib`, which includes the five unit tests in `futex/mod.rs`, plus every
queue, channel and pool test that parks), where Miri emulates the syscall
rather than calling the kernel. Natively on x86_64, i686 and AArch64 Linux,
and by the futex-parker TSan job. **`macos.rs` is exercised only natively, on
the `macos-latest` CI leg.**

| site | operation | relies on |
|---|---|---|
| `linux.rs:23` | `syscall(SYS_futex, word, FUTEX_WAIT \| FUTEX_PRIVATE_FLAG, expected, ts_ptr)` | `word` is a live, 4-byte-aligned atomic borrowed for the call, and `FUTEX_WAIT` reads it atomically only. `ts_ptr` is null or points to a `timespec` in this frame. Errors other than EAGAIN, EINTR and ETIMEDOUT `debug_assert` |
| `linux.rs:45` | `syscall(SYS_futex, word, FUTEX_WAKE \| FUTEX_PRIVATE_FLAG, count)` | "as above; FUTEX_WAKE does not dereference a private futex word". The return value is ignored |
| `macos.rs:11` `unsafe extern "C"` | Hand-written declarations of `__ulock_wait(u32, *mut c_void, u64, u32) -> c_int` and `__ulock_wake(u32, *mut c_void, u64) -> c_int`, plus the constants `UL_COMPARE_AND_WAIT = 1`, `ULF_WAKE_ALL = 0x100`, `ULF_NO_ERRNO = 0x0100_0000` | The comments cite xnu's `bsd/sys/ulock.h`. Nothing checks them against a header |
| `macos.rs:28` | `__ulock_wait(UL_COMPARE_AND_WAIT \| ULF_NO_ERRNO, word, expected, timeout_us)` | `word` is a live, aligned atomic. The kernel reads 4 bytes atomically and writes nothing. EINTR, ETIMEDOUT and EFAULT are treated as spurious (`debug_assert`) |
| `macos.rs:48` | `__ulock_wake(op, word, 0)`, retried on EINTR | "wake does not dereference the address". ENOENT (no waiters) is accepted |

---

## Resolved before 1.0

These were open in the draft of this file. PR #6 addressed the pool and deque
items; the `Worker` item was already covered. What is left of each is stated
here, and carried into the list below where it still matters.

- **`join` had no abort-on-unwind guard while `job_b` might be held by a
  thief.** `join` now arms an `AbortOnUnwind` guard (`pool/mod.rs:104–111`)
  before pushing `job_b` (`:187`) and forgets it only once `job_b` is known
  to be finished (`:224`). `install` does the same around the injected job
  (`:339`, `:344`). An internal-bug panic in that window (`"stole an empty
  slot"`, `"job ran twice"`) now aborts the process instead of freeing a
  frame another thread is using. No test exercises the abort path.
- **`current_worker()` and `CURRENT` on unwind.** `worker_main` now arms a
  `ClearCurrent` guard (`:91–97`, `:237`), so `CURRENT` is reset whether the
  worker returns or unwinds. `current_worker()` still returns
  `&'static WorkerThread`; the doc comment (`:77–79`) and SAFETY comment
  narrow it to "valid while inside `worker_main`, never stored", but that is
  a convention every internal caller follows, not something the types check.
- **`JobRef: Send` and `Latch` lacking `Sync`.** `Latch` now has a `Sync`
  supertrait (`latch.rs:11`), and the SAFETY comment on `JobRef: Send`
  (`job.rs:23–28`) covers `F`, `R` and the latch, and says why a `Copy`
  `JobRef` still runs once. That last part is a scheduler property:
  `HeapJob::execute` still has no check against a second run, which would be
  a double `Box::from_raw`.
- **Safe `into_result` and missing `# Safety` sections.** `into_result` is now
  an `unsafe fn` (`job.rs:171`) whose `# Safety` section requires the latch
  to have been observed set with Acquire, and both call sites say how they
  observed it. `HeapJob::execute` and `StackJob::execute` now have `# Safety`
  sections. The `execute` fn-pointer field (`job.rs:16`) still has no doc of
  its own. `clippy::missing_safety_doc` checks only publicly visible items by
  default (`clippy.toml` does not change that), so these crate-private docs
  are held in place by review, not by the lint.
- **`NonNull::new_unchecked` in the deque** (`deque/mod.rs:188`, `:198`,
  `:277`). The SAFETY comments now state why the slot is non-null: every
  published slot holds a pointer from `E::into_raw`, which returns a
  `NonNull`. `steal` (`:232`) keeps the checked form.
- **Deque `Worker` `!Sync` untested.** It is tested: a `compile_fail`
  doctest in `lib.rs` (`:92–95`) asserts `parkring::Worker<u32>` is not
  `Sync`. Removing `_not_sync` (`deque/mod.rs:340`) would make
  `Inner<E>: Sync` unsound, and that doctest would then fail.

- **The many-seeds Miri job ran no channel tests.** A test-name filter
  applies to every `--test` binary given, so `lockfree` filtered out all of
  `tests/channel.rs`. The job now runs the channel tests and the filtered
  queue tests in two invocations.

## Weak spots / questions for an external reviewer

Ordered by how much I would want a second pair of eyes on each.

1. **Weakest model checking sits on the most delicate code.** The stack-job
   and latch handoff is checked by loom only in `join_with_a_possible_steal`
   at preemption bound 1, and loom cannot see frame deallocation anyway.
   Miri runs `tests/pool.rs` with one seed (under three configurations).
   POOL.md's "12 seeds" is a local run that CI does not repeat. The new
   many-seeds job covers the Vyukov queue only, not the pool or the deque.
   Neither TSan nor fuzzing runs the pool. Consider `-Zmiri-many-seeds` on
   `tests/pool.rs` and `tests/deque.rs`.
2. **Stack jobs, `JobRef` and run-once are protocol properties.**
   `as_job_ref` (`job.rs:125`) returns a `JobRef` with no lifetime, so the
   borrow of the `StackJob` ends at once and the only guard is its `unsafe
   fn` contract plus the abort guards in `join` and `install`. `JobRef` is
   `Copy`; each one runs once only because deques and the injector hand it to
   one thread. `StackJob` has a sequential `expect("job ran twice")`;
   `HeapJob::execute` (`job.rs:73`) has no check, so a second run is a double
   free. `current_worker()` still hands out `&'static WorkerThread`
   (`pool/mod.rs:80`); a scoped accessor (`with_current(|w| ..)`) would let
   the compiler check what the doc comment now promises.
3. **macOS `__ulock` FFI** (`macos.rs:11–56`). These are private symbols with
   hand-written signatures and flag constants, and no tool checks them: loom
   compiles the FFI out, Miri has no shim and switches to the condvar
   fallback, and TSan runs on Linux only. The only evidence is native tests
   on one macOS runner version. `EFAULT` is folded into "spurious" (`:36–41`),
   so a bad address would turn into a busy re-wait in release builds rather
   than an error. App Store builds must opt out with
   `--cfg parkring_force_condvar` (DESIGN.md §4).
4. **Linux futex on untested targets** (`linux.rs`). CI now runs the tests on
   i686 Linux (32-bit `time_t`) and AArch64 Linux, so the `libc::timespec`
   layout and the variadic `syscall` arguments (`u32`, `c_int`, the same
   pattern as std) are exercised on those. Still untested: 32-bit targets
   built with a 64-bit `time_t` ABI, targets that lack a 32-bit-time
   `SYS_futex` (riscv32 may be one; verify), and Android, which takes the
   same path. SCQ is compiled out on 32-bit targets, so the i686 leg does not
   cover it.
5. **Retired deque buffers are reclaimed only in `Drop`**
   (`deque/mod.rs:90–100`, `:249–252`, `:282–290`). This is sound because a
   thief's buffer reference is bounded by `&self` and `Drop` needs `&mut`.
   The cost is memory: a long-lived pool worker keeps every buffer it ever
   outgrew (less than 2× the peak, DEQUE.md §6) until the pool is dropped.
   The stale-buffer read in `steal` relies on `grow` copying rather than
   moving `[t, b)` and never writing to a retired buffer. Loom checks this
   with `Token` elements only (`growth_concurrent_with_steal`); the fuzz
   target exercises growth with real `Box`es, but from one thread.
6. **SCQ soundness rests on safe-code protocol properties.** No `unsafe` in
   `ring.rs`, but `DataCell` reads are sound only if each index is delivered
   exactly once. The D5 "always CAS" change is "argued, not machine-checked"
   (SCQ.md §4: the failing schedule needs four threads, and a build without it
   passed every loom model). The only runtime guard against a double consume
   is a `debug_assert` (`ring.rs:185`). `Drop` (`scq/mod.rs:292–306`) drops
   every non-⊥ `aq` entry with no duplicate check. It also uses `Relaxed`
   loads (`ring.rs:296–304`) without stating the synchronising-last-reference
   assumption that `lockfree.rs:410–413` states. SCQ runs under TSan but not
   under the many-seeds Miri job.
7. **Deque `Drop` with `E = JobRef` leaks instead of dropping.** `drop(JobRef)`
   is a no-op, so a `HeapJob` left in a deque at drop time would leak its
   box. That is not UB, and `ThreadPool::drop` waits for every worker to
   drain its deque first, but this relies on the pool's shutdown protocol
   (`pool/mod.rs:372–387`).
8. **Tool-model caveats.** Loom treats SeqCst accesses as AcqRel and models
   SeqCst fences more strongly than C++20 (DEQUE.md §7), so the deque's fence
   lemma is argued on paper. Loom's futex model adds a happens-before edge
   that a real futex lacks (`futex/mod.rs:39–43`). The README says Miri
   covers "the real `futex` system call on Linux". Miri emulates the syscall;
   the kernel is never called (the native and TSan jobs do call it). TSan
   does not model standalone `fence(SeqCst)`, which is why its job leaves out
   the deque and the pool; it is also `continue-on-error`, so a TSan report
   does not fail CI yet.
