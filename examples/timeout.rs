use embedded_async::timer::{Timer, TimerHandle, TimerBackend};
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::{Instant, Duration};

struct Inner {
    waker: Mutex<Option<Waker>>,
}

struct ThreadTimer {
    inner: Arc<Inner>,
    start_stop_time: (Instant, Option<Instant>),
}

impl ThreadTimer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                waker: Mutex::new(None),
            }),
            start_stop_time: (Instant::now(), None),
        }
    }
}

impl TimerBackend for ThreadTimer {
    type Duration = std::time::Duration;

    fn wake_in(&mut self, waker: Waker, dur: Self::Duration) {
        self.start_stop_time = (Instant::now(), None);
        self.inner = Arc::new(Inner {
            waker: Mutex::new(Some(waker)),
        });

        let thread_data = self.inner.clone();
        std::thread::spawn(move || {
            std::thread::sleep(dur);
            let lock = thread_data.waker.lock().unwrap();
            if let Some(w) = &*lock {
                w.wake_by_ref();
            }
        });
    }

    fn pause(&mut self) -> Self::Duration {
        *self.inner.waker.lock().unwrap() = None;
        if let (x, None) = self.start_stop_time {
            let stop_time = Instant::now();
            self.start_stop_time = (x, Some(stop_time));
            stop_time - x
        } else {
            Self::Duration::from_secs(0)
        }
    }
}

async fn immediate() -> i32 {
    println!("Immediate work done!");
    20
}

async fn work_work(timer: TimerHandle<ThreadTimer>) -> i32 {
    timer.delay(Duration::from_secs(1)).await;
    10
}

async fn hello_world(handle: TimerHandle<ThreadTimer>) {
    use embedded_async::prelude::*;

    let res = work_work(handle.clone()).timeout(handle.clone(),
                      Duration::from_millis(500)).await;
    println!("Result {:?}", res);
    let res = work_work(handle.clone()).timeout(handle.clone(),
                                                Duration::from_millis(1500)).await;
    println!("Result again: {:?}", res);
}

async fn deferred(handle: TimerHandle<ThreadTimer>) {
    use embedded_async::prelude::*;
    println!("Delaying some work");
    let res = immediate().delay(handle.clone(), Duration::from_secs(2)).await;
    println!("Delayed work result {}", res);
}

fn main() {
    // This is our timer from the timer backend
    let timer = Timer::new(ThreadTimer::new());
    pin_utils::pin_mut!(timer);
    let (task, handle) = timer.split().unwrap();

    uio::task_start!(timer_task, task.run());
    uio::task_start!(hello_world2, hello_world(handle.clone()));
    uio::task_start!(def, deferred(handle));
    uio::executor::run();
    println!("Exiting!");
}
