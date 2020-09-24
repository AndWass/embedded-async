use embedded_async::timer::*;
use std::sync::{Mutex, Arc};
use embedded_time::duration::{Microseconds, Extensions};
use std::task::Waker;
use std::time::Duration;

struct Inner {
    waker: Mutex<Option<Waker>>,
}

struct ThreadTimer
{
    inner: Arc<Inner>,
    start_stop_time: (std::time::Instant, Option<std::time::Instant>)
}

impl ThreadTimer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                waker: Mutex::new(None)
            }),
            start_stop_time: (std::time::Instant::now(), None)
        }
    }
}

impl TimerBackend for ThreadTimer {
    fn wake_in(&mut self, waker: Waker, micros: Microseconds<u32>) {
        self.start_stop_time = (std::time::Instant::now(), None);
        self.inner = Arc::new(Inner {
            waker: Mutex::new(Some(waker)),
        });

        let thread_data = self.inner.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_micros(micros.0 as u64));
            let lock = thread_data.waker.lock().unwrap();
            if let Some(w) = &*lock {
                w.wake_by_ref();
            }
        });
    }

    fn pause(&mut self) {
        *self.inner.waker.lock().unwrap() = None;
        if let (x, None) = self.start_stop_time {
            self.start_stop_time = (x, Some(std::time::Instant::now()));
        }
    }

    fn elapsed_time(&self) -> Microseconds<u32> {
        if let (begin, Some(end)) = self.start_stop_time {
            Microseconds((end-begin).as_micros() as u32)
        }
        else {
            0u32.microseconds()
        }
    }

    fn reset_elapsed_time(&mut self) {
        self.start_stop_time.1 = None;
    }
}

async fn hello_world(id: i32, delay: Microseconds, handle: TimerHandle<impl TimerBackend>) {
    loop {
        println!("Sleeping {} for {}us", id, delay.0);
        let sleep_time = std::time::Instant::now();
        handle.delay(delay).await;
        let wake_time = std::time::Instant::now();
        println!("Hello world from {}, slept for {:?}", id, wake_time - sleep_time);
        break;
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
    uio::task_start!(hello_world1, hello_world(1, Microseconds(1_000_000), handle.clone()));
    uio::task_start!(hello_world2, hello_world(2, Microseconds(1_500_000), handle));
    uio::executor::run();
}