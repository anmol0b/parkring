//! Chase-Lev work-stealing deque.
//!
//! One owner pushes and pops at the bottom (LIFO); any number of thieves steal
//! from the top (FIFO). The algorithm is Chase and Lev (SPAA 2005) with the
//! memory orderings Lê, Pop, Cohen and Zappa Nardelli proved correct for C11
//! (PPoPP 2013), adapted to the C++20/Rust release-sequence rules. See
//! `docs/DEQUE.md` for the correctness argument and for what goes wrong on
//! weak hardware without the two `SeqCst` fences.

use std::marker::PhantomData;
use std::ptr::{self, NonNull};

use crate::sync::{
    Arc, AtomicPtr, AtomicUsize,
    Ordering::{Acquire, Relaxed, Release, SeqCst},
    UnsafeCell, fence,
};
use crate::utils::CachePadded;

const INITIAL_CAPACITY: usize = 64;

/// A value that can travel through the deque as one thin pointer.
///
/// # Safety
/// `from_raw(into_raw(x))` must give back `x`, and `from_raw` must be called
/// at most once per `into_raw`: it transfers ownership.
pub(crate) unsafe trait Element: Send {
    fn into_raw(self) -> NonNull<()>;
    /// # Safety
    /// `ptr` came from `into_raw` and has not been passed to `from_raw` yet.
    unsafe fn from_raw(ptr: NonNull<()>) -> Self;
}

// SAFETY: `Box::into_raw` / `Box::from_raw` round-trip, and the pointer is
// never null (zero-sized types get a dangling, non-null pointer).
unsafe impl<T: Send> Element for Box<T> {
    fn into_raw(self) -> NonNull<()> {
        NonNull::from(Box::leak(self)).cast()
    }
    unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        // SAFETY: per the trait contract, `ptr` came from `Box::leak` above.
        unsafe { Box::from_raw(ptr.cast::<T>().as_ptr()) }
    }
}

/// A power-of-two ring of pointer slots.
///
/// Slots are atomic so that a thief reading a slot the owner is overwriting is
/// a race on an atomic, not undefined behaviour. The classic implementation
/// copies elements out with a non-atomic read and discards the copy if its CAS
/// fails; that read is still a data race in the Rust/C++ model, and loom and
/// Miri would both flag it. The cost is that elements must be one pointer, so
/// `Worker<T>` boxes each value.
struct Buffer {
    slots: Box<[AtomicPtr<()>]>,
    mask: usize,
}

impl Buffer {
    // Boxed: thieves hold raw pointers to the `Buffer` itself, so its address
    // must stay fixed when it is retired.
    #[allow(clippy::unnecessary_box_returns)]
    fn new(capacity: usize) -> Box<Self> {
        debug_assert!(capacity.is_power_of_two());
        Box::new(Self {
            slots: (0..capacity)
                .map(|_| AtomicPtr::new(ptr::null_mut()))
                .collect(),
            mask: capacity - 1,
        })
    }

    fn capacity(&self) -> usize {
        self.mask + 1
    }

    fn slot(&self, index: usize) -> &AtomicPtr<()> {
        &self.slots[index & self.mask]
    }
}

/// State shared by the owner and every thief.
struct Inner<E: Element> {
    /// Next index to steal. Only ever increases, by a successful CAS.
    top: CachePadded<AtomicUsize>,
    /// One past the owner's last element. Written only by the owner.
    bottom: CachePadded<AtomicUsize>,
    /// The current buffer. Written only by the owner, when it grows.
    buffer: CachePadded<AtomicPtr<Buffer>>,
    /// Buffers replaced by growth. A thief may still be reading one, so they
    /// are freed only when the deque is dropped. The deque grows by doubling,
    /// so their total size is less than the current buffer's.
    /// Accessed only by the owner, and by `Drop`.
    ///
    /// Raw pointers, not `Box`es: turning a retired buffer back into a `Box`
    /// asserts unique ownership (a retag, which counts as a write), while a
    /// thief may still be reading it through its raw pointer. Miri reported
    /// exactly that race. Ownership is reclaimed only in `Drop`, when no
    /// thief remains.
    retired: UnsafeCell<Vec<*mut Buffer>>,
    _elements: PhantomData<E>,
}

// SAFETY: elements are `Send` and move between threads through the slots;
// `retired` is only touched by the single owner (which `Worker` enforces by
// being `!Sync` and not `Clone`) and by `Drop`.
unsafe impl<E: Element> Send for Inner<E> {}
// SAFETY: as above.
unsafe impl<E: Element> Sync for Inner<E> {}

/// Signed distance between wrapping indices.
#[inline]
fn distance(bottom: usize, top: usize) -> isize {
    bottom.wrapping_sub(top) as isize
}

