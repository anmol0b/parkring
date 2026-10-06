//! The shared behaviour suite, run against both implementations.
//!
//! Test names read `lockfree::fifo_many_laps`, `blocking::mpmc_4p_4c`, and so on.

mod common;

common::queue_tests!(lockfree, LockFreeQueue);
common::queue_tests!(blocking, BlockingQueue);
#[cfg(target_pointer_width = "64")]
common::queue_tests!(scq, ScqQueue);
