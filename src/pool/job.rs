//! Type-erased jobs: a pointer to a header whose first field says how to run
//! the rest.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::ptr::NonNull;

use super::latch::Latch;
use crate::deque::Element;
use crate::sync::UnsafeCell;

/// The first field of every job. `#[repr(C)]` on the job types guarantees a
/// pointer to the job is also a pointer to its header.
#[repr(C)]
pub(super) struct JobHeader {
    execute: unsafe fn(NonNull<JobHeader>),
}

/// A pointer to a job, erased to its header. Runs the job at most once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct JobRef(NonNull<JobHeader>);

// SAFETY: sending a `JobRef` lets another thread run the job. That is sound
// because every job type requires it: `HeapJob` needs `F: Send + 'static`;
// `StackJob` needs `F: Send` and `R: Send` (the result travels back to the
// waiter) and a `Latch`, which is `Sync` (setter and waiter share it). A
// `JobRef` is `Copy`, but the scheduler hands each one to exactly one thread
// (a deque pop or steal, or an injector pop), so a job runs once.
unsafe impl Send for JobRef {}

// SAFETY: a `JobRef` round-trips through its pointer unchanged.
unsafe impl Element for JobRef {
    fn into_raw(self) -> NonNull<()> {
        self.0.cast()
    }
    unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        Self(ptr.cast())
    }
}

impl JobRef {
    /// # Safety
    /// The job must still be alive and must not have been executed.
    pub(super) unsafe fn execute(self) {
        // SAFETY: per the caller contract; `execute` is the job's own function.
        unsafe { (self.0.as_ref().execute)(self.0) }
    }
}

/// A heap-allocated fire-and-forget job, used by `spawn`.
#[repr(C)]
pub(super) struct HeapJob<F> {
    header: JobHeader,
    func: F,
}

impl<F: FnOnce() + Send + 'static> HeapJob<F> {
    pub(super) fn new_ref(func: F) -> JobRef {
        let job = Box::new(Self {
            header: JobHeader {
                execute: Self::execute,
            },
            func,
        });
        JobRef(NonNull::from(Box::leak(job)).cast())
    }

    unsafe fn execute(this: NonNull<JobHeader>) {
        // SAFETY: `this` came from `new_ref`, and a job runs once.
        let job = unsafe { Box::from_raw(this.cast::<Self>().as_ptr()) };
        // A spawned job has nobody to report a panic to. Unwinding into the
        // worker loop would kill the worker and lose its queued jobs, so abort,
        // as rayon does by default.
        if panic::catch_unwind(AssertUnwindSafe(job.func)).is_err() {
            eprintln!("parkring: a spawned job panicked; aborting");
            std::process::abort();
        }
    }
}

/// The outcome of a job run on another thread.
pub(super) enum JobResult<R> {
    None,
    Ok(R),
    Panic(Box<dyn Any + Send>),
}

/// A job that lives in the frame of the thread waiting for it (`join`,
/// `install`), so a join never allocates.
///
/// The waiting thread must not return until `latch` is set, and `execute`'s
/// last access to the job is setting the latch: after that, the frame may be
/// gone.
#[repr(C)]
pub(super) struct StackJob<F, R, L> {
    header: JobHeader,
    func: UnsafeCell<Option<F>>,
    result: UnsafeCell<JobResult<R>>,
    pub(super) latch: L,
}

impl<F, R, L> StackJob<F, R, L>
where
    F: FnOnce() -> R + Send,
    R: Send,
    L: Latch,
{
    pub(super) fn new(func: F, latch: L) -> Self {
        Self {
            header: JobHeader {
                execute: Self::execute,
            },
            func: UnsafeCell::new(Some(func)),
            result: UnsafeCell::new(JobResult::None),
            latch,
        }
    }

    /// # Safety
    /// The job must stay alive, and in place, until its latch is set or it
    /// has been taken back with [`run_inline`](Self::run_inline).
    pub(super) unsafe fn as_job_ref(&self) -> JobRef {
        JobRef(NonNull::from(self).cast())
    }

    unsafe fn execute(this: NonNull<JobHeader>) {
        // A raw pointer throughout, never `&Self`: the waiter frees the job
        // (its stack frame) as soon as the latch is set, possibly before this
        // function returns. See `Latch::set`.
        let job: *const Self = this.cast::<Self>().as_ptr();
        // SAFETY: the job is live (the waiter is blocked on its latch), and
        // only one thread executes a job.
        let func = unsafe { (*job).func.with_mut(|f| (*f).take()) }.expect("job ran twice");
        let result = match panic::catch_unwind(AssertUnwindSafe(func)) {
            Ok(r) => JobResult::Ok(r),
            Err(payload) => JobResult::Panic(payload),
        };
        // SAFETY: still live; the waiter reads `result` only after observing
        // the latch, which the Release in `set` orders after this write.
        unsafe { (*job).result.with_mut(|r| *r = result) };
        // SAFETY: still live. This is the last access to the job.
        unsafe { L::set(&raw const (*job).latch) };
    }

    /// Runs the job on the current thread, if nobody else took it.
    ///
    /// # Safety
    /// The job's `JobRef` must have been popped back by the thread that
    /// pushed it, so no other thread can execute it.
    pub(super) unsafe fn run_inline(&self) -> std::thread::Result<R> {
        // SAFETY: per the caller contract, we are the only thread with access.
        let func = self
            .func
            .with_mut(|f| unsafe { (*f).take() })
            .expect("job ran twice");
        panic::catch_unwind(AssertUnwindSafe(func))
    }

    /// Takes the result after the latch has been set.
    pub(super) fn into_result(self) -> std::thread::Result<R> {
        let result = self.result.with_mut(|r| {
            // SAFETY: the latch was observed set (Acquire), so the executing
            // thread's write happens-before this read, and it no longer
            // touches the job.
            unsafe { std::mem::replace(&mut *r, JobResult::None) }
        });
        match result {
            JobResult::Ok(r) => Ok(r),
            JobResult::Panic(payload) => Err(payload),
            JobResult::None => unreachable!("latch set without a result"),
        }
    }
}
