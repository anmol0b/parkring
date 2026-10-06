//! Bounded multi-producer multi-consumer channels.
//!
//! [`bounded`] returns a [`Sender`] and a [`Receiver`] sharing one
//! [`LockFreeQueue`]. Both handles can be cloned, so any number of threads can
//! send and receive. Waiting works like the queue's: a blocked thread spins
//! briefly, then parks, so an idle receiver uses no CPU.
//!
//! The channel disconnects when every handle on one side is gone:
//!
//! * when the last [`Sender`] is dropped, receivers still get every message
//!   that was sent, and then [`recv`](Receiver::recv) returns [`RecvError`];
//! * when the last [`Receiver`] is dropped, [`send`](Sender::send) fails at
//!   once and hands the message back inside [`SendError`].
//!
//! Disconnecting is the queue's [`close`](LockFreeQueue::close), so it wakes
//! every blocked thread. loom checks that no sender or receiver can sleep
//! through it (`tests/loom_channel.rs`).
//!
//! ```
//! use std::thread;
//! use parkring::channel;
//!
//! let (tx, rx) = channel::bounded(16);
//! let producers: Vec<_> = (0..4)
//!     .map(|id| {
//!         let tx = tx.clone();
//!         thread::spawn(move || {
//!             for i in 0..100 {
//!                 tx.send(id * 100 + i).unwrap();
//!             }
//!         })
//!     })
//!     .collect();
//! drop(tx); // only the producers' clones keep the channel open now
//!
//! // Iteration ends once every producer has finished and dropped its sender.
//! let sum: u32 = rx.iter().sum();
//! assert_eq!(sum, (0..400).sum());
//! for p in producers {
//!     p.join().unwrap();
//! }
//! ```
//!
//! A channel holds at least the capacity it was created with, like
//! [`LockFreeQueue::new`]. There is no zero-capacity (rendezvous) channel and
//! no `select`; use `std::sync::mpsc` or `crossbeam-channel` for those.

mod error;

use std::fmt;
use std::time::Duration;

pub use error::{
    RecvError, RecvTimeoutError, SendError, SendTimeoutError, TryRecvError, TrySendError,
};

use crate::LockFreeQueue;
use crate::error::{PopTimeoutError, PushTimeoutError, TryPopError, TryPushError};
use crate::sync::{Arc, AtomicUsize, Ordering};

/// Creates a channel holding at least `capacity` messages.
///
/// [`Sender::capacity`] and [`Receiver::capacity`] report the actual size.
///
/// # Panics
/// If `capacity` is zero or larger than [`LockFreeQueue::new`] allows.
///
/// ```
/// let (tx, rx) = parkring::channel::bounded::<u32>(3);
/// assert!(tx.capacity() >= 3);
/// assert_eq!(tx.capacity(), rx.capacity());
/// ```
#[must_use]
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        queue: LockFreeQueue::new(capacity),
        senders: AtomicUsize::new(1),
        receivers: AtomicUsize::new(1),
    });
    (
        Sender {
            shared: Arc::clone(&shared),
        },
        Receiver { shared },
    )
}

struct Shared<T> {
    queue: LockFreeQueue<T>,
    /// Live `Sender` handles. The queue is closed when this reaches zero.
    senders: AtomicUsize,
    /// Live `Receiver` handles. The queue is closed when this reaches zero.
    receivers: AtomicUsize,
}

impl<T> Shared<T> {
    /// Drops one handle from `count`; the last one disconnects the channel.
    fn release(&self, count: &AtomicUsize) {
        // `close` is itself a release/acquire handoff that wakes every
        // waiter, so the counters need no ordering of their own beyond
        // agreeing on which handle was last.
        let previous = count.fetch_sub(1, Ordering::AcqRel);
        // The mutant forgets to disconnect; loom must catch it.
        #[cfg(not(parkring_mutant = "channel_no_disconnect"))]
        if previous == 1 {
            self.queue.close();
        }
        #[cfg(parkring_mutant = "channel_no_disconnect")]
        let _ = previous;
    }
}

/// The sending half of a channel. Clone it to send from several threads.
///
/// Dropping the last `Sender` disconnects the channel: receivers drain what is
/// left, then stop.
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

