//! Compile-time thread-safety contract. The negative cases (`Rc<T>` is not
//! accepted) are `compile_fail` doctests in `src/lib.rs`.

use std::cell::Cell;

use parkring::{BlockingQueue, BoundedQueue, LockFreeQueue, Stealer, Worker};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn queues_are_send_and_sync_for_send_items() {
    assert_send_sync::<LockFreeQueue<String>>();
    assert_send_sync::<BlockingQueue<String>>();
    // `T: Send` is enough: items are moved between threads, never shared.
    assert_send_sync::<LockFreeQueue<Cell<u32>>>();
    assert_send_sync::<BlockingQueue<Cell<u32>>>();
    #[cfg(target_pointer_width = "64")]
    {
        assert_send_sync::<parkring::ScqQueue<String>>();
        assert_send_sync::<parkring::ScqQueue<Cell<u32>>>();
    }
}

fn assert_send<T: Send>() {}

#[test]
fn channel_handles_are_send_and_sync_for_send_messages() {
    use parkring::channel::{Receiver, Sender};
    assert_send_sync::<Sender<String>>();
    assert_send_sync::<Receiver<String>>();
    assert_send_sync::<Sender<Cell<u32>>>();
    assert_send_sync::<Receiver<Cell<u32>>>();
}

#[test]
fn deque_handles_have_the_right_auto_traits() {
    assert_send::<Worker<String>>();
    assert_send_sync::<Stealer<String>>();
    assert_send_sync::<Stealer<Cell<u32>>>();
}

#[test]
fn trait_is_object_safe() {
    let queues: Vec<Box<dyn BoundedQueue<u32>>> = vec![
        Box::new(LockFreeQueue::new(2)),
        Box::new(BlockingQueue::new(2)),
    ];
    for q in &queues {
        q.push(1).unwrap();
        assert_eq!(q.pop(), Ok(1));
    }
}
