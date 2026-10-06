//! The point of spin-then-park: a consumer blocked on an idle queue must not
//! burn a core. This file is its own process, so process CPU time is the
//! test's CPU time. Timing-sensitive, so it is `#[ignore]`d by default:
//!
//! ```text
//! cargo test --release --test cpu_burn -- --ignored
//! ```

#![cfg(unix)]

use std::hint;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::thread;
use std::time::{Duration, Instant};

use parkring::{BlockingQueue, BoundedQueue, LockFreeQueue};

const WINDOW: Duration = Duration::from_millis(300);

fn cpu_time() -> Duration {
    // SAFETY: `getrusage` only writes into the zeroed struct we pass it.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        assert_eq!(libc::getrusage(libc::RUSAGE_SELF, &raw mut usage), 0);
        usage
    };
    let tv = |t: libc::timeval| {
        Duration::from_secs(u64::try_from(t.tv_sec).unwrap_or(0))
            + Duration::from_micros(u64::try_from(t.tv_usec).unwrap_or(0))
    };
    tv(usage.ru_utime) + tv(usage.ru_stime)
}

/// Measures CPU used while `waiting` runs and the main thread sleeps.
fn cpu_while<F: FnOnce() + Send>(waiting: F, release: impl FnOnce()) -> (Duration, Duration) {
    thread::scope(|s| {
        let handle = s.spawn(waiting);
        // Let the waiter get past its spin phase before measuring.
        thread::sleep(Duration::from_millis(20));
        let (wall0, cpu0) = (Instant::now(), cpu_time());
        thread::sleep(WINDOW);
        let (wall, cpu) = (wall0.elapsed(), cpu_time().saturating_sub(cpu0));
        release();
        handle.join().unwrap();
        (cpu, wall)
    })
}

fn parked_consumer_is_idle<Q: BoundedQueue<u32>>(q: &Q, name: &str) {
    let (cpu, wall) = cpu_while(
        || {
            let _ = q.pop();
        },
        || q.push(1).unwrap(),
    );
    println!("{name}: parked consumer used {cpu:?} CPU over {wall:?}");
    assert!(
        cpu < wall / 20,
        "{name}: blocked consumer burned {cpu:?} over {wall:?}"
    );
}

#[test]
#[ignore = "timing-sensitive; run with --ignored"]
fn blocked_consumers_do_not_burn_cpu() {
    parked_consumer_is_idle(&LockFreeQueue::new(4), "LockFreeQueue");
    parked_consumer_is_idle(&BlockingQueue::new(4), "BlockingQueue");
    #[cfg(target_pointer_width = "64")]
    parked_consumer_is_idle(&parkring::ScqQueue::new(4), "ScqQueue");
}

/// An idle pool's workers park too.
#[test]
#[ignore = "timing-sensitive; run with --ignored"]
fn idle_pool_does_not_burn_cpu() {
    let pool = parkring::ThreadPool::new(4);
    assert_eq!(pool.install(|| 1 + 1), 2);
    let (cpu, wall) = cpu_while(|| thread::sleep(Duration::from_millis(1)), || ());
    println!("idle pool of 4 used {cpu:?} CPU over {wall:?}");
    assert!(cpu < wall / 20, "idle pool burned {cpu:?} over {wall:?}");
    drop(pool);
}

/// Proves the measurement can see a busy thread, so the test above cannot
/// pass vacuously.
#[test]
#[ignore = "timing-sensitive; run with --ignored"]
fn measurement_detects_a_spinning_thread() {
    let stop = AtomicBool::new(false);
    let (cpu, wall) = cpu_while(
        || {
            while !stop.load(Relaxed) {
                hint::spin_loop();
            }
        },
        || stop.store(true, Relaxed),
    );
    println!("spinner used {cpu:?} CPU over {wall:?}");
    assert!(cpu > wall / 2);
}