impl<E: Element> Inner<E> {
    fn with_capacity(capacity: usize, start: usize) -> Self {
        Self {
            top: CachePadded::new(AtomicUsize::new(start)),
            bottom: CachePadded::new(AtomicUsize::new(start)),
            buffer: CachePadded::new(AtomicPtr::new(Box::into_raw(Buffer::new(
                capacity.next_power_of_two(),
            )))),
            retired: UnsafeCell::new(Vec::new()),
            _elements: PhantomData,
        }
    }

    /// The owner's view of the current buffer.
    ///
    /// # Safety
    /// Only the owner may call this: the pointer is valid until the owner
    /// grows the buffer (after which the old one is retired, still valid).
    unsafe fn owner_buffer(&self) -> &Buffer {
        // SAFETY: the buffer is allocated in `with_capacity` or `grow` and
        // freed only in `Drop`. Relaxed: the owner is the only writer.
        unsafe { &*self.buffer.load(Relaxed) }
    }

    /// Owner only.
    fn push(&self, element: E) {
        let b = self.bottom.load(Relaxed);
        // Acquire pairs with the Release half of thieves' CAS on `top`, so a
        // thief's read of slot `t` happens before we overwrite that slot on
        // the next lap.
        let t = self.top.load(Acquire);
        // SAFETY: we are the owner.
        let mut buffer = unsafe { self.owner_buffer() };
        if distance(b, t) >= buffer.capacity() as isize {
            // SAFETY: we are the owner.
            buffer = unsafe { self.grow(t, b) };
        }
        buffer.slot(b).store(element.into_raw().as_ptr(), Relaxed);
        // Release publishes the slot (and the element behind it) to a thief
        // that reads this `bottom`.
        self.bottom.store(b.wrapping_add(1), Release);
    }

    /// Owner only.
    fn pop(&self) -> Option<E> {
        let b = self.bottom.load(Relaxed).wrapping_sub(1);
        // SAFETY: we are the owner.
        let buffer = unsafe { self.owner_buffer() };
        // Reserve index `b`. Release: every `bottom` store is Release, so a
        // thief that reads any `bottom` value sees the slots written before
        // it (C++20 no longer extends release sequences through plain stores).
        self.bottom.store(b, Release);
        // The pop fence. The store to `bottom` must be visible before we read
        // `top`, and a thief reads `top` then `bottom`: store buffering, which
        // only SeqCst fences on both sides forbid. Without it, a thief and the
        // owner can both take the last element (docs/DEQUE.md §5).
        #[cfg(not(parkring_mutant = "deque_no_pop_fence"))]
        fence(SeqCst);
        let t = self.top.load(Relaxed);
        let len = distance(b, t);
        if len < 0 {
            // Empty: undo the reservation.
            self.bottom.store(b.wrapping_add(1), Release);
            return None;
        }
        let ptr = buffer.slot(b).load(Relaxed);
        if len > 0 {
            // More than one element: no thief can reach index `b`.
            // SAFETY: `b` was published by our own push and is now ours.
            return Some(unsafe { E::from_raw(NonNull::new_unchecked(ptr)) });
        }
        // The last element: race the thieves for it on `top`.
        let won = self
            .top
            .compare_exchange(t, t.wrapping_add(1), SeqCst, Relaxed)
            .is_ok();
        self.bottom.store(b.wrapping_add(1), Release);
        // SAFETY: winning the CAS on `top` from `t == b` makes index `b` ours.
        won.then(|| unsafe { E::from_raw(NonNull::new_unchecked(ptr)) })
    }

    /// Any thread.
    fn steal(&self) -> Steal<E> {
        let t = self.top.load(Acquire);
        // The steal fence: order the read of `top` before the read of
        // `bottom`, and place this thief in the SeqCst fence order relative to
        // the owner's pop fence. See `pop`.
        #[cfg(not(parkring_mutant = "deque_no_steal_fence"))]
        fence(SeqCst);
        // Acquire pairs with the owner's Release stores of `bottom`, making the
        // slots (and the elements behind them) visible.
        let b = self.bottom.load(Acquire);
        if distance(b, t) <= 0 {
            return Steal::Empty;
        }
        // Acquire pairs with the owner's Release publish of a grown buffer.
        // SAFETY: buffers are freed only in `Drop`.
        let buffer = unsafe { &*self.buffer.load(Acquire) };
        // May be stale or even null if we are about to lose the CAS; it is an
        // atomic load, so that is a harmless value, never dereferenced unless
        // the CAS below makes index `t` ours.
        let ptr = buffer.slot(t).load(Relaxed);
        if self
            .top
            .compare_exchange(t, t.wrapping_add(1), SeqCst, Relaxed)
            .is_err()
        {
            return Steal::Retry;
        }
        // SAFETY: the CAS gave us index `t`; the Acquire loads above make its
        // slot and element visible. A null here would mean a published index
        // held no element, which the protocol rules out.
        Steal::Success(unsafe { E::from_raw(NonNull::new(ptr).expect("stole an empty slot")) })
    }

