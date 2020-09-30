use embedded_async::task::*;
use std::time::{Duration, Instant};

async fn yield_for(dur: Duration) {
    let start_time = Instant::now();
    while Instant::now() - start_time < dur {
        yield_now().await;
    }
}

async fn runner(id: i32, yield_time: Duration) {
    loop {
        println!("Yielding {}", id);
        let start_time = std::time::Instant::now();
        yield_for(yield_time).await;
        let end_time = std::time::Instant::now();
        println!("{} yielded for {:?}", id, end_time - start_time);
    }
}

fn main() {
    uio::task_start!(t1, runner(1, Duration::from_secs(1)));
    uio::task_start!(t2, runner(2, Duration::from_secs(2)));
    uio::task_start!(t3, runner(3, Duration::from_secs(3)));
    uio::executor::run();
    println!("Exiting!");
}
