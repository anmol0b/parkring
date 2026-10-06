//! Exactly-once delivery and per-producer FIFO order across a grid of
//! producer/consumer counts and capacities, for blocking and non-blocking use.

mod common;

use common::{mpmc_exactly_once_and_ordered as check, scale};
#[cfg(target_pointer_width = "64")]
use parkring::ScqQueue;
use parkring::{BlockingQueue, LockFreeQueue};

const SHAPES: [(usize, usize); 6] = [(1, 1), (2, 1), (1, 2), (4, 4), (8, 2), (2, 8)];
const CAPACITIES: [usize; 4] = [1, 2, 16, 1024];

#[test]
#[cfg_attr(
    miri,
    ignore = "grid is too large for Miri; see sequential_and_concurrent"
)]
fn lockfree_grid() {
    for (p, c) in SHAPES {
        for cap in CAPACITIES {
            check::<LockFreeQueue<u64>>(p, c, cap, scale(4000) / p, false);
            check::<LockFreeQueue<u64>>(p, c, cap, scale(1000) / p, true);
        }
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "grid is too large for Miri; see sequential_and_concurrent"
)]
fn blocking_grid() {
    for (p, c) in SHAPES {
        for cap in CAPACITIES {
            check::<BlockingQueue<u64>>(p, c, cap, scale(4000) / p, false);
            check::<BlockingQueue<u64>>(p, c, cap, scale(1000) / p, true);
        }
    }
}

#[test]
fn lockfree_small_under_miri() {
    check::<LockFreeQueue<u64>>(2, 2, 2, scale(500), false);
    check::<LockFreeQueue<u64>>(2, 2, 2, scale(500), true);
}

#[cfg(target_pointer_width = "64")]
#[test]
#[cfg_attr(
    miri,
    ignore = "grid is too large for Miri; see sequential_and_concurrent"
)]
fn scq_grid() {
    for (p, c) in SHAPES {
        for cap in CAPACITIES {
            check::<ScqQueue<u64>>(p, c, cap, scale(4000) / p, false);
            check::<ScqQueue<u64>>(p, c, cap, scale(1000) / p, true);
        }
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn scq_small_under_miri() {
    check::<ScqQueue<u64>>(2, 2, 2, scale(500), false);
    check::<ScqQueue<u64>>(2, 2, 2, scale(500), true);
}
