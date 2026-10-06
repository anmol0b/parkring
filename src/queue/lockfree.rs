//! Lock-free bounded MPMC queue with a spin-then-park slow path.

use std::fmt;
use std::time::{Duration, Instant};

use crate::error::{
    PopError, PopTimeoutError, PushError, PushTimeoutError, TryPopError, TryPushError,
};
use crate::queue::slot::Slot;
use crate::sync::pos::{CLOSED_BIT, MAX_CAPACITY, POS_MASK, pos_add, pos_diff};
use crate::sync::{
    AtomicUsize, Backoff,
    Ordering::{AcqRel, Acquire, Relaxed, Release},
    WaitQueue,
};
use crate::traits::forward_bounded_queue;
use crate::utils::CachePadded;

/// A bounded MPMC queue built on Dmitry Vyukov's per-slot sequence numbers.
///
/// * **Fast path:** `try_push` and `try_pop` claim a slot with one CAS on
///   `tail` or `head` and publish it with one `Release` store. Producers and
///   consumers working on different slots never touch the same cache line.
/// * **Slow path:** `push` and `pop` spin briefly, then yield, then **park**
///   on a condition variable. A thread waiting on an idle queue uses no CPU.
///   Parking costs the fast path one `Relaxed` load of a read-mostly counter.
/// * **Shutdown:** [`close`](Self::close) rejects further pushes, wakes every
///   parked thread, and lets consumers drain what is left.
///
/// The queue holds at least the requested capacity. Today it rounds up to a
/// power of two, with a minimum of 2, so slot indexing is a bitwise AND; that
/// rounding is an implementation detail, and [`capacity`](Self::capacity)
/// reports the real size.
///
/// # Progress guarantee
///
/// The fast path takes no locks, but the queue is not *lock-free* in the
/// formal sense: a producer preempted between claiming a slot and publishing
/// it makes consumers of that slot wait until it is rescheduled. This is the
/// same trade-off as crossbeam's `ArrayQueue` and Vyukov's original design.
///
/// # Example
///
/// ```
/// use parkring::LockFreeQueue;
///
/// let q = LockFreeQueue::new(3);
/// assert_eq!(q.capacity(), 4); // rounded up
///
/// std::thread::scope(|s| {
///     s.spawn(|| {
///         for i in 0..10 {
///             q.push(i).unwrap();
///         }
///         q.close();
///     });
///     let mut got = Vec::new();
///     while let Ok(v) = q.pop() {
///         got.push(v);
///     }
///     assert_eq!(got, (0..10).collect::<Vec<_>>());
/// });
/// ```
pub struct LockFreeQueue<T> {
    /// Next position to pop.
    head: CachePadded<AtomicUsize>,
    /// Next position to push, with [`CLOSED_BIT`] as the closed flag.
    tail: CachePadded<AtomicUsize>,
    /// Consumers parked on an empty queue. Notified by pushes.
    consumers: WaitQueue,
    /// Producers parked on a full queue. Notified by pops.
    producers: WaitQueue,
    slots: Box<[Slot<T>]>,
    /// `capacity - 1`.
    mask: usize,
}

