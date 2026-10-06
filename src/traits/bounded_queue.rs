//! The behaviour shared by every queue in this crate.

use std::time::Duration;

use crate::error::{
    PopError, PopTimeoutError, PushError, PushTimeoutError, TryPopError, TryPushError,
};

/// A bounded, closable, multi-producer multi-consumer FIFO queue.
///
/// Construction is left to each implementation because capacity semantics
/// differ: [`LockFreeQueue`](crate::LockFreeQueue) rounds up to a power of two (minimum 2),
/// [`BlockingQueue`](crate::BlockingQueue) uses the exact value.
///
/// # Closing
///
/// After [`close`](Self::close), every push fails and hands the item back.
/// Pops keep returning the remaining items and fail only once the queue is
/// both closed and empty. All blocked threads are woken by `close`.
///
/// # Example
///
/// Code written against the trait runs on any of the queues:
///
/// ```
/// use parkring::{BlockingQueue, BoundedQueue, LockFreeQueue};
///
/// fn produce_then_drain<Q: BoundedQueue<u64>>(queue: &Q) -> u64 {
///     std::thread::scope(|s| {
///         let consumer = s.spawn(|| {
///             let mut sum = 0;
///             while let Ok(v) = queue.pop() {
///                 sum += v;
///             }
///             sum
///         });
///         for i in 1..=100 {
///             queue.push(i).unwrap();
///         }
///         queue.close();
///         consumer.join().unwrap()
///     })
/// }
///
/// assert_eq!(produce_then_drain(&LockFreeQueue::new(8)), 5050);
/// assert_eq!(produce_then_drain(&BlockingQueue::new(8)), 5050);
/// ```
pub trait BoundedQueue<T: Send>: Send + Sync {
    /// Pushes `item`, blocking while the queue is full.
    ///
    /// # Errors
    /// Returns the item if the queue is closed.
    fn push(&self, item: T) -> Result<(), PushError<T>>;

    /// Pops the oldest item, blocking while the queue is empty.
    ///
    /// # Errors
    /// Fails once the queue is closed and drained.
    fn pop(&self) -> Result<T, PopError>;

    /// Pushes `item` if there is room right now.
    ///
    /// # Errors
    /// Returns the item if the queue is full or closed.
    fn try_push(&self, item: T) -> Result<(), TryPushError<T>>;

    /// Pops the oldest item if one is available right now.
    ///
    /// # Errors
    /// Fails if the queue is empty, distinguishing closed-and-drained.
    fn try_pop(&self) -> Result<T, TryPopError>;

    /// Like [`push`](Self::push), but gives up after `timeout`.
    ///
    /// # Errors
    /// Returns the item on timeout or if the queue is closed.
    fn push_timeout(&self, item: T, timeout: Duration) -> Result<(), PushTimeoutError<T>>;

    /// Like [`pop`](Self::pop), but gives up after `timeout`.
    ///
    /// # Errors
    /// Fails on timeout, or once the queue is closed and drained.
    fn pop_timeout(&self, timeout: Duration) -> Result<T, PopTimeoutError>;

    /// Closes the queue and wakes every blocked thread. Returns `true` if this
    /// call closed it, `false` if it was already closed.
    fn close(&self) -> bool;

    /// Returns `true` once [`close`](Self::close) has been called.
    fn is_closed(&self) -> bool;

    /// The maximum number of items the queue holds.
    fn capacity(&self) -> usize;

    /// A snapshot of the number of items in the queue. It may be stale by the
    /// time the caller looks at it.
    fn len(&self) -> usize;

    /// Snapshot: `true` if the queue held no items.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot: `true` if the queue was at capacity.
    fn is_full(&self) -> bool {
        self.len() >= self.capacity()
    }
}

/// Implements [`BoundedQueue`] by forwarding to inherent methods of the same name.
macro_rules! forward_bounded_queue {
    ($ty:ident) => {
        impl<T: Send> $crate::BoundedQueue<T> for $ty<T> {
            fn push(&self, item: T) -> Result<(), $crate::PushError<T>> {
                $ty::push(self, item)
            }
            fn pop(&self) -> Result<T, $crate::PopError> {
                $ty::pop(self)
            }
            fn try_push(&self, item: T) -> Result<(), $crate::TryPushError<T>> {
                $ty::try_push(self, item)
            }
            fn try_pop(&self) -> Result<T, $crate::TryPopError> {
                $ty::try_pop(self)
            }
            fn push_timeout(
                &self,
                item: T,
                timeout: ::std::time::Duration,
            ) -> Result<(), $crate::PushTimeoutError<T>> {
                $ty::push_timeout(self, item, timeout)
            }
            fn pop_timeout(
                &self,
                timeout: ::std::time::Duration,
            ) -> Result<T, $crate::PopTimeoutError> {
                $ty::pop_timeout(self, timeout)
            }
            fn close(&self) -> bool {
                $ty::close(self)
            }
            fn is_closed(&self) -> bool {
                $ty::is_closed(self)
            }
            fn capacity(&self) -> usize {
                $ty::capacity(self)
            }
            fn len(&self) -> usize {
                $ty::len(self)
            }
            fn is_empty(&self) -> bool {
                $ty::is_empty(self)
            }
            fn is_full(&self) -> bool {
                $ty::is_full(self)
            }
        }
    };
}
pub(crate) use forward_bounded_queue;
