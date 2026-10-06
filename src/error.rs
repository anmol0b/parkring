//! Error types, split by operation in the style of `std::sync::mpsc`.
//!
//! Every error that can happen while the caller still owns the item hands the
//! item back, so a failed push never drops data.

use std::error::Error;
use std::fmt;

/// Returned by `push` when the queue is closed. Carries the rejected item.
///
/// ```
/// use parkring::LockFreeQueue;
///
/// let queue = LockFreeQueue::new(4);
/// queue.close();
/// let err = queue.push(String::from("late")).unwrap_err();
/// assert_eq!(err.into_inner(), "late"); // the item comes back, never dropped
/// ```
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct PushError<T>(pub T);

/// Returned by `pop` when the queue is closed and fully drained.
///
/// Items pushed before `close` are still delivered; `PopError` means there is
/// nothing left and nothing more will arrive.
///
/// ```
/// use parkring::{LockFreeQueue, PopError};
///
/// let queue = LockFreeQueue::new(4);
/// queue.push(1).unwrap();
/// queue.close();
/// assert_eq!(queue.pop(), Ok(1));
/// assert_eq!(queue.pop(), Err(PopError));
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PopError;

/// Returned by `try_push`.
///
/// ```
/// use parkring::{LockFreeQueue, TryPushError};
///
/// let queue = LockFreeQueue::new(2);
/// queue.try_push(1).unwrap();
/// queue.try_push(2).unwrap();
/// assert_eq!(queue.try_push(3), Err(TryPushError::Full(3)));
///
/// queue.close();
/// let err = queue.try_push(4).unwrap_err();
/// assert!(err.is_closed());
/// assert_eq!(err.into_inner(), 4);
/// ```
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TryPushError<T> {
    /// The queue is at capacity.
    Full(T),
    /// The queue is closed.
    Closed(T),
}

/// Returned by `try_pop`.
///
/// ```
/// use parkring::{LockFreeQueue, TryPopError};
///
/// let queue = LockFreeQueue::<u32>::new(4);
/// assert_eq!(queue.try_pop(), Err(TryPopError::Empty));
/// queue.close();
/// assert_eq!(queue.try_pop(), Err(TryPopError::Closed));
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TryPopError {
    /// The queue holds no items.
    Empty,
    /// The queue is closed and fully drained.
    Closed,
}

/// Returned by `push_timeout`.
///
/// ```
/// use std::time::Duration;
/// use parkring::{LockFreeQueue, PushTimeoutError};
///
/// let queue = LockFreeQueue::new(2);
/// queue.push(1).unwrap();
/// queue.push(2).unwrap();
/// let err = queue.push_timeout(3, Duration::from_millis(10)).unwrap_err();
/// assert_eq!(err, PushTimeoutError::Timeout(3));
/// ```
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum PushTimeoutError<T> {
    /// The queue stayed full until the timeout elapsed.
    Timeout(T),
    /// The queue is closed.
    Closed(T),
}

/// Returned by `pop_timeout`.
///
/// ```
/// use std::time::Duration;
/// use parkring::{LockFreeQueue, PopTimeoutError};
///
/// let queue = LockFreeQueue::<u32>::new(4);
/// assert_eq!(queue.pop_timeout(Duration::from_millis(10)), Err(PopTimeoutError::Timeout));
/// queue.close();
/// assert_eq!(queue.pop_timeout(Duration::from_millis(10)), Err(PopTimeoutError::Closed));
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PopTimeoutError {
    /// The queue stayed empty until the timeout elapsed.
    Timeout,
    /// The queue is closed and fully drained.
    Closed,
}

impl<T> PushError<T> {
    /// Returns the item that could not be pushed.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> TryPushError<T> {
    /// Returns the item that could not be pushed.
    pub fn into_inner(self) -> T {
        match self {
            Self::Full(v) | Self::Closed(v) => v,
        }
    }

    /// Returns `true` if the push failed because the queue was full.
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full(_))
    }

    /// Returns `true` if the push failed because the queue was closed.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed(_))
    }
}

impl<T> PushTimeoutError<T> {
    /// Returns the item that could not be pushed.
    pub fn into_inner(self) -> T {
        match self {
            Self::Timeout(v) | Self::Closed(v) => v,
        }
    }

    /// Returns `true` if the push failed because the timeout elapsed.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }

    /// Returns `true` if the push failed because the queue was closed.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed(_))
    }
}

// `Debug` is written by hand so that `T: Debug` is not required, matching
// `std::sync::mpsc::SendError`.
impl<T> fmt::Debug for PushError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PushError(..)")
    }
}

impl<T> fmt::Debug for TryPushError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => f.write_str("Full(..)"),
            Self::Closed(_) => f.write_str("Closed(..)"),
        }
    }
}

impl<T> fmt::Debug for PushTimeoutError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(_) => f.write_str("Timeout(..)"),
            Self::Closed(_) => f.write_str("Closed(..)"),
        }
    }
}

impl<T> fmt::Display for PushError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("pushing into a closed queue")
    }
}

impl fmt::Display for PopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("popping from a closed and empty queue")
    }
}

impl<T> fmt::Display for TryPushError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => f.write_str("pushing into a full queue"),
            Self::Closed(_) => f.write_str("pushing into a closed queue"),
        }
    }
}

impl fmt::Display for TryPopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("popping from an empty queue"),
            Self::Closed => f.write_str("popping from a closed and empty queue"),
        }
    }
}

impl<T> fmt::Display for PushTimeoutError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(_) => f.write_str("timed out pushing into a full queue"),
            Self::Closed(_) => f.write_str("pushing into a closed queue"),
        }
    }
}

impl fmt::Display for PopTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => f.write_str("timed out popping from an empty queue"),
            Self::Closed => f.write_str("popping from a closed and empty queue"),
        }
    }
}

impl<T> Error for PushError<T> {}
impl Error for PopError {}
impl<T> Error for TryPushError<T> {}
impl Error for TryPopError {}
impl<T> Error for PushTimeoutError<T> {}
impl Error for PopTimeoutError {}