    /// Doubles the buffer. Owner only.
    ///
    /// # Safety
    /// Must be called by the owner.
    #[cold]
    unsafe fn grow(&self, t: usize, b: usize) -> &Buffer {
        // SAFETY: we are the owner.
        let old = unsafe { self.owner_buffer() };
        let new = Buffer::new(old.capacity() * 2);
        let mut i = t;
        while i != b {
            new.slot(i).store(old.slot(i).load(Relaxed), Relaxed);
            i = i.wrapping_add(1);
        }
        let old_ptr = self.buffer.swap(Box::into_raw(new), Release);
        // SAFETY: only the owner touches `retired` outside `Drop`.
        self.retired
            .with_mut(|retired| unsafe { (*retired).push(old_ptr) });
        // SAFETY: we are the owner and just installed it.
        unsafe { self.owner_buffer() }
    }

    fn len(&self) -> usize {
        let b = self.bottom.load(Acquire);
        let t = self.top.load(Acquire);
        distance(b, t).max(0).unsigned_abs()
    }
}

impl<E: Element> Drop for Inner<E> {
    fn drop(&mut self) {
        // `&mut self`: every handle is gone, so no operation is in flight.
        let b = self.bottom.load(Relaxed);
        let t = self.top.load(Relaxed);
        // SAFETY: allocated by `Box::into_raw`, freed only here.
        let buffer = unsafe { Box::from_raw(self.buffer.load(Relaxed)) };
        let mut i = t;
        while distance(b, i) > 0 {
            let ptr = buffer.slot(i).load(Relaxed);
            // SAFETY: indices in `[top, bottom)` were pushed and never taken,
            // and the current buffer holds every one of them.
            drop(unsafe { E::from_raw(NonNull::new_unchecked(ptr)) });
            i = i.wrapping_add(1);
        }
        // Retired buffers hold copies of element pointers only; freeing them
        // frees the arrays, not the elements.
        self.retired.with_mut(|retired| {
            // SAFETY: exclusive access in `Drop`.
            let retired = unsafe { &mut *retired };
            for old in retired.drain(..) {
                // SAFETY: each pointer came from `Box::into_raw` and was
                // retired exactly once; no thief can still hold it.
                drop(unsafe { Box::from_raw(old) });
            }
        });
    }
}

/// The result of [`Stealer::steal`].
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub enum Steal<T> {
    /// The deque was empty.
    Empty,
    /// Stole an element.
    Success(T),
    /// Lost a race with another thief or the owner; the deque may still hold
    /// elements. Try again.
    Retry,
}

impl<T> Steal<T> {
    /// The stolen element, if any.
    pub fn success(self) -> Option<T> {
        match self {
            Self::Success(v) => Some(v),
            _ => None,
        }
    }

    /// `true` if the deque was empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// `true` if the steal lost a race and should be retried.
    pub fn is_retry(&self) -> bool {
        matches!(self, Self::Retry)
    }

    fn map<U>(self, f: impl FnOnce(T) -> U) -> Steal<U> {
        match self {
            Self::Empty => Steal::Empty,
            Self::Success(v) => Steal::Success(f(v)),
            Self::Retry => Steal::Retry,
        }
    }
}

/// The owner's handle over raw elements. `!Sync` and not `Clone`: exactly one
/// thread may push and pop.
pub(crate) struct RawWorker<E: Element> {
    inner: Arc<Inner<E>>,
    _not_sync: PhantomData<std::cell::Cell<()>>,
}

/// A thief's handle over raw elements.
pub(crate) struct RawStealer<E: Element> {
    inner: Arc<Inner<E>>,
}

impl<E: Element> RawWorker<E> {
    pub(crate) fn new() -> Self {
        Self::with_capacity_and_start(INITIAL_CAPACITY, 0)
    }

    fn with_capacity_and_start(capacity: usize, start: usize) -> Self {
        Self {
            inner: Arc::new(Inner::with_capacity(capacity.max(1), start)),
            _not_sync: PhantomData,
        }
    }

    pub(crate) fn stealer(&self) -> RawStealer<E> {
        RawStealer {
            inner: Arc::clone(&self.inner),
        }
    }

    pub(crate) fn push(&self, element: E) {
        self.inner.push(element);
    }

