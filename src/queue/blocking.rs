//! Mutex + Condvar bounded MPMC queue.

use std::fmt;
use std::sync::PoisonError;
use std::time::{Duration, Instant};

use crate::error::{
    PopError, PopTimeoutError, PushError, PushTimeoutError, TryPopError, TryPushError,
};
use crate::queue::ring_buffer::RingBuffer;
use crate::sync::{Condvar, Mutex, MutexGuard};
use crate::traits::forward_bounded_queue;

struct Inner<T> {
    ring: RingBuffer<T>,
    closed: bool,
}

/// A bounded MPMC queue guarded by one mutex, with separate `not_full` and
/// `not_empty` condition variables.
///
/// This is the reference implementation: simple enough to be obviously
/// correct, and the baseline the lock-free queue is measured against. Every
/// operation serialises on the mutex, so it stops scaling once more than a
/// few threads contend. Capacity is exact (not rounded).
///
/// # Example
///
/// ```
/// use parkring::{BlockingQueue, TryPushError};
///
/// let q = BlockingQueue::new(2);
/// q.push("a").unwrap();
/// q.push("b").unwrap();
/// assert!(matches!(q.try_push("c"), Err(TryPushError::Full("c"))));
/// q.close();
/// assert_eq!(q.pop(), Ok("a"));
/// assert_eq!(q.pop(), Ok("b"));
/// assert!(q.pop().is_err());
/// ```
pub struct BlockingQueue<T> {
    inner: Mutex<Inner<T>>,
    not_empty: Condvar,
    not_full: Condvar,
    capacity: usize,
}

impl<T> BlockingQueue<T> {
    /// Creates a queue holding exactly `capacity` items.
    ///
    /// # Panics
    /// If `capacity` is zero.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                ring: RingBuffer::new(capacity),
                closed: false,
            }),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            capacity,
        }
    }

    /// The maximum number of items the queue holds.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    fn lock(&self) -> MutexGuard<'_, Inner<T>> {
        // `RingBuffer` never unwinds half-way through an update, so the state
        // behind a poisoned lock is still consistent.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits on `cv`. Returns `None` once `deadline` has passed.
    fn wait<'a>(
        cv: &Condvar,
        guard: MutexGuard<'a, Inner<T>>,
        deadline: Option<Instant>,
    ) -> Option<MutexGuard<'a, Inner<T>>> {
        match deadline {
            None => Some(cv.wait(guard).unwrap_or_else(PoisonError::into_inner)),
            Some(deadline) => {
                let now = Instant::now();
                if now >= deadline {
                    return None;
                }
                let (guard, _) = cv
                    .wait_timeout(guard, deadline - now)
                    .unwrap_or_else(PoisonError::into_inner);
                Some(guard)
            }
        }
    }

    fn push_until(&self, item: T, deadline: Option<Instant>) -> Result<(), PushTimeoutError<T>> {
        let mut guard = self.lock();
        loop {
            if guard.closed {
                return Err(PushTimeoutError::Closed(item));
            }
            if !guard.ring.is_full() {
                guard.ring.push(item);
                drop(guard);
                self.not_empty.notify_one();
                return Ok(());
            }
            match Self::wait(&self.not_full, guard, deadline) {
                Some(g) => guard = g,
                None => return Err(PushTimeoutError::Timeout(item)),
            }
        }
    }

    fn pop_until(&self, deadline: Option<Instant>) -> Result<T, PopTimeoutError> {
        let mut guard = self.lock();
        loop {
            if let Some(item) = guard.ring.pop() {
                drop(guard);
                self.not_full.notify_one();
                return Ok(item);
            }
            if guard.closed {
                return Err(PopTimeoutError::Closed);
            }
            match Self::wait(&self.not_empty, guard, deadline) {
                Some(g) => guard = g,
                None => return Err(PopTimeoutError::Timeout),
            }
        }
    }

    /// Pushes `item`, blocking while the queue is full.
    ///
    /// # Errors
    /// Returns the item if the queue is closed.
    pub fn push(&self, item: T) -> Result<(), PushError<T>> {
        match self.push_until(item, None) {
            Ok(()) => Ok(()),
            Err(PushTimeoutError::Closed(v)) => Err(PushError(v)),
            Err(PushTimeoutError::Timeout(_)) => unreachable!("no deadline was set"),
        }
    }

    /// Pops the oldest item, blocking while the queue is empty.
    ///
    /// # Errors
    /// Fails once the queue is closed and drained.
    pub fn pop(&self) -> Result<T, PopError> {
        match self.pop_until(None) {
            Ok(v) => Ok(v),
            Err(PopTimeoutError::Closed) => Err(PopError),
            Err(PopTimeoutError::Timeout) => unreachable!("no deadline was set"),
        }
    }

    /// Pushes `item` if there is room right now.
    ///
    /// # Errors
    /// Returns the item if the queue is full or closed.
    pub fn try_push(&self, item: T) -> Result<(), TryPushError<T>> {
        let mut guard = self.lock();
        if guard.closed {
            return Err(TryPushError::Closed(item));
        }
        if guard.ring.is_full() {
            return Err(TryPushError::Full(item));
        }
        guard.ring.push(item);
        drop(guard);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Pops the oldest item if one is available right now.
    ///
    /// # Errors
    /// Fails if the queue is empty, distinguishing closed-and-drained.
    pub fn try_pop(&self) -> Result<T, TryPopError> {
        let mut guard = self.lock();
        match guard.ring.pop() {
            Some(item) => {
                drop(guard);
                self.not_full.notify_one();
                Ok(item)
            }
            None if guard.closed => Err(TryPopError::Closed),
            None => Err(TryPopError::Empty),
        }
    }

    /// Like [`push`](Self::push), but gives up after `timeout`.
    ///
    /// # Errors
    /// Returns the item on timeout or if the queue is closed.
    pub fn push_timeout(&self, item: T, timeout: Duration) -> Result<(), PushTimeoutError<T>> {
        self.push_until(item, Instant::now().checked_add(timeout))
    }

    /// Like [`pop`](Self::pop), but gives up after `timeout`.
    ///
    /// # Errors
    /// Fails on timeout, or once the queue is closed and drained.
    pub fn pop_timeout(&self, timeout: Duration) -> Result<T, PopTimeoutError> {
        self.pop_until(Instant::now().checked_add(timeout))
    }

    /// Closes the queue and wakes every blocked thread. Returns `true` if this
    /// call closed it.
    pub fn close(&self) -> bool {
        let mut guard = self.lock();
        let newly_closed = !guard.closed;
        guard.closed = true;
        drop(guard);
        if newly_closed {
            self.not_empty.notify_all();
            self.not_full.notify_all();
        }
        newly_closed
    }

    /// Returns `true` once [`close`](Self::close) has been called.
    pub fn is_closed(&self) -> bool {
        self.lock().closed
    }

    /// A snapshot of the number of items in the queue.
    pub fn len(&self) -> usize {
        self.lock().ring.len()
    }

    /// Snapshot: `true` if the queue held no items.
    pub fn is_empty(&self) -> bool {
        self.lock().ring.is_empty()
    }

    /// Snapshot: `true` if the queue was at capacity.
    pub fn is_full(&self) -> bool {
        self.lock().ring.is_full()
    }
}

impl<T> fmt::Debug for BlockingQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let guard = self.lock();
        f.debug_struct("BlockingQueue")
            .field("capacity", &self.capacity)
            .field("len", &guard.ring.len())
            .field("closed", &guard.closed)
            .finish_non_exhaustive()
    }
}

forward_bounded_queue!(BlockingQueue);
