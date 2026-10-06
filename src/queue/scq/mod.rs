//! Lock-free bounded MPMC queue built on SCQ index rings.

mod data;
mod entry;
mod ring;

use std::fmt;
use std::time::{Duration, Instant};

use self::data::DataCell;
use self::entry::Geometry;
use self::ring::{Deq, IndexRing};
use crate::error::{
    PopError, PopTimeoutError, PushError, PushTimeoutError, TryPopError, TryPushError,
};
use crate::sync::{Backoff, WaitQueue};
use crate::traits::forward_bounded_queue;

/// A bounded MPMC queue built on Nikolaev's SCQ (DISC 2019): claims are
/// `fetch_add`s, so contended threads never retry them.
///
/// Items live in an `n`-cell data array. Two rings of `2n` entries hold cell
/// indices: `fq` the free cells, `aq` the full ones in FIFO order. A push
/// takes a free index, writes the cell, and enqueues the index on `aq`; a pop
/// does the reverse.
///
/// # Compared with [`LockFreeQueue`](crate::LockFreeQueue)
///
/// * **Lock-free.** No operation ever waits on one particular other thread.
///   In the Vyukov design a producer preempted between claiming a slot and
///   publishing it stalls that slot's consumer; here the consumer invalidates
///   the slot after a bounded wait and the producer takes another position.
/// * **Slower here.** A claim is one `fetch_add`, never a CAS retry loop,
///   but each item touches two rings and a data cell. On a 10-core Apple M4
///   it is 5–8× slower than `LockFreeQueue` (`docs/SCQ.md` §7). Choose it
///   for the progress guarantee, not for throughput.
/// * **Memory.** 4 words of ring per data cell.
/// * **`Full` is weaker.** `try_push` can report `Full` while a pop has
///   removed an item but not yet returned its cell to the free ring. That is
///   the price of never waiting on the popper. With one thread, `Full` and
///   `Empty` are exact.
///
/// Capacity is rounded up to a power of two. Only available on 64-bit
/// targets, where positions cannot wrap in practice.
///
/// # Example
///
/// ```
/// use parkring::ScqQueue;
///
/// let q = ScqQueue::new(4);
/// std::thread::scope(|s| {
///     s.spawn(|| {
///         for i in 0..100 {
///             q.push(i).unwrap();
///         }
///         q.close();
///     });
///     let received: Vec<_> = std::iter::from_fn(|| q.pop().ok()).collect();
///     assert_eq!(received, (0..100).collect::<Vec<_>>());
/// });
/// ```
pub struct ScqQueue<T> {
    /// Allocated indices, FIFO. Its `tail` carries the closed flag.
    aq: IndexRing,
    /// Free indices.
    fq: IndexRing,
    consumers: WaitQueue,
    producers: WaitQueue,
    data: Box<[DataCell<T>]>,
}

