use embedded_async::timer::*;
use embedded_time::duration::Milliseconds;
use std::sync::{Arc, Mutex};
use std::task::Waker;

struct Inner {
    waker: Mutex<Option<Waker>>,
}

struct ThreadTimer {
    inner: Arc<Inner>,
    start_stop_time: (std::time::Instant, Option<std::time::Instant>),
}

impl ThreadTimer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                waker: Mutex::new(None),
            }),
            start_stop_time: (std::time::Instant::now(), None),
        }
    }

    fn from_duration(dur: std::time::Duration) -> embedded_time::duration::Milliseconds {
        embedded_time::duration::Milliseconds::new(dur.as_millis() as u32)
    }

    fn to_duration(dur: embedded_time::duration::Milliseconds) -> std::time::Duration {
        std::time::Duration::from_millis(dur.0 as u64)
    }
}

impl TimerBackend for ThreadTimer {
    type Duration = embedded_time::duration::Milliseconds;

    fn wake_in(&mut self, waker: Waker, dur: Self::Duration) {
        self.start_stop_time = (std::time::Instant::now(), None);
        self.inner = Arc::new(Inner {
            waker: Mutex::new(Some(waker)),
        });

        let thread_data = self.inner.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Self::to_duration(dur));
            let lock = thread_data.waker.lock().unwrap();
            if let Some(w) = &*lock {
                w.wake_by_ref();
            }
        });
    }

    fn pause(&mut self) -> Self::Duration {
        *self.inner.waker.lock().unwrap() = None;
        if let (x, None) = self.start_stop_time {
            let stop_time = std::time::Instant::now();
            self.start_stop_time = (x, Some(stop_time));
            Self::from_duration(stop_time - x)
        } else {
            Self::Duration::new(0)
        }
    }
}

async fn hello_world<B: TimerBackend>(id: i32, delay: B::Duration, handle: TimerHandle<B>) {
    loop {
        println!("Sleeping {} for {:?}", id, delay);
        let sleep_time = std::time::Instant::now();
        handle.delay(delay.clone()).await;
        let wake_time = std::time::Instant::now();
        println!(
            "Hello world from {}, slept for {:?}",
            id,
            wake_time - sleep_time
        );
    }
}

fn main() {
    // This is our timer from the timer backend
    let timer = Timer::new(ThreadTimer::new());
    pin_utils::pin_mut!(timer);
    // This splits the timer in two parts, one task that needs to run for the timer to function
    // and one handle that can be used to delay for a specified time.
    let (task, handle) = timer.split().unwrap();

    uio::task_start!(timer_task, task.run());
    uio::task_start!(
        hello_world1,
        hello_world(1, Milliseconds(1000), handle.clone())
    );
    uio::task_start!(hello_world2, hello_world(2, Milliseconds(1500), handle));
    uio::executor::run();
    println!("Exiting!");
}
