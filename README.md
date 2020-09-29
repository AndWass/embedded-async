# embedded-async

This crate contains embedded-friendly async utilities with no allocations.
All utilities are executor-agnostic, examples uses the `uio` executor but
other executors such as `smol` should work as well.

## Example

```rust
use std::time::{Instant, Duration};
use embedded_async::task;
use embedded_async::channel::{Channel, Sender, Receiver};

async fn yield_for(dur: Duration) {
    let start_time = Instant::now();
    while Instant::now() - start_time < dur {
        task::yield_now().await;
    }
}

async fn receiver(id: i32, rx: Receiver<i32>) {
    loop {
        let value = rx.recv().await.unwrap();
        println!("{} Received {}", id, value);
        // Simulate doing some work with value
        task::yield_now().await;
    }
}

async fn sender(tx: Sender<i32>) {
    let mut value = 0;
    loop {
        println!("Sending {} and {}", value, value+1);
        tx.send(value).await.unwrap();
        tx.send(value + 1).await.unwrap();
        yield_for(Duration::from_secs(1)).await;
        value += 2;
    }
}

fn main() {
    // Heapless is used as the backing queue
    let channel: Channel<i32, embedded_async::channel::consts::U10> = Channel::new();
    // The channel must be pinned before it can be split
    pin_utils::pin_mut!(channel);
    // Get a sender and receiver
    let (tx, rx) = channel.split().unwrap();

    uio::task_start!(tx_task, sender(tx));
    uio::task_start!(rx_task, receiver(1, rx.clone()));
    uio::task_start!(rx_task, receiver(2, rx));

    uio::executor::run();
}
```

## Limitations

All utilities are designed with a single-threaded executor in mind. No thread-synchronization is
supported, all synchronization is done on a task-to-task basis where both tasks must run on
the same thread (and obviously cannot have thread-races between them).
