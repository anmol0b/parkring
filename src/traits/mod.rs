//! Public traits.

pub(crate) mod bounded_queue;
pub(crate) use bounded_queue::sealed;

pub use bounded_queue::BoundedQueue;
pub(crate) use bounded_queue::forward_bounded_queue;
