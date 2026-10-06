//! Concurrency primitives built and verified from first principles: bounded
//! MPMC queues and channels, a work-stealing deque, and a work-stealing thread
//! pool.
//!
//! | | what it is |
//! |---|---|
//! | [`LockFreeQueue`] | Vyukov's per-slot sequence ring. The recommended queue. |
//! | [`channel::bounded`] | [`Sender`](channel::Sender) / [`Receiver`](channel::Receiver) on a `LockFreeQueue`, disconnecting when either side drops |
//! | [`ScqQueue`] | Nikolaev's SCQ: fetch-add claims, genuinely lock-free, slower on this hardware |
//! | [`BlockingQueue`] | one mutex, two condvars: the reference implementation |
//! | [`Worker`] / [`Stealer`] | Chase-Lev work-stealing deque |
//! | [`ThreadPool`], [`join`] | a work-stealing pool built from the pieces above |
//!
//! For `select`, parallel iterators or `async`, use crossbeam-channel, Rayon
//! or an async runtime's channels instead; the README's "When to use
//! something else" section says where each one is the better choice.
//!
//! Blocked threads spin briefly, then park on a futex (`futex(2)` on Linux and
//! Android, `__ulock` on macOS) or on std's `Condvar` elsewhere, so an idle
//! thread does not burn a core. The only dependency is `libc`.
//!
//! The three queues implement [`BoundedQueue`] and share one API:
//!
//! * `push` / `pop` block;
//! * `try_push` / `try_pop` never block;
//! * `push_timeout` / `pop_timeout` give up after a deadline;
//! * `close` rejects further pushes, wakes every waiter, and lets consumers
//!   drain the remaining items before `pop` reports [`PopError`].
//!
//! Every failed push hands the item back inside the error.
//!
//! ```
//! use parkring::LockFreeQueue;
//!
//! let queue = LockFreeQueue::new(64);
//! std::thread::scope(|s| {
//!     let consumers: Vec<_> = (0..4)
//!         .map(|_| s.spawn(|| {
//!             let mut popped = 0;
//!             while queue.pop().is_ok() {
//!                 popped += 1;
//!             }
//!             popped
//!         }))
//!         .collect();
//!
//!     // Every producer is joined when this inner scope ends.
//!     std::thread::scope(|p| {
//!         for _ in 0..4 {
//!             p.spawn(|| (0..1000).for_each(|i| queue.push(i).unwrap()));
//!         }
//!     });
//!
//!     // Consumers drain what is left, then `pop` returns `Err` and they exit.
//!     queue.close();
//!     let total: usize = consumers.into_iter().map(|h| h.join().unwrap()).sum();
//!     assert_eq!(total, 4000);
//! });
//! ```
//!
//! # Thread safety
//!
//! Both queues are `Send + Sync` exactly when `T: Send`. Items move between
//! threads but are never shared, so `T: Sync` is not required:
//!
//! ```compile_fail
//! fn assert_sync<T: Sync>() {}
//! assert_sync::<parkring::LockFreeQueue<std::rc::Rc<()>>>();
//! ```
//!
//! ```compile_fail
//! fn assert_sync<T: Sync>() {}
//! assert_sync::<parkring::BlockingQueue<std::rc::Rc<()>>>();
//! ```
//!
//! ```compile_fail
//! fn assert_sync<T: Sync>() {}
//! assert_sync::<parkring::ScqQueue<std::rc::Rc<()>>>();
//! ```
//!
//! Channel handles follow the same rule, and a non-`Send` message type is
//! rejected:
//!
//! ```compile_fail
//! fn assert_send<T: Send>() {}
//! assert_send::<parkring::channel::Sender<std::rc::Rc<()>>>();
//! ```
//!
//! A deque's [`Worker`] belongs to one thread at a time: it is `Send` but not
//! `Sync`. [`Stealer`] is `Send + Sync`.
//!
//! ```compile_fail
//! fn assert_sync<T: Sync>() {}
//! assert_sync::<parkring::Worker<u32>>();
//! ```
//!
//! ```compile_fail
//! fn assert_send<T: Send>() {}
//! assert_send::<parkring::Stealer<std::rc::Rc<()>>>();
//! ```
//!
//! See `docs/DESIGN.md` in the repository for the memory-ordering argument
//! and how it is verified with loom and Miri.

pub mod channel;
mod deque;
mod error;
mod pool;
mod queue;
mod sync;
mod traits;
mod utils;

pub use deque::{Steal, Stealer, Worker};
pub use error::{
    PopError, PopTimeoutError, PushError, PushTimeoutError, TryPopError, TryPushError,
};
pub use pool::{ThreadPool, join};
#[cfg(target_pointer_width = "64")]
pub use queue::ScqQueue;
pub use queue::{BlockingQueue, LockFreeQueue};
pub use traits::BoundedQueue;

/// Compiles and runs the README's code examples as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
