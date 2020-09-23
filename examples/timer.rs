use embedded_async::timer::*;
use std::sync::{Mutex, Arc};
use embedded_time::duration::Microseconds;
use std::task::Waker;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use std::thread::JoinHandle;

#[derive(PartialOrd, PartialEq)]
enum  TimerState {
    Started(u32),
    Paused,
    Stopped,
}

struct ThreadTimer {
    state: Mutex<TimerState>,
    waker: Mutex<Option<Waker>>,
    elapsed: std::sync::atomic::AtomicU32,
}

impl ThreadTimer {
    pub fn new() -> (Arc<Self>, JoinHandle<()>) {
        let retval = Arc::new(Self {
            state: Mutex::new(TimerState::Paused),
            waker: Mutex::new(None),
            elapsed: AtomicU32::new(0),
        });

        let thread_data = retval.clone();

        let joiner = std::thread::spawn(move || {
            let mut start_time : std::time::Instant;
            loop {
                while *thread_data.state.lock().unwrap() == TimerState::Paused {
                    std::thread::sleep(Duration::from_micros(10));
                }

                thread_data.elapsed.store(0, Ordering::Release);

                if let TimerState::Started(x) = *thread_data.state.lock().unwrap() {
                    start_time = std::time::Instant::now();
                    let stop_time = start_time + Duration::from_micros(x as u64);
                    while let TimerState::Started(_) = *thread_data.state.lock().unwrap() {
                        std::thread::sleep(Duration::from_micros(10));
                        if std::time::Instant::now() >= stop_time {

                            let mut waker = thread_data.waker.lock().unwrap();
                            if let Some(x) = (*waker).take() {
                                x.wake();
                            }
                            break;
                        }
                    }

                    let elapsed = std::time::Instant::now() - start_time;
                    thread_data.elapsed.store(elapsed.as_micros() as u32, Ordering::Release);
                }

                if *thread_data.state.lock().unwrap() == TimerState::Stopped {
                    break;
                }
            }
        });

        (retval, joiner)
    }

    fn stop(&self) {
        *self.state.lock().unwrap() = TimerState::Stopped;
    }
}

impl TimerBackend for ThreadTimer {
    fn wake_in(&mut self, waker: Waker, micros: Microseconds<u32>) {
        *self.waker.lock().unwrap() = Some(waker);
        *self.state.lock().unwrap() = TimerState::Started(micros.0);
    }

    fn pause(&mut self) {
        *self.state.lock().unwrap() = TimerState::Paused;
    }

    fn elapsed_time(&self) -> Microseconds<u32> {
        Microseconds::new(self.elapsed.load(Ordering::Acquire))
    }
}

fn main() {
    let timer = ThreadTimer::new();
    timer.0.stop();
    timer.1.join().unwrap();
}