impl<T> ScqQueue<T> {
    /// Creates a queue holding at least `capacity` items, rounded up to the
    /// next power of two.
    ///
    /// # Panics
    /// If `capacity` is zero or larger than 2^32.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self::with_start_position(capacity, None)
    }

    /// Starts both rings at a chosen position so tests can exercise very large
    /// positions without performing 2^62 operations.
    fn with_start_position(capacity: usize, start: Option<usize>) -> Self {
        assert!(capacity > 0, "capacity must be non-zero");
        assert!(capacity <= 1 << 32, "capacity too large");
        let geo = Geometry::new(capacity.next_power_of_two().trailing_zeros());
        let start = start.unwrap_or(geo.ring_len());
        Self {
            aq: IndexRing::new_empty(geo, start),
            fq: IndexRing::new_full(geo, start),
            consumers: WaitQueue::new(),
            producers: WaitQueue::new(),
            data: (0..geo.n()).map(|_| DataCell::new()).collect(),
        }
    }

    /// The number of data cells (a power of two).
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// Pushes `item` if a free cell is available right now.
    ///
    /// # Errors
    /// Returns the item as [`TryPushError::Full`] or [`TryPushError::Closed`].
    pub fn try_push(&self, item: T) -> Result<(), TryPushError<T>> {
        // Checking first keeps a closed queue's rings untouched, so
        // single-threaded behaviour stays exact.
        if self.aq.is_closed() {
            return Err(TryPushError::Closed(item));
        }
        let Deq::Item(index) = self.fq.dequeue(false) else {
            return Err(if self.aq.is_closed() {
                TryPushError::Closed(item)
            } else {
                TryPushError::Full(item)
            });
        };
        // SAFETY: we dequeued `index` from the free ring, so we own the cell,
        // and the free ring's Acquire makes the previous reader's read of it
        // happen-before this write.
        unsafe { self.data[index].write(item) };
        if self.aq.enqueue(index, true).is_ok() {
            self.consumers.notify_one();
            return Ok(());
        }
        // Closed between our check and our enqueue. The index was never
        // published, so the cell is still ours: take the item back and
        // return the cell.
        // SAFETY: we wrote the cell above and never published its index.
        let item = unsafe { self.data[index].read() };
        let _ = self.fq.enqueue(index, false);
        Err(TryPushError::Closed(item))
    }

    /// Pops the oldest item if one is available right now.
    ///
    /// # Errors
    /// [`TryPopError::Empty`], or [`TryPopError::Closed`] once the queue is
    /// closed and drained.
    pub fn try_pop(&self) -> Result<T, TryPopError> {
        // The closed flag lives on the allocated ring's `tail`, the line every
        // push increments, so it is only read once the ring looks empty.
        // Reading it up front on every pop made that line bounce between
        // producer and consumer cores on every operation.
        let result = match self.aq.dequeue(false) {
            // Out of threshold but closed: drain, ignoring the threshold.
            Deq::EmptyByThreshold if self.aq.is_closed() => self.aq.dequeue(true),
            other => other,
        };
        match result {
            Deq::Item(index) => {
                // SAFETY: we dequeued `index` from the allocated ring, whose
                // Acquire synchronises with the pusher's Release publish.
                let item = unsafe { self.data[index].read() };
                let _ = self.fq.enqueue(index, false);
                self.producers.notify_one();
                Ok(item)
            }
            Deq::EmptyAtTail { closed: true } => Err(TryPopError::Closed),
            Deq::EmptyAtTail { closed: false } | Deq::EmptyByThreshold => Err(TryPopError::Empty),
        }
    }

    fn push_until(
        &self,
        mut item: T,
        deadline: Option<Instant>,
    ) -> Result<(), PushTimeoutError<T>> {
        let mut backoff = Backoff::new();
        loop {
            match self.try_push(item) {
                Ok(()) => return Ok(()),
                Err(TryPushError::Closed(v)) => return Err(PushTimeoutError::Closed(v)),
                Err(TryPushError::Full(v)) => item = v,
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Err(PushTimeoutError::Timeout(item));
            }
            if backoff.is_completed() {
                self.producers
                    .wait_until(|| self.fq.ready() || self.aq.is_closed(), deadline);
                backoff.reset();
            } else {
                backoff.snooze();
            }
        }
    }

    fn pop_until(&self, deadline: Option<Instant>) -> Result<T, PopTimeoutError> {
        let mut backoff = Backoff::new();
        loop {
            match self.try_pop() {
                Ok(v) => return Ok(v),
                Err(TryPopError::Closed) => return Err(PopTimeoutError::Closed),
                Err(TryPopError::Empty) => {}
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Err(PopTimeoutError::Timeout);
            }
            if backoff.is_completed() {
                self.consumers.wait_until(|| self.aq.ready(), deadline);
                backoff.reset();
            } else {
                backoff.snooze();
            }
        }
    }

    /// Pushes `item`, spinning briefly and then parking while the queue is full.
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

    /// Pops the oldest item, spinning briefly and then parking while the
    /// queue is empty.
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

    /// Closes the queue: later pushes fail, pops drain what is left, and every
    /// parked thread is woken. Returns `true` if this call closed the queue.
    ///
    /// The flag is the top bit of the allocated ring's `tail`. A push only
    /// succeeds at a position it claimed before the flag was set in `tail`'s
    /// modification order; a later claim sees the flag and backs out.
    pub fn close(&self) -> bool {
        let newly_closed = self.aq.close();
        if newly_closed {
            self.consumers.notify_all();
            self.producers.notify_all();
        }
        newly_closed
    }

    /// Returns `true` once [`close`](Self::close) has been called.
    pub fn is_closed(&self) -> bool {
        self.aq.is_closed()
    }

    /// A snapshot of the number of items in the queue.
    pub fn len(&self) -> usize {
        self.aq.len()
    }

    /// Snapshot: `true` if the queue held no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot: `true` if the queue was at capacity.
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }
}

impl<T> Drop for ScqQueue<T> {
    fn drop(&mut self) {
        if !std::mem::needs_drop::<T>() {
            return;
        }
        // `&mut self`: no operation is in flight, so every index is either in
        // the free ring or in the allocated ring, and consumed entries read ⊥.
        let data = &self.data;
        self.aq.for_each_index(|index| {
            // SAFETY: an index in the allocated ring marks an initialised,
            // unconsumed cell, and we have exclusive access.
            unsafe { data[index].drop_in_place() };
        });
    }
}

impl<T> fmt::Debug for ScqQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScqQueue")
            .field("capacity", &self.capacity())
            .field("len", &self.len())
            .field("closed", &self.is_closed())
            .field("aq", &self.aq)
            .field("fq", &self.fq)
            .finish_non_exhaustive()
    }
}

forward_bounded_queue!(ScqQueue);

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    /// Empty pops advance `head` past `tail`; catchup must bring `tail` back
    /// so the next push lands where the next pop looks.
    #[test]
    fn empty_pops_then_push_pop_round_trips() {
        for cap in [1, 2, 4, 16] {
            let q = ScqQueue::new(cap);
            for round in 0..200 {
                for _ in 0..(3 * q.capacity() + 2) {
                    assert_eq!(q.try_pop(), Err(TryPopError::Empty));
                }
                q.try_push(round).unwrap();
                assert_eq!(q.try_pop(), Ok(round));
            }
        }
    }

    /// Exhausting the free ring's threshold with failed pushes must not stop
    /// a later push once a cell is freed.
    #[test]
    fn full_pushes_then_pop_frees_a_cell() {
        for cap in [1, 2, 8] {
            let q = ScqQueue::new(cap);
            let n = q.capacity();
            for i in 0..n {
                q.try_push(i).unwrap();
            }
            for round in 0..100 {
                for _ in 0..(3 * n + 2) {
                    assert!(q.try_push(usize::MAX).unwrap_err().is_full());
                }
                assert_eq!(q.try_pop(), Ok(round));
                q.try_push(n + round).unwrap();
            }
        }
    }

    #[test]
    fn huge_positions_do_not_overflow() {
        let geo = Geometry::new(3);
        let start = (1usize << 62) / geo.ring_len() * geo.ring_len();
        let q = ScqQueue::with_start_position(8, Some(start));
        for round in 0..50 {
            for i in 0..8 {
                q.try_push(round * 8 + i).unwrap();
            }
            assert!(q.try_push(0).unwrap_err().is_full());
            for i in 0..8 {
                assert_eq!(q.try_pop(), Ok(round * 8 + i));
            }
            assert_eq!(q.try_pop(), Err(TryPopError::Empty));
        }
    }
}
