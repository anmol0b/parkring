//! A small work-stealing thread pool built from this crate's pieces.
//!
//! * each worker owns a Chase-Lev [`Worker`](crate::Worker) deque;
//! * jobs from outside the pool enter a [`LockFreeQueue`] injector;
//! * idle workers spin, then park on the crate's futex-based wait queue.
//!
//! `join` pushes its second closure onto the local deque, runs the first, and
//! then runs the second itself unless a thief took it, helping with other work
//! while it waits. Its job lives on the caller's stack, so a join never
//! allocates. See `docs/POOL.md`.

mod job;
mod latch;

use std::cell::Cell;
use std::fmt;
use std::panic;
use std::ptr;
use std::{mem, process};

use self::job::{HeapJob, JobRef, StackJob};
use self::latch::{LockLatch, SpinLatch};
use crate::deque::{RawStealer, RawWorker, Steal};
use crate::queue::LockFreeQueue;
use crate::sync::{
    Arc, AtomicUsize, Backoff,
    Ordering::{AcqRel, Acquire},
    WaitQueue, thread, thread_local,
};
use crate::utils::CachePadded;

/// Set in `events` when the pool is shutting down.
const TERMINATE: usize = 1 << (usize::BITS - 1);
const INJECTOR_CAPACITY: usize = 1024;

/// State shared by every worker of one pool.
struct Registry {
    injector: LockFreeQueue<JobRef>,
    stealers: Box<[RawStealer<JobRef>]>,
    sleep: WaitQueue,
    /// Bumped whenever work is made available to sleepers; `TERMINATE` is
    /// the top bit. Sleeping workers re-check it with an RMW, pairing with
    /// the RMW that bumps it (the same argument as the queues' parking).
    events: CachePadded<AtomicUsize>,
}

impl Registry {
    /// Makes an injected job visible to sleeping workers.
    fn inject(&self, job: JobRef) {
        // The injector is bounded: a full injector blocks the caller, which
        // is backpressure on outside producers.
        if self.injector.push(job).is_err() {
            unreachable!("the injector is closed only by Drop, which owns the pool");
        }
        self.events.fetch_add(1, AcqRel);
        self.sleep.notify_one();
    }
}

/// The per-thread state of a worker. Lives on the worker's stack; `CURRENT`
/// points at it while the worker runs.
struct WorkerThread {
    index: usize,
    deque: RawWorker<JobRef>,
    rng: Cell<u64>,
    registry: Arc<Registry>,
}

thread_local! {
    // Not `const { .. }`: loom's `thread_local!` does not accept that form.
    #[allow(clippy::missing_const_for_thread_local)]
    static CURRENT: Cell<*const WorkerThread> = Cell::new(ptr::null());
}

/// The worker running on this thread, if any.
///
/// The `'static` is narrower than it looks: the reference is valid only
/// while this thread is inside `worker_main`. No caller stores it; each uses
/// it within the call that fetched it, which runs inside a job on this thread.
fn current_worker() -> Option<&'static WorkerThread> {
    let ptr = CURRENT.with(Cell::get);
    // SAFETY: `CURRENT` is non-null only while `worker_main` runs (it is reset
    // when `worker_main` returns or unwinds; see `ClearCurrent`), and that
    // frame owns the `WorkerThread`. Callers run on this thread, inside that
    // frame, and do not keep the reference past their own call.
    (!ptr.is_null()).then(|| unsafe { &*ptr })
}

/// Clears `CURRENT` when `worker_main` ends, including by unwinding, so no
/// later code on this thread can reach the dead `WorkerThread`.
struct ClearCurrent;

impl Drop for ClearCurrent {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(ptr::null()));
    }
}

/// Aborts the process if it is dropped, that is, if the frame that owns it
/// unwinds. Armed while another thread may hold a reference into this frame
/// (a `StackJob` it is running): unwinding would free memory still in use,
/// so aborting is the only sound option. Disarmed with `mem::forget` once
/// the job is known to be finished. Rayon does the same.
struct AbortOnUnwind;

impl Drop for AbortOnUnwind {
    fn drop(&mut self) {
        eprintln!("parkring: unwinding while another thread uses this stack frame; aborting");
        process::abort();
    }
}

enum Found {
    Job(JobRef),
    /// Nothing found, but a steal lost a race: work may exist.
    Retry,
    Nothing,
}

impl WorkerThread {
    /// xorshift64: a victim-selection RNG that needs no shared state.
    fn next_random(&self) -> u64 {
        let mut x = self.rng.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng.set(x);
        x
    }

