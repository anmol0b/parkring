//! loom models for channel disconnection.
//!
//! ```text
//! RUSTFLAGS="--cfg loom" cargo test --release --test loom_channel
//! ```
//!
//! A channel disconnects by closing its queue when the last handle on one
//! side drops. These models check the races that adds on top of the queue's
//! own close: a blocked thread must never sleep through the last handle going
//! away, and the last of several concurrent drops must still disconnect.
#![cfg(loom)]

use loom::thread;
use parkring::channel::{self, RecvError, TrySendError};

fn model(f: impl Fn() + Sync + Send + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.max_branches = 20_000;
    let from_env = std::env::var("LOOM_MAX_PREEMPTIONS")
        .ok()
        .and_then(|v| v.parse().ok());
    builder.preemption_bound = Some(from_env.map_or(3, |n: usize| n.min(3)));
    builder.check(f);
}

/// C1. A receiver parking on an empty channel must wake when the only sender
/// is dropped on another thread.
#[test]
fn receiver_wakes_when_the_last_sender_drops() {
    model(|| {
        let (tx, rx) = channel::bounded::<u32>(2);
        let dropper = thread::spawn(move || drop(tx));
        assert_eq!(rx.recv(), Err(RecvError));
        dropper.join().unwrap();
    });
}

/// C2. A sender parking on a full channel must wake, and get its message back,
/// when the only receiver is dropped on another thread.
#[test]
fn sender_wakes_when_the_last_receiver_drops() {
    model(|| {
        let (tx, rx) = channel::bounded(2);
        tx.try_send(0).unwrap();
        tx.try_send(1).unwrap();
        let dropper = thread::spawn(move || drop(rx));
        assert_eq!(tx.send(2).unwrap_err().into_inner(), 2);
        dropper.join().unwrap();
    });
}

/// C3. Two senders each send once and drop at the same time. Exactly one of
/// the drops is the last, so the receiver gets both messages and then sees
/// the disconnect, whichever order the drops land in.
#[test]
fn concurrent_sender_drops_disconnect_once_after_every_message() {
    model(|| {
        let (tx, rx) = channel::bounded(2);
        let tx2 = tx.clone();
        let a = thread::spawn(move || tx.send(1).unwrap());
        let b = thread::spawn(move || tx2.send(2).unwrap());
        let mut got: Vec<u32> = rx.iter().collect();
        got.sort_unstable();
        assert_eq!(got, [1, 2]);
        assert_eq!(rx.recv(), Err(RecvError));
        a.join().unwrap();
        b.join().unwrap();
    });
}

/// C4. A send racing with the last receiver's drop either lands before the
/// disconnect or fails with the message; it never blocks or vanishes.
#[test]
fn send_racing_the_last_receiver_drop_succeeds_or_returns_the_message() {
    model(|| {
        let (tx, rx) = channel::bounded(2);
        let dropper = thread::spawn(move || drop(rx));
        match tx.try_send(7) {
            Ok(()) => {}
            Err(TrySendError::Disconnected(m)) => assert_eq!(m, 7),
            Err(TrySendError::Full(_)) => panic!("a two-slot channel cannot be full"),
        }
        dropper.join().unwrap();
        assert!(tx.is_disconnected());
    });
}
