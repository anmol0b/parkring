//! Clones and drops channel handles in arbitrary order between sends and
//! receives, and checks disconnection against a model that counts handles.
#![no_main]

use std::collections::VecDeque;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use parkring::channel::{self, Receiver, Sender, TryRecvError, TrySendError};

#[derive(Arbitrary, Debug)]
enum Op {
    CloneSender(u8),
    DropSender(u8),
    CloneReceiver(u8),
    DropReceiver(u8),
    TrySend(u8, u16),
    TryRecv(u8),
    /// Blocking send, issued only when it cannot block.
    Send(u8, u16),
    /// Blocking receive, issued only when it cannot block.
    Recv(u8),
}

#[derive(Arbitrary, Debug)]
struct Input {
    capacity: u8,
    ops: Vec<Op>,
}

fn pick<T>(handles: &[T], i: u8) -> Option<&T> {
    (!handles.is_empty()).then(|| &handles[usize::from(i) % handles.len()])
}

fuzz_target!(|input: Input| {
    let requested = usize::from(input.capacity % 32) + 1;
    let (tx, rx) = channel::bounded::<u16>(requested);
    let cap = tx.capacity();
    assert!(cap >= requested);
    let mut senders: Vec<Sender<u16>> = vec![tx];
    let mut receivers: Vec<Receiver<u16>> = vec![rx];
    let mut model = VecDeque::new();
    // Once a side reaches zero handles the channel stays disconnected.
    let (mut no_senders, mut no_receivers) = (false, false);

    for op in input.ops {
        match op {
            Op::CloneSender(i) => {
                if let Some(s) = pick(&senders, i) {
                    senders.push(s.clone());
                }
            }
            Op::DropSender(i) => {
                if !senders.is_empty() {
                    senders.swap_remove(usize::from(i) % senders.len());
                    no_senders |= senders.is_empty();
                }
            }
            Op::CloneReceiver(i) => {
                if let Some(r) = pick(&receivers, i) {
                    receivers.push(r.clone());
                }
            }
            Op::DropReceiver(i) => {
                if !receivers.is_empty() {
                    receivers.swap_remove(usize::from(i) % receivers.len());
                    no_receivers |= receivers.is_empty();
                }
            }
            Op::TrySend(i, v) => {
                if let Some(s) = pick(&senders, i) {
                    let disconnected = no_receivers || no_senders;
                    let expected = if disconnected {
                        Err(TrySendError::Disconnected(v))
                    } else if model.len() == cap {
                        Err(TrySendError::Full(v))
                    } else {
                        Ok(())
                    };
                    assert_eq!(s.try_send(v), expected);
                    if expected.is_ok() {
                        model.push_back(v);
                    }
                }
            }
            Op::Send(i, v) => {
                let disconnected = no_receivers || no_senders;
                if let Some(s) = pick(&senders, i) {
                    if disconnected || model.len() < cap {
                        let result = s.send(v);
                        if disconnected {
                            assert_eq!(result.unwrap_err().into_inner(), v);
                        } else {
                            assert!(result.is_ok());
                            model.push_back(v);
                        }
                    }
                }
            }
            Op::TryRecv(i) => {
                if let Some(r) = pick(&receivers, i) {
                    let expected = match model.pop_front() {
                        Some(v) => Ok(v),
                        None if no_senders || no_receivers => Err(TryRecvError::Disconnected),
                        None => Err(TryRecvError::Empty),
                    };
                    assert_eq!(r.try_recv(), expected);
                }
            }
            Op::Recv(i) => {
                let disconnected = no_senders || no_receivers;
                if let Some(r) = pick(&receivers, i) {
                    if disconnected || !model.is_empty() {
                        assert_eq!(r.recv().ok(), model.pop_front());
                    }
                }
            }
        }
        let disconnected = no_senders || no_receivers;
        if let Some(s) = senders.first() {
            assert_eq!(s.is_disconnected(), disconnected);
            assert_eq!(s.len(), model.len());
        }
        if let Some(r) = receivers.first() {
            assert_eq!(r.is_disconnected(), disconnected);
        }
    }
});