impl<T> LockFreeQueue<T> {
    /// Creates a queue holding at least `capacity` items.
    ///
    /// # Panics
    /// If `capacity` is zero or larger than `2^(usize::BITS - 3)`.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self::with_start_position(capacity, 0)
    }

    /// Starts `head` and `tail` at `start`. Lets tests exercise position
    /// wraparound without pushing `2^63` items.
    fn with_start_position(capacity: usize, start: usize) -> Self {
        assert!(capacity > 0, "capacity must be non-zero");
        assert!(capacity <= MAX_CAPACITY, "capacity too large");
        debug_assert_eq!(start & CLOSED_BIT, 0);
        // A one-slot ring cannot work: the "full" sequence of lap `n`
        // (`pos + 1`) would equal the "empty" sequence of lap `n + 1`, so a
        // producer would overwrite a live item. Vyukov's original asserts a
        // buffer size of at least two for the same reason.
        let capacity = capacity.next_power_of_two().max(2);
        let mask = capacity - 1;
        // Slot `i` starts at the unique position in [start, start + cap) that
        // maps to it, in the "empty for this lap" state (`sequence == pos`).
        let slots = (0..capacity)
            .map(|i| Slot::new(pos_add(start, i.wrapping_sub(start) & mask)))
            .collect();
        Self {
            head: CachePadded::new(AtomicUsize::new(start)),
            tail: CachePadded::new(AtomicUsize::new(start)),
            consumers: WaitQueue::new(),
            producers: WaitQueue::new(),
            slots,
            mask,
        }
    }

    /// The number of items the queue holds when full: at least the capacity
    /// requested in [`new`](Self::new).
    #[inline]
    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    #[inline]
    fn slot(&self, pos: usize) -> &Slot<T> {
        &self.slots[pos & self.mask]
    }

    /// Pushes `item` if there is room right now.
    ///
    /// Never fails spuriously: `Full` is returned only after confirming that
    /// `tail` has not moved and `head` is a full lap behind it.
    ///
    /// # Errors
    /// Returns the item as [`TryPushError::Full`] or [`TryPushError::Closed`].
    pub fn try_push(&self, item: T) -> Result<(), TryPushError<T>> {
        let mut backoff = Backoff::new();
        let mut tail = self.tail.load(Relaxed);
        loop {
            if tail & CLOSED_BIT != 0 {
                return Err(TryPushError::Closed(item));
            }
            let slot = self.slot(tail);
            let seq = slot.sequence.load(Acquire);
            let diff = pos_diff(seq, tail);
            if diff == 0 {
                // Slot is empty for this lap. Claim it. The Acquire half of
                // AcqRel is what makes a parked consumer's registration
                // visible to `notify_one` below (see `WaitQueue`).
                match self
                    .tail
                    .compare_exchange_weak(tail, pos_add(tail, 1), AcqRel, Relaxed)
                {
                    Ok(_) => {
                        // SAFETY: we won the tail CAS having observed
                        // `seq == tail`, so we own this slot for this lap.
                        unsafe { slot.write(item) };
                        slot.sequence.store(pos_add(tail, 1), Release);
                        self.consumers.notify_one();
                        return Ok(());
                    }
                    Err(actual) => {
                        tail = actual;
                        backoff.spin();
                    }
                }
            } else if diff < 0 {
                // The slot still holds last lap's item: either the queue is
                // full, or a consumer is between its head CAS and its recycle
                // store. Only the first is a reason to fail.
                let head = self.head.load(Acquire);
                let current = self.tail.load(Relaxed);
                if current == tail && pos_diff(tail, head) >= self.capacity() as isize {
                    return Err(TryPushError::Full(item));
                }
                // A peer is at most a few instructions from finishing.
                backoff.spin();
                tail = current;
            } else {
                // Another producer already claimed this position; our `tail`
                // is stale. Snooze (spin, then yield) before reloading, as
                // crossbeam does. Under loom this is a yield, without which
                // the model may return the stale value forever.
                backoff.snooze();
                tail = self.tail.load(Relaxed);
            }
        }
    }

    /// Pops the oldest item if one is available right now.
    ///
    /// Never fails spuriously: `Empty` is returned only after confirming that
    /// `tail == head`.
    ///
    /// # Errors
    /// [`TryPopError::Empty`], or [`TryPopError::Closed`] once the queue is
    /// closed and drained.
    pub fn try_pop(&self) -> Result<T, TryPopError> {
        let mut backoff = Backoff::new();
        let mut head = self.head.load(Relaxed);
        loop {
            let slot = self.slot(head);
            let seq = slot.sequence.load(Acquire);
            let diff = pos_diff(seq, pos_add(head, 1));
            if diff == 0 {
                // Slot holds the item pushed at `head`. Claim it.
                match self
                    .head
                    .compare_exchange_weak(head, pos_add(head, 1), AcqRel, Relaxed)
                {
                    Ok(_) => {
                        // SAFETY: we won the head CAS having observed
                        // `seq == head + 1` with Acquire, which synchronises
                        // with the producer's Release publish.
                        let item = unsafe { slot.read() };
                        slot.sequence.store(pos_add(head, self.capacity()), Release);
                        self.producers.notify_one();
                        return Ok(item);
                    }
                    Err(actual) => {
                        head = actual;
                        backoff.spin();
                    }
                }
            } else if diff < 0 {
                // Not published for this lap: either the queue is empty, or a
                // producer is between its tail CAS and its publish.
                let tail = self.tail.load(Acquire);
                if tail & POS_MASK == head {
                    return Err(if tail & CLOSED_BIT == 0 {
                        TryPopError::Empty
                    } else {
                        TryPopError::Closed
                    });
                }
                // A peer is at most a few instructions from finishing.
                backoff.spin();
                head = self.head.load(Relaxed);
            } else {
                // Another consumer already claimed this position; snooze
                // and reload, as in `try_push`.
                backoff.snooze();
                head = self.head.load(Relaxed);
            }
        }
    }

    /// Wake condition for parked consumers.
    ///
    /// Reads `tail` with an RMW rather than a load: an RMW always sees the
    /// latest value, and it forms the release sequence that lets a producer's
    /// CAS see this consumer's registration. That pairing is what rules out a
    /// lost wakeup (see `WaitQueue`). A stale `head` can only make this
    /// return `true` spuriously, which is harmless.
    fn pop_ready(&self) -> bool {
        let tail = self.tail.fetch_add(0, AcqRel);
        let head = self.head.load(Relaxed);
        tail & CLOSED_BIT != 0 || tail & POS_MASK != head
    }

    /// Wake condition for parked producers: the mirror image on `head`.
    /// `close` wakes every parked thread through the wait queue, which makes
    /// the closed flag visible (via the mutex in the fallback, via the epoch's
    /// Release/Acquire pair with a futex), so a stale `tail` is harmless.
    fn push_ready(&self) -> bool {
        let head = self.head.fetch_add(0, AcqRel);
        let tail = self.tail.load(Relaxed);
        tail & CLOSED_BIT != 0 || pos_diff(tail, head) < self.capacity() as isize
    }

    /// Shared body of `push` and `push_timeout`: retry, back off, then park.
    /// After a timed-out park the loop retries once more before giving up,
    /// so an item that became pushable at the deadline is not rejected.
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
                self.producers.wait_until(|| self.push_ready(), deadline);
                backoff.reset();
            } else {
                backoff.snooze();
            }
        }
    }

    /// Shared body of `pop` and `pop_timeout`.
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
                self.consumers.wait_until(|| self.pop_ready(), deadline);
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
    /// The flag is the top bit of `tail`, so closing is ordered against every
    /// push by `tail`'s modification order: a push either claimed its slot
    /// before the close (and its item will be popped) or its CAS fails and it
    /// returns `Closed`. A separate flag could not give that guarantee.
    pub fn close(&self) -> bool {
        let previous = self.tail.fetch_or(CLOSED_BIT, AcqRel);
        let newly_closed = previous & CLOSED_BIT == 0;
        if newly_closed {
            self.consumers.notify_all();
            self.producers.notify_all();
        }
        newly_closed
    }

    /// Returns `true` once [`close`](Self::close) has been called.
    pub fn is_closed(&self) -> bool {
        self.tail.load(Acquire) & CLOSED_BIT != 0
    }

    /// A snapshot of the number of items in the queue.
    pub fn len(&self) -> usize {
        loop {
            let tail = self.tail.load(Acquire);
            let head = self.head.load(Acquire);
            // A consistent snapshot requires `tail` not to move while `head`
            // was read.
            if self.tail.load(Acquire) == tail {
                let len = pos_diff(tail, head).clamp(0, self.capacity() as isize);
                return len.unsigned_abs();
            }
        }
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

impl<T> Drop for LockFreeQueue<T> {
    fn drop(&mut self) {
        if !std::mem::needs_drop::<T>() {
            return;
        }
        // `&mut self` proves no other thread can reach the queue, and whatever
        // released the last shared reference (an `Arc` drop, a scope join)
        // made every completed operation visible. Exactly the positions in
        // `[head, tail)` hold initialised values.
        let head = self.head.load(Relaxed);
        let tail = self.tail.load(Relaxed) & POS_MASK;
        let mut pos = head;
        while pos != tail {
            let slot = self.slot(pos);
            debug_assert_eq!(slot.sequence.load(Relaxed), pos_add(pos, 1));
            // SAFETY: `pos` is in `[head, tail)` so the slot was published and
            // never consumed, and we have exclusive access.
            unsafe { slot.drop_in_place() };
            pos = pos_add(pos, 1);
        }
    }
}

impl<T> fmt::Debug for LockFreeQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockFreeQueue")
            .field("capacity", &self.capacity())
            .field("len", &self.len())
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

forward_bounded_queue!(LockFreeQueue);

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn positions_wrap_around_the_closed_bit() {
        // Start three pushes before the position space wraps.
        let q = LockFreeQueue::with_start_position(4, POS_MASK - 2);
        for round in 0..5 {
            for i in 0..4 {
                q.try_push(round * 10 + i).unwrap();
            }
            assert!(q.try_push(99).unwrap_err().is_full());
            assert_eq!(q.len(), 4);
            for i in 0..4 {
                assert_eq!(q.try_pop(), Ok(round * 10 + i));
            }
            assert_eq!(q.try_pop(), Err(TryPopError::Empty));
            assert!(!q.is_closed());
        }
    }

    #[test]
    fn drop_after_wrap_releases_exactly_the_live_items() {
        use crate::sync::Arc;
        let marker = Arc::new(());
        let q = LockFreeQueue::with_start_position(4, POS_MASK - 1);
        for _ in 0..4 {
            q.try_push(Arc::clone(&marker)).unwrap();
        }
        drop(q.try_pop().unwrap());
        q.try_push(Arc::clone(&marker)).unwrap();
        assert_eq!(Arc::strong_count(&marker), 5);
        drop(q);
        assert_eq!(Arc::strong_count(&marker), 1);
    }
}
