//! Channel errors, named like `std::sync::mpsc`'s and `crossbeam-channel`'s.
//!
//! As with the queue errors, every error raised while the caller still owns
//! the message hands the message back.

use std::error::Error;
use std::fmt;

/// Returned by [`Sender::send`](super::Sender::send) when every receiver has
/// been dropped. Carries the message.
///
/// ```
/// let (tx, rx) = parkring::channel::bounded(4);
/// drop(rx);
/// assert_eq!(tx.send("hello").unwrap_err().into_inner(), "hello");
/// ```
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SendError<T>(pub T);

/// Returned by [`Receiver::recv`](super::Receiver::recv) when every sender has
/// been dropped and the channel is empty.
///
/// ```
/// use parkring::channel::{self, RecvError};
///
/// let (tx, rx) = channel::bounded(4);
/// tx.send(1).unwrap();
/// drop(tx);
/// assert_eq!(rx.recv(), Ok(1)); // what was sent is still delivered
/// assert_eq!(rx.recv(), Err(RecvError));
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RecvError;

/// Returned by [`Sender::try_send`](super::Sender::try_send).
///
/// ```
/// use parkring::channel::{self, TrySendError};
///
/// let (tx, rx) = channel::bounded(2);
/// tx.try_send(1).unwrap();
/// tx.try_send(2).unwrap();
/// assert_eq!(tx.try_send(3), Err(TrySendError::Full(3)));
/// drop(rx);
/// assert_eq!(tx.try_send(4), Err(TrySendError::Disconnected(4)));
/// ```
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TrySendError<T> {
    /// The channel is at capacity.
    Full(T),
    /// Every receiver has been dropped.
    Disconnected(T),
}

/// Returned by [`Receiver::try_recv`](super::Receiver::try_recv).
///
/// ```
/// use parkring::channel::{self, TryRecvError};
///
/// let (tx, rx) = channel::bounded::<u32>(2);
/// assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
/// drop(tx);
/// assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TryRecvError {
    /// No message is waiting.
    Empty,
    /// Every sender has been dropped and the channel is empty.
    Disconnected,
}

/// Returned by [`Sender::send_timeout`](super::Sender::send_timeout).
///
/// ```
/// use std::time::Duration;
/// use parkring::channel::{self, SendTimeoutError};
///
/// let (tx, _rx) = channel::bounded(2);
/// tx.send(1).unwrap();
/// tx.send(2).unwrap();
/// let err = tx.send_timeout(3, Duration::from_millis(10)).unwrap_err();
/// assert_eq!(err, SendTimeoutError::Timeout(3));
/// ```
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum SendTimeoutError<T> {
    /// The channel stayed full until the timeout elapsed.
    Timeout(T),
    /// Every receiver has been dropped.
    Disconnected(T),
}

/// Returned by [`Receiver::recv_timeout`](super::Receiver::recv_timeout).
///
/// ```
/// use std::time::Duration;
/// use parkring::channel::{self, RecvTimeoutError};
///
/// let (tx, rx) = channel::bounded::<u32>(2);
/// assert_eq!(rx.recv_timeout(Duration::from_millis(10)), Err(RecvTimeoutError::Timeout));
/// drop(tx);
/// assert_eq!(rx.recv_timeout(Duration::from_millis(10)), Err(RecvTimeoutError::Disconnected));
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RecvTimeoutError {
    /// The channel stayed empty until the timeout elapsed.
    Timeout,
    /// Every sender has been dropped and the channel is empty.
    Disconnected,
}

impl<T> SendError<T> {
    /// Returns the message that could not be sent.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> TrySendError<T> {
    /// Returns the message that could not be sent.
    pub fn into_inner(self) -> T {
        match self {
            Self::Full(m) | Self::Disconnected(m) => m,
        }
    }

    /// Returns `true` if the send failed because the channel was full.
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full(_))
    }

    /// Returns `true` if the send failed because every receiver was dropped.
    pub fn is_disconnected(&self) -> bool {
        matches!(self, Self::Disconnected(_))
    }
}

impl<T> SendTimeoutError<T> {
    /// Returns the message that could not be sent.
    pub fn into_inner(self) -> T {
        match self {
            Self::Timeout(m) | Self::Disconnected(m) => m,
        }
    }

    /// Returns `true` if the send failed because the timeout elapsed.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }

    /// Returns `true` if the send failed because every receiver was dropped.
    pub fn is_disconnected(&self) -> bool {
        matches!(self, Self::Disconnected(_))
    }
}

// `Debug` is written by hand so that `T: Debug` is not required, matching
// `std::sync::mpsc::SendError`.
impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SendError(..)")
    }
}

impl<T> fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => f.write_str("Full(..)"),
            Self::Disconnected(_) => f.write_str("Disconnected(..)"),
        }
    }
}

impl<T> fmt::Debug for SendTimeoutError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(_) => f.write_str("Timeout(..)"),
            Self::Disconnected(_) => f.write_str("Disconnected(..)"),
        }
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("sending on a disconnected channel")
    }
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("receiving on an empty and disconnected channel")
    }
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => f.write_str("sending on a full channel"),
            Self::Disconnected(_) => f.write_str("sending on a disconnected channel"),
        }
    }
}

impl fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("receiving on an empty channel"),
            Self::Disconnected => f.write_str("receiving on an empty and disconnected channel"),
        }
    }
}

impl<T> fmt::Display for SendTimeoutError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout(_) => f.write_str("timed out sending on a full channel"),
            Self::Disconnected(_) => f.write_str("sending on a disconnected channel"),
        }
    }
}

impl fmt::Display for RecvTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => f.write_str("timed out receiving on an empty channel"),
            Self::Disconnected => f.write_str("receiving on an empty and disconnected channel"),
        }
    }
}

impl<T> Error for SendError<T> {}
impl Error for RecvError {}
impl<T> Error for TrySendError<T> {}
impl Error for TryRecvError {}
impl<T> Error for SendTimeoutError<T> {}
impl Error for RecvTimeoutError {}