    /// Pushes onto the local deque and, if some worker may be asleep, wakes
    /// one to steal it. The wake is best-effort (a `Relaxed` check): if it is
    /// missed, this worker still runs the job itself, so nothing is stranded;
    /// only parallelism is lost.
    fn push(&self, job: JobRef) {
        self.deque.push(job);
        let registry = &self.registry;
        if registry.sleep.has_waiters() {
            registry.events.fetch_add(1, AcqRel);
            registry.sleep.notify_one();
        }
    }

    /// Own deque first (newest, cache-hot), then steal (oldest) from the
    /// others starting at a random victim, then the injector.
    fn find_work(&self) -> Found {
        if let Some(job) = self.deque.pop() {
            return Found::Job(job);
        }
        let stealers = &self.registry.stealers;
        let n = stealers.len();
        let mut retry = false;
        if n > 1 {
            // Lemire's multiply-shift maps the random word onto 0..n; the
            // result is below `n`, so the cast cannot truncate.
            #[allow(clippy::cast_possible_truncation)]
            let start = ((u128::from(self.next_random()) * n as u128) >> 64) as usize;
            for k in 0..n {
                let victim = (start + k) % n;
                if victim == self.index {
                    continue;
                }
                match stealers[victim].steal() {
                    Steal::Success(job) => return Found::Job(job),
                    Steal::Retry => retry = true,
                    Steal::Empty => {}
                }
            }
        }
        if let Ok(job) = self.registry.injector.try_pop() {
            return Found::Job(job);
        }
        if retry { Found::Retry } else { Found::Nothing }
    }

    fn join<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        let job_b = StackJob::new(b, SpinLatch::new());
        // From the push until `job_b` is known to be finished, another thread
        // may be running it out of this frame: this frame must not unwind.
        // Panics in `a` and `b` are caught; this guards everything else.
        let guard = AbortOnUnwind;
        // SAFETY: we do not return or unwind until `job_b` has run (below), so
        // it stays alive and in place for as long as anyone can hold this
        // reference.
        let job_b_ref = unsafe { job_b.as_job_ref() };
        self.push(job_b_ref);

        // `a` runs here. Its panic is held until `b` is finished: `b` may be
        // running on another thread and borrowing this frame.
        let result_a = panic::catch_unwind(panic::AssertUnwindSafe(a));

        let mut backoff = Backoff::new();
        let result_b = loop {
            if job_b.latch.probe() {
                break job_b.into_result();
            }
            match self.deque.pop() {
                Some(job) if job == job_b_ref => {
                    // Nobody stole it: run it inline, without the job machinery.
                    // SAFETY: we popped it back ourselves, so no thief has it.
                    break unsafe { job_b.run_inline() };
                }
                Some(job) => {
                    // SAFETY: jobs in our deque are alive and not yet run.
                    unsafe { job.execute() };
                    backoff.reset();
                }
                None => match self.find_work() {
                    // `b` was stolen: help with other work while it runs.
                    // SAFETY: as above.
                    Found::Job(job) => unsafe { job.execute() },
                    Found::Retry | Found::Nothing => backoff.snooze(),
                },
            }
        };
        // `job_b` has finished: nobody else refers to this frame any more.
        mem::forget(guard);
        match (result_a, result_b) {
            (Ok(ra), Ok(rb)) => (ra, rb),
            (Err(payload), _) | (_, Err(payload)) => panic::resume_unwind(payload),
        }
    }
}

// By value on purpose: the `WorkerThread` must live in this thread's own
// frame, which `CURRENT` points into for the thread's whole life.
#[allow(clippy::needless_pass_by_value)]
fn worker_main(worker: WorkerThread) {
    CURRENT.with(|c| c.set(&raw const worker));
    let _clear = ClearCurrent;
    let registry = Arc::clone(&worker.registry);
    let mut backoff = Backoff::new();
    loop {
        // Snapshot before scanning: if nothing is found and nothing changed
        // since, it is safe to sleep; a change wakes us (see `Registry`).
        let seen = registry.events.load(Acquire);
        match worker.find_work() {
            Found::Job(job) => {
                // SAFETY: jobs come from deques or the injector, which hand
                // each job to exactly one thread, while it is alive.
                unsafe { job.execute() };
                backoff.reset();
            }
            Found::Retry => backoff.spin(),
            Found::Nothing if seen & TERMINATE != 0 => break,
            Found::Nothing if backoff.is_completed() => {
                registry
                    .sleep
                    .wait_until(|| registry.events.fetch_add(0, AcqRel) != seen, None);
                backoff.reset();
            }
            Found::Nothing => backoff.snooze(),
        }
    }
}

