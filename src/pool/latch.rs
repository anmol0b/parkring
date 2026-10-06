//! One-shot signals that a job has finished.

use std::sync::PoisonError;

use crate::sync::{Arc, AtomicUsize, Condvar, Mutex, Ordering};

/// Signals completion of a job.
///
/// `Sync` because `set` runs on the thread that executed the job while the
/// waiter reads the latch on its own thread.
pub(super) trait Latch: Sync {
    /// Marks the latch as set.
    ///
    /// Takes a raw pointer, not `&Self`: the waiter may free the latch (it
    /// lives in the waiter's stack frame) the moment it sees the flag, while
    /// `set` is still running. A `&Self` argument stays protected until the
    /// function returns, so freeing it earlier is undefined behaviour under
    /// Rust's aliasing model; Miri caught exactly that. Rayon's latches take
    /// `*const Self` for the same reason.
    ///
    /// # Safety
    /// `this` must point to a live latch. After the flag is published, `set`
    /// must not touch `*this` again.
    unsafe fn set(this: *const Self);
}

/// A flag the waiting worker polls while it helps with other work. Used by
/// `join`, where the waiter is a worker thread that keeps executing jobs.
pub(super) struct SpinLatch {
    state: AtomicUsize,
}

impl SpinLatch {
    pub(super) fn new() -> Self {
        Self {
            state: AtomicUsize::new(0),
        }
    }

    /// Acquire pairs with `set`'s Release: the job's result is visible.
    pub(super) fn probe(&self) -> bool {
        self.state.load(Ordering::Acquire) == 1
    }
}

impl Latch for SpinLatch {
    unsafe fn set(this: *const Self) {
        // SAFETY: `this` is live until this store publishes the flag, and the
        // store is the last access.
        unsafe { (*this).state.store(1, Ordering::Release) };
    }
}

/// A latch an outside thread can block on. Used by `install`.
///
/// The flag and condvar are behind an `Arc` the setter clones *before*
/// setting: the waiter lives in another thread's frame and may return (and
/// free the latch) the moment it sees the flag, so the setter must not unlock
/// or notify through the latch itself.
pub(super) struct LockLatch {
    shared: Arc<(Mutex<bool>, Condvar)>,
}

impl LockLatch {
    pub(super) fn new() -> Self {
        Self {
            shared: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    pub(super) fn wait(&self) {
        let (lock, cv) = &*self.shared;
        let mut set = lock.lock().unwrap_or_else(PoisonError::into_inner);
        while !*set {
            set = cv.wait(set).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl Latch for LockLatch {
    unsafe fn set(this: *const Self) {
        // SAFETY: the flag is not set yet, so the waiter cannot have freed
        // `*this`. From here on only `shared` is used, never `this`.
        let shared = unsafe { Arc::clone(&(*this).shared) };
        let (lock, cv) = &*shared;
        *lock.lock().unwrap_or_else(PoisonError::into_inner) = true;
        cv.notify_all();
    }
}
