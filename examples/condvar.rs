use embedded_async::sync::{MutexRef, CondvarNotifier, CondvarWaiter, Mutex, Condvar};
use std::time::{Duration, Instant};

async fn yield_for(dur: Duration) {
    let start_time = Instant::now();
    while Instant::now() - start_time < dur {
        embedded_async::task::yield_now().await;
    }
}

async fn waiter(id: i32, mutex: MutexRef<i32>, cv: CondvarWaiter) {
    loop {
        let lock = cv.wait(mutex.lock().await).await.unwrap();
        println!("Waiter {} notified with value {}", id, *lock);
    }
}

async fn wait_for_modulo(mutex: MutexRef<i32>, cv: CondvarWaiter) {
    loop {
        let lock = cv.wait_until(mutex.lock().await,
                                 |x| *x % 5 == 0).await.unwrap();
        println!("Wait for modulo notified with value {}", *lock);
    }
}

async fn notifier(mutex: MutexRef<i32>, cv: CondvarNotifier) {
    let mut value = 0;
    loop {
        yield_for(Duration::from_secs(1)).await;
        *mutex.lock().await = value;
        if value % 2 == 0 {
            println!("Notifying ALL with value {}", value);
            cv.notify_all();
        }
        else {
            println!("Notifying ONE with value {}", value);
            cv.notify_one();
        }
        value += 1;
    }
}

fn main() {
    let mutex = Mutex::new(0);
    pin_utils::pin_mut!(mutex);
    let mutex = mutex.take_ref();

    let cv = Condvar::new();
    pin_utils::pin_mut!(cv);
    let (notif, cv_waiter) = cv.split().unwrap();

    uio::task_start!(notifier, notifier(mutex.clone(), notif));
    uio::task_start!(waiter1, waiter(1, mutex.clone(), cv_waiter.clone()));
    uio::task_start!(waiter2, waiter(2, mutex.clone(), cv_waiter.clone()));
    uio::task_start!(waiter3, wait_for_modulo(mutex.clone(), cv_waiter.clone()));
    uio::executor::run();
    println!("Exiting!");
}