    pub(crate) fn pop(&self) -> Option<E> {
        self.inner.pop()
    }

    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<E: Element> RawStealer<E> {
    pub(crate) fn steal(&self) -> Steal<E> {
        self.inner.steal()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }
}

impl<E: Element> Clone for RawStealer<E> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// The owner's end of a work-stealing deque.
///
/// The owning thread pushes and pops at one end (LIFO, cache-hot); other
/// threads [`steal`](Stealer::steal) from the other end (FIFO). `Worker` is
/// `Send` but not `Sync`: exactly one thread uses it at a time.
///
/// Each value is boxed, so that every slot is a single atomic pointer (see
/// `docs/DEQUE.md` for why that is the sound choice).
///
/// ```
/// use parkring::{Steal, Worker};
///
/// let worker = Worker::new();
/// let stealer = worker.stealer();
/// worker.push(1);
/// worker.push(2);
/// worker.push(3);
/// assert_eq!(stealer.steal(), Steal::Success(1)); // oldest, from the top
/// assert_eq!(worker.pop(), Some(3));              // newest, from the bottom
/// assert_eq!(worker.pop(), Some(2));
/// assert_eq!(worker.pop(), None);
/// ```
pub struct Worker<T: Send> {
    raw: RawWorker<Box<T>>,
}

/// A thief's handle to a [`Worker`]'s deque. Cheap to clone; `Send + Sync`.
pub struct Stealer<T: Send> {
    raw: RawStealer<Box<T>>,
}

impl<T: Send> Worker<T> {
    /// Creates an empty deque.
    #[must_use]
    pub fn new() -> Self {
        Self {
            raw: RawWorker::new(),
        }
    }

    /// Starts with room for `capacity` values (rounded up to a power of two)
    /// and indices near `start`, so tests can force growth and index
    /// wraparound cheaply.
    ///
    /// Only with the `__test-hooks` feature, which this crate's integration
    /// tests enable. Not public API: it may change or disappear in any release.
    #[cfg(any(test, feature = "__test-hooks"))]
    #[doc(hidden)]
    #[must_use]
    pub fn with_capacity_and_start(capacity: usize, start: usize) -> Self {
        Self {
            raw: RawWorker::with_capacity_and_start(capacity, start),
        }
    }

    /// Returns a new handle for stealing from this deque.
    #[must_use]
    pub fn stealer(&self) -> Stealer<T> {
        Stealer {
            raw: self.raw.stealer(),
        }
    }

    /// Pushes a value onto the owner's end.
    pub fn push(&self, value: T) {
        self.raw.push(Box::new(value));
    }

    /// Pops the most recently pushed value, if any.
    #[must_use]
    pub fn pop(&self) -> Option<T> {
        self.raw.pop().map(|b| *b)
    }

    /// A snapshot of the number of values in the deque.
    #[must_use]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Snapshot: `true` if the deque held no values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: Send> Default for Worker<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send> Stealer<T> {
    /// Steals the oldest value.
    pub fn steal(&self) -> Steal<T> {
        self.raw.steal().map(|b| *b)
    }

    /// Snapshot: `true` if the deque held no values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }
}

impl<T: Send> Clone for Stealer<T> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
        }
    }
}

impl<T: Send> std::fmt::Debug for Worker<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}

impl<T: Send> std::fmt::Debug for Stealer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stealer").finish_non_exhaustive()
    }
}

#[cfg(all(test, loom))]
mod loom_tests;

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn owner_is_lifo_and_thief_is_fifo() {
        let w = Worker::new();
        let s = w.stealer();
        for i in 0..5 {
            w.push(i);
        }
        assert_eq!(s.steal(), Steal::Success(0));
        assert_eq!(w.pop(), Some(4));
        assert_eq!(s.steal(), Steal::Success(1));
        assert_eq!(w.pop(), Some(3));
        assert_eq!(w.pop(), Some(2));
        assert_eq!(w.pop(), None);
        assert_eq!(s.steal(), Steal::Empty);
    }

    #[test]
    fn grows_and_keeps_order() {
        let w = Worker::with_capacity_and_start(1, 0);
        let s = w.stealer();
        for i in 0..1000 {
            w.push(i);
        }
        for i in 0..500 {
            assert_eq!(s.steal(), Steal::Success(i));
        }
        for i in (500..1000).rev() {
            assert_eq!(w.pop(), Some(i));
        }
        assert!(w.is_empty());
    }

    #[test]
    fn indices_wrap_around() {
        let w = Worker::with_capacity_and_start(2, usize::MAX - 3);
        let s = w.stealer();
        for round in 0..20 {
            for i in 0..10 {
                w.push(round * 100 + i);
            }
            assert_eq!(s.steal(), Steal::Success(round * 100));
            for i in (1..10).rev() {
                assert_eq!(w.pop(), Some(round * 100 + i));
            }
            assert_eq!(w.pop(), None);
        }
    }

    #[test]
    fn zero_sized_values_work() {
        let w = Worker::new();
        let s = w.stealer();
        for _ in 0..100 {
            w.push(());
        }
        assert_eq!(s.steal(), Steal::Success(()));
        assert_eq!(w.len(), 99);
    }
}
