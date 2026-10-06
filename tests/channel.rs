//! Behaviour of `parkring::channel`: delivery, disconnection from either
//! side, wakeups of blocked threads, and that no message leaks or drops twice.

mod common;

use std::collections::HashSet;
use std::thread;
use std::time::{Duration, Instant};

use common::drop_counter::{DropStats, Tracked};
use common::{PARK_DELAY, scale};
use parkring::channel::{
    self, RecvError, RecvTimeoutError, SendTimeoutError, TryRecvError, TrySendError,
};

#[test]
fn delivers_in_order_with_one_sender_and_one_receiver() {
    let (tx, rx) = channel::bounded(4);
    let n = u32::try_from(scale(10_000)).unwrap();
    let producer = thread::spawn(move || {
        for i in 0..n {
            tx.send(i).unwrap();
        }
    });
    let received: Vec<u32> = rx.iter().collect();
    producer.join().unwrap();
    assert_eq!(received, (0..n).collect::<Vec<_>>());
}

#[test]
fn last_sender_dropped_lets_receivers_drain_then_disconnects() {
    let (tx, rx) = channel::bounded(4);
    let tx2 = tx.clone();
    tx.send(1).unwrap();
    tx2.send(2).unwrap();
    drop(tx);
    assert!(!rx.is_disconnected(), "one sender is still alive");
    tx2.send(3).unwrap();
    drop(tx2);
    assert!(rx.is_disconnected());
    assert_eq!(rx.recv(), Ok(1));
    assert_eq!(rx.try_recv(), Ok(2));
    assert_eq!(rx.recv_timeout(Duration::from_millis(1)), Ok(3));
    assert_eq!(rx.recv(), Err(RecvError));
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(1)),
        Err(RecvTimeoutError::Disconnected)
    );
}

#[test]
fn last_receiver_dropped_makes_sends_fail_and_return_the_message() {
    let (tx, rx) = channel::bounded(4);
    let rx2 = rx.clone();
    drop(rx);
    tx.send(1).unwrap();
    assert!(!tx.is_disconnected(), "one receiver is still alive");
    drop(rx2);
    assert!(tx.is_disconnected());
    assert_eq!(tx.send(2).unwrap_err().into_inner(), 2);
    assert_eq!(tx.try_send(3), Err(TrySendError::Disconnected(3)));
    assert_eq!(
        tx.send_timeout(4, Duration::from_millis(1)),
        Err(SendTimeoutError::Disconnected(4))
    );
}

#[test]
fn try_and_timeout_variants_report_full_and_empty() {
    let (tx, rx) = channel::bounded(2);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    let start = Instant::now();
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(20)),
        Err(RecvTimeoutError::Timeout)
    );
    assert!(start.elapsed() >= Duration::from_millis(20));

    tx.try_send(1).unwrap();
    tx.try_send(2).unwrap();
    assert!(tx.is_full());
    assert_eq!(tx.len(), 2);
    assert_eq!(tx.try_send(3), Err(TrySendError::Full(3)));
    assert_eq!(
        tx.send_timeout(4, Duration::from_millis(20)),
        Err(SendTimeoutError::Timeout(4))
    );
    assert_eq!(rx.try_iter().collect::<Vec<_>>(), [1, 2]);
    assert!(rx.is_empty());
}

#[test]
fn a_receiver_parked_on_an_empty_channel_wakes_when_the_last_sender_drops() {
    let (tx, rx) = channel::bounded::<u32>(4);
    let waiter = thread::spawn(move || rx.recv());
    thread::sleep(PARK_DELAY);
    drop(tx);
    assert_eq!(waiter.join().unwrap(), Err(RecvError));
}

#[test]
fn a_sender_parked_on_a_full_channel_wakes_when_the_last_receiver_drops() {
    let (tx, rx) = channel::bounded(2);
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    let waiter = thread::spawn(move || tx.send(3));
    thread::sleep(PARK_DELAY);
    drop(rx);
    assert_eq!(waiter.join().unwrap().unwrap_err().into_inner(), 3);
}

#[test]
fn many_senders_and_receivers_deliver_every_message_exactly_once() {
    const SENDERS: u64 = 4;
    const RECEIVERS: usize = 4;
    let per_sender = scale(5_000) as u64;
    let (tx, rx) = channel::bounded(16);

    let receivers: Vec<_> = (0..RECEIVERS)
        .map(|_| {
            let rx = rx.clone();
            thread::spawn(move || rx.into_iter().collect::<Vec<u64>>())
        })
        .collect();
    drop(rx);
    let senders: Vec<_> = (0..SENDERS)
        .map(|id| {
            let tx = tx.clone();
            thread::spawn(move || {
                for i in 0..per_sender {
                    tx.send(id * per_sender + i).unwrap();
                }
            })
        })
        .collect();
    drop(tx);

    for s in senders {
        s.join().unwrap();
    }
    let mut seen = HashSet::new();
    for r in receivers {
        let got = r.join().unwrap();
        // Each receiver sees each sender's messages in send order.
        for id in 0..SENDERS {
            let from: Vec<_> = got.iter().filter(|&&m| m / per_sender == id).collect();
            assert!(from.windows(2).all(|w| w[0] < w[1]), "per-sender FIFO");
        }
        for m in got {
            assert!(seen.insert(m), "message {m} received twice");
        }
    }
    assert_eq!(seen.len() as u64, SENDERS * per_sender);
}

#[test]
fn messages_left_in_a_dropped_channel_are_dropped_once() {
    let stats = DropStats::new();
    {
        let (tx, rx) = channel::bounded(8);
        for i in 0..5 {
            tx.send(Tracked::new(i, &stats)).unwrap();
        }
        drop(rx.recv().unwrap());
        // A send after the receivers are gone hands the message back; dropping
        // the error drops it.
        drop(rx);
        drop(tx.send(Tracked::new(99, &stats)).unwrap_err());
        assert_eq!(stats.live(), 4, "four messages still in the channel");
    }
    assert_eq!(stats.created(), 6);
    assert_eq!(stats.live(), 0, "every message dropped exactly once");
}

#[test]
fn capacity_rounds_up_like_the_queue() {
    let (tx, rx) = channel::bounded::<u8>(5);
    assert_eq!(tx.capacity(), 8);
    assert_eq!(rx.capacity(), 8);
    let (tx, _rx) = channel::bounded::<u8>(1);
    assert_eq!(tx.capacity(), 2);
}

#[test]
#[should_panic(expected = "capacity must be non-zero")]
fn zero_capacity_panics() {
    let _ = channel::bounded::<u8>(0);
}

#[test]
fn debug_does_not_require_debug_messages() {
    struct Opaque;
    let (tx, rx) = channel::bounded(2);
    tx.send(Opaque).unwrap();
    assert_eq!(
        format!("{tx:?}"),
        "Sender { len: 1, capacity: 2, disconnected: false }"
    );
    assert!(format!("{rx:?}").starts_with("Receiver {"));
    drop(rx);
    let err = tx.send(Opaque).unwrap_err();
    assert_eq!(format!("{err:?}"), "SendError(..)");
    assert_eq!(err.to_string(), "sending on a disconnected channel");
}
