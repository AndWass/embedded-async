use embedded_async::timer::*;
use std::sync::{Mutex, Arc, Condvar};
use embedded_time::duration::{Microseconds, Seconds};
use std::task::Waker;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use std::thread::JoinHandle;

#[derive(PartialOrd, PartialEq, Debug, Copy, Clone)]
enum TimerState {
    Started(u32),
    Paused,
    Stopped,
}

struct Inner {
    cv: Condvar,
    state: Mutex<TimerState>,
    waker: Mutex<Option<Waker>>,
    elapsed: std::sync::atomic::AtomicU32,
    is_running: AtomicBool,
}

struct ThreadTimer {
    state: Arc<Inner>
}

impl ThreadTimer {
    pub fn new() -> (Self, JoinHandle<()>) {
        let retval = Self {
            state: Arc::new(Inner {
                cv: Condvar::new(),
                state: Mutex::new(TimerState::Paused),
                waker: Mutex::new(None),
                elapsed: AtomicU32::new(0),
                is_running: AtomicBool::new(false),
            })
        };

        let thread_data = retval.state.clone();

        let joiner = std::thread::spawn(move || {
            loop {
                let pause_timer = || {
                    *thread_data.state.lock().unwrap() = TimerState::Paused;
                };

                thread_data.is_running.store(false, Ordering::Release);

                let current_state =
                {
                    let lock = thread_data.state.lock().unwrap();
                    *thread_data.cv.wait_while(lock, |x| {
                        match *x {
                            TimerState::Paused => true,
                            _ => false,
                        }
                    }).unwrap()
                };

                thread_data.elapsed.store(0, Ordering::Release);
                thread_data.is_running.store(true, Ordering::Release);

                if let TimerState::Started(x) = current_state {
                    let start_time = std::time::Instant::now();
                    let stop_time = start_time + Duration::from_micros(x as u64);

                    let timeout = {
                        let guard = thread_data.state.lock().unwrap();
                        let res = thread_data.cv.wait_timeout_while(guard,
                            Duration::from_micros(x.into()), |x| {
                            if let TimerState::Started(_) = *x {
                                true
                            } else {
                                false
                            }
                        });

                        res.unwrap().1.timed_out()
                    };

                    if timeout {
                        let mut waker = thread_data.waker.lock().unwrap();
                        if let Some(x) = (*waker).take() {
                            x.wake();
                        }
                    }

                    let elapsed = std::time::Instant::now() - start_time;
                    thread_data.elapsed.store(elapsed.as_micros() as u32, Ordering::Release);

                    pause_timer();
                }

                if current_state == TimerState::Stopped {
                    break;
                }
            }
        });

        (retval, joiner)
    }

    fn stop(&self) {
        *self.state.state.lock().unwrap() = TimerState::Stopped;
    }
}

impl TimerBackend for ThreadTimer {
    fn wake_in(&mut self, waker: Waker, micros: Microseconds<u32>) {
        *self.state.waker.lock().unwrap() = Some(waker);
        {
            *self.state.state.lock().unwrap() = TimerState::Started(micros.0);
            self.state.cv.notify_one();
        }
        while self.state.is_running.load(Ordering::Acquire) == false {}
    }

    fn pause(&mut self) {
        {
            *self.state.state.lock().unwrap() = TimerState::Paused;
            self.state.cv.notify_one();
        }

        while self.state.is_running.load(Ordering::Acquire) {}
    }

    fn elapsed_time(&self) -> Microseconds<u32> {
        Microseconds::new(self.state.elapsed.load(Ordering::Acquire))
    }

    fn reset_elapsed_time(&self) {
        self.state.elapsed.store(0, Ordering::Release);
    }
}

async fn hello_world(id: i32, delay: Microseconds, handle: TimerHandle<impl TimerBackend>) {
    loop {
        println!("Sleeping {}", id);
        let sleep_time = std::time::Instant::now();
        handle.delay(delay).await;
        let wake_time = std::time::Instant::now();
        println!("Hello world from {}, slept for {:?}", id, wake_time - sleep_time);
    }
}


fn main() {
    /*let (timer_backend, thread_join) = ThreadTimer::new();
    let timer = Timer::new(timer_backend);
    pin_utils::pin_mut!(timer);
    let (task, handle) = timer.split().unwrap();

    uio::task_start!(timer_task, embedded_async::timer::run_timer(task));
    uio::task_start!(hello_world1, hello_world(1, Microseconds(1_000_000), handle.clone()));
    uio::task_start!(hello_world2, hello_world(2, Microseconds(1_500_000), handle));
    uio::executor::run();
    thread_join.join();*/

    let mutex = Mutex::new(false);
    let cv = Condvar::new();
    loop {
        let start = std::time::Instant::now();
        let lock = mutex.lock().unwrap();
        cv.wait_timeout_while(lock, Duration::from_micros(333_333), |x| {
            true
        });
        let end = std::time::Instant::now();
        println!("{:?}", end-start);
    }
}