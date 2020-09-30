use embedded_async::channel::{Channel, Receiver, Sender};
use embedded_async::task;
use std::time::{Duration, Instant};

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
        println!("Sending {} and {}", value, value + 1);
        tx.send(value).await.unwrap();
        tx.send(value + 1).await.unwrap();
        yield_for(Duration::from_secs(1)).await;
        value += 2;
    }
}

fn main() {
    let channel: Channel<i32, embedded_async::channel::consts::U10> = Channel::new();
    pin_utils::pin_mut!(channel);
    let (tx, rx) = channel.split().unwrap();

    uio::task_start!(tx_task, sender(tx));
    uio::task_start!(rx_task, receiver(1, rx.clone()));
    uio::task_start!(rx_task, receiver(2, rx));

    uio::executor::run();
}