/// A work-stealing thread pool.
///
/// ```
/// use parkring::{ThreadPool, join};
///
/// fn fib(n: u64) -> u64 {
///     if n < 2 {
///         return n;
///     }
///     let (a, b) = join(|| fib(n - 1), || fib(n - 2));
///     a + b
/// }
///
/// let pool = ThreadPool::new(4);
/// assert_eq!(pool.install(|| fib(20)), 6765);
/// ```
pub struct ThreadPool {
    registry: Arc<Registry>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl ThreadPool {
    /// Starts a pool with `threads` workers.
    ///
    /// # Panics
    /// If `threads` is zero.
    pub fn new(threads: usize) -> Self {
        assert!(threads > 0, "a pool needs at least one thread");
        let deques: Vec<RawWorker<JobRef>> = (0..threads).map(|_| RawWorker::new()).collect();
        let registry = Arc::new(Registry {
            injector: LockFreeQueue::new(INJECTOR_CAPACITY),
            stealers: deques.iter().map(RawWorker::stealer).collect(),
            sleep: WaitQueue::new(),
            events: CachePadded::new(AtomicUsize::new(0)),
        });
        let threads = deques
            .into_iter()
            .enumerate()
            .map(|(index, deque)| {
                let worker = WorkerThread {
                    index,
                    deque,
                    // Any odd, index-dependent seed works for xorshift.
                    rng: Cell::new((index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
                    registry: Arc::clone(&registry),
                };
                thread::spawn(move || worker_main(worker))
            })
            .collect();
        Self { registry, threads }
    }

    /// The number of worker threads.
    #[must_use]
    pub fn threads(&self) -> usize {
        self.threads.len()
    }

    /// Runs `f` on a worker and returns its result. Inside `f`, [`join`]
    /// runs in parallel on this pool.
    ///
    /// Called from one of this pool's own workers, `f` runs inline.
    pub fn install<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        if let Some(worker) = current_worker() {
            if ptr::eq(Arc::as_ptr(&worker.registry), Arc::as_ptr(&self.registry)) {
                return f();
            }
        }
        let job = StackJob::new(f, LockLatch::new());
        // A worker may run the job out of this frame until the latch is set,
        // so this frame must not unwind in between. See `AbortOnUnwind`.
        let guard = AbortOnUnwind;
        // SAFETY: we block on the latch below, and cannot unwind before it is
        // set, so the job outlives any use.
        self.registry.inject(unsafe { job.as_job_ref() });
        job.latch.wait();
        mem::forget(guard);
        match job.into_result() {
            Ok(r) => r,
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    /// Runs `f` on the pool without waiting for it. Dropping the pool waits
    /// for every spawned job to finish.
    ///
    /// A panic in a spawned job aborts the process: there is nobody to
    /// report it to.
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = HeapJob::new_ref(f);
        match current_worker() {
            Some(worker) if ptr::eq(Arc::as_ptr(&worker.registry), Arc::as_ptr(&self.registry)) => {
                worker.push(job);
            }
            _ => self.registry.inject(job),
        }
    }
}

impl Drop for ThreadPool {
    /// Runs every queued and spawned job to completion, then stops the workers.
    ///
    /// A worker exits only after a scan that found no work anywhere and no
    /// lost races. Jobs spawned by running jobs go to that worker's own deque,
    /// which it drains before it can make such a scan.
    fn drop(&mut self) {
        self.registry.events.fetch_or(TERMINATE, AcqRel);
        self.registry.sleep.notify_all();
        for handle in self.threads.drain(..) {
            // A worker never unwinds (spawned panics abort, joined panics are
            // caught), so a join error would be a bug in the pool.
            handle.join().expect("worker thread panicked");
        }
    }
}

impl fmt::Debug for ThreadPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThreadPool")
            .field("threads", &self.threads())
            .finish_non_exhaustive()
    }
}

/// Runs `a` and `b`, potentially in parallel, and returns both results.
///
/// On a pool worker, `b` is offered to other workers while the current thread
/// runs `a`. Outside a pool, both run sequentially on the current thread. If
/// either closure panics, the panic propagates after both have finished.
pub fn join<A, B, RA, RB>(a: A, b: B) -> (RA, RB)
where
    A: FnOnce() -> RA + Send,
    B: FnOnce() -> RB + Send,
    RA: Send,
    RB: Send,
{
    match current_worker() {
        Some(worker) => worker.join(a, b),
        None => (a(), b()),
    }
}