/// The receiving half of a channel. Clone it to receive on several threads;
/// each message goes to exactly one receiver.
///
/// Dropping the last `Receiver` disconnects the channel: sends fail and hand
/// the message back. Messages still in the channel are dropped with it.
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Sender<T> {
    /// Sends `msg`, blocking while the channel is full.
    ///
    /// # Errors
    /// Returns the message if every receiver has been dropped.
    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        self.shared
            .queue
            .push(msg)
            .map_err(|e| SendError(e.into_inner()))
    }

    /// Sends `msg` if there is room right now.
    ///
    /// # Errors
    /// Returns the message if the channel is full or disconnected.
    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.shared.queue.try_push(msg).map_err(|e| match e {
            TryPushError::Full(m) => TrySendError::Full(m),
            TryPushError::Closed(m) => TrySendError::Disconnected(m),
        })
    }

    /// Like [`send`](Self::send), but gives up after `timeout`.
    ///
    /// # Errors
    /// Returns the message on timeout or if the channel is disconnected.
    pub fn send_timeout(&self, msg: T, timeout: Duration) -> Result<(), SendTimeoutError<T>> {
        self.shared
            .queue
            .push_timeout(msg, timeout)
            .map_err(|e| match e {
                PushTimeoutError::Timeout(m) => SendTimeoutError::Timeout(m),
                PushTimeoutError::Closed(m) => SendTimeoutError::Disconnected(m),
            })
    }

    /// Returns `true` once every receiver has been dropped.
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        self.shared.queue.is_closed()
    }

    /// The maximum number of messages the channel holds.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.shared.queue.capacity()
    }

    /// A snapshot of the number of messages waiting in the channel.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shared.queue.len()
    }

    /// Snapshot: `true` if no messages were waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shared.queue.is_empty()
    }

    /// Snapshot: `true` if the channel was at capacity.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.shared.queue.is_full()
    }
}

impl<T> Receiver<T> {
    /// Receives the oldest message, blocking while the channel is empty.
    ///
    /// # Errors
    /// Fails once every sender has been dropped and the channel is empty.
    pub fn recv(&self) -> Result<T, RecvError> {
        self.shared.queue.pop().map_err(|_| RecvError)
    }

    /// Receives the oldest message if one is waiting right now.
    ///
    /// # Errors
    /// Fails if the channel is empty, distinguishing disconnected-and-drained.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.shared.queue.try_pop().map_err(|e| match e {
            TryPopError::Empty => TryRecvError::Empty,
            TryPopError::Closed => TryRecvError::Disconnected,
        })
    }

    /// Like [`recv`](Self::recv), but gives up after `timeout`.
    ///
    /// # Errors
    /// Fails on timeout, or once every sender has been dropped and the channel
    /// is empty.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        self.shared.queue.pop_timeout(timeout).map_err(|e| match e {
            PopTimeoutError::Timeout => RecvTimeoutError::Timeout,
            PopTimeoutError::Closed => RecvTimeoutError::Disconnected,
        })
    }

    /// A blocking iterator over messages. It ends once every sender has been
    /// dropped and the channel is empty.
    #[must_use]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter { rx: self }
    }

    /// An iterator over the messages waiting right now. It never blocks.
    ///
    /// ```
    /// let (tx, rx) = parkring::channel::bounded(4);
    /// tx.send(1).unwrap();
    /// tx.send(2).unwrap();
    /// assert_eq!(rx.try_iter().collect::<Vec<_>>(), [1, 2]);
    /// assert_eq!(rx.try_iter().next(), None); // empty, but still connected
    /// ```
    #[must_use]
    pub fn try_iter(&self) -> TryIter<'_, T> {
        TryIter { rx: self }
    }

    /// Returns `true` once every sender has been dropped. Messages may still
    /// be waiting.
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        self.shared.queue.is_closed()
    }

    /// The maximum number of messages the channel holds.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.shared.queue.capacity()
    }

    /// A snapshot of the number of messages waiting in the channel.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shared.queue.len()
    }

    /// Snapshot: `true` if no messages were waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shared.queue.is_empty()
    }

    /// Snapshot: `true` if the channel was at capacity.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.shared.queue.is_full()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        // Like `Arc::clone`: a new handle is made from an existing one, which
        // already keeps the count above zero, so no ordering is needed.
        self.shared.senders.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.shared.receivers.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared.release(&self.shared.senders);
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.release(&self.shared.receivers);
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sender")
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .field("disconnected", &self.is_disconnected())
            .finish()
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receiver")
            .field("len", &self.len())
            .field("capacity", &self.capacity())
            .field("disconnected", &self.is_disconnected())
            .finish()
    }
}

/// A blocking iterator over a [`Receiver`]'s messages, from [`Receiver::iter`].
#[derive(Debug)]
pub struct Iter<'a, T> {
    rx: &'a Receiver<T>,
}

impl<T> Iterator for Iter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.rx.recv().ok()
    }
}

/// A non-blocking iterator over a [`Receiver`]'s waiting messages, from
/// [`Receiver::try_iter`].
#[derive(Debug)]
pub struct TryIter<'a, T> {
    rx: &'a Receiver<T>,
}

impl<T> Iterator for TryIter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// An owning blocking iterator over a [`Receiver`]'s messages.
#[derive(Debug)]
pub struct IntoIter<T> {
    rx: Receiver<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.rx.recv().ok()
    }
}

impl<T> IntoIterator for Receiver<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> IntoIter<T> {
        IntoIter { rx: self }
    }
}

impl<'a, T> IntoIterator for &'a Receiver<T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
