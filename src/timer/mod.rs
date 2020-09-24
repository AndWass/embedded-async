use core::task::{Waker, Context, Poll};
use core::future::Future;
use core::pin::Pin;

use crate::intrusive::rc::*;
use crate::intrusive::double_list::{List, Link};

use embedded_time::duration::{Microseconds, Extensions};
use core::hint::unreachable_unchecked;

pub trait TimerBackend {
    /// Starts the timer.
    ///
    /// The timer backend must call `waker.wake()` or `waker.wake_by_ref()` when
    /// `micros` micro-seconds has elapsed.
    fn wake_in(&mut self, waker: Waker, micros: Microseconds);
    /// Pauses the timer.
    ///
    /// A paused timer must not call wake, and any calls to
    /// `TimerBackend::elapsed_time` must return the same value. Pausing an already paused timer
    /// should be a no-op to the best of the backends ability.
    fn pause(&mut self);
    /// Get the elapsed time since start.
    ///
    /// The value is only required to be valid when the timer is paused.
    fn elapsed_time(&self) -> Microseconds;
    /// Reset the elapsed time since start.
    ///
    /// This function is only valid while the timer is paused and `elapsed_time` after this must
    /// return 0.
    fn reset_elapsed_time(&self);
}

struct WaitingTimer {
    time_until_timeout: Microseconds,
    waker: Option<Waker>,
}

pub struct Timer<Timer: TimerBackend> {
    backend: Timer,
    waker: Option<Waker>,
    sleeping_timers: List<WaitingTimer>,
    new_timers: List<WaitingTimer>,
    rc: RcAnchor<*mut Self>,
}

impl<T: TimerBackend> Timer<T> {
    pub fn new(backend: T) -> Self {
        Self {
            backend,
            waker: None,
            sleeping_timers: List::new(),
            new_timers: List::new(),
            rc: RcAnchor::new(core::ptr::null_mut()),
        }
    }

    pub fn split(self: Pin<&mut Self>) -> Option<(TimerTask<T>, TimerHandle<T>)> {
        if (*self.rc).is_null() {
            let me = unsafe { self.get_unchecked_mut() };
            me.rc = RcAnchor::new(me);
            let rc_ref = unsafe { me.rc.get_ref() };
            Some(
                (TimerTask {
                    timer: RcRef::clone(&rc_ref),
                },
                 TimerHandle {
                     timer: rc_ref
                 })
            )
        } else {
            None
        }
    }
}

pub struct TimerTask<T: TimerBackend> {
    timer: RcRef<*mut Timer<T>>,
}

struct TimerTaskRunner<'a, T: TimerBackend> {
    timer: &'a mut Timer<T>,
}

impl<T: TimerBackend> TimerTaskRunner<'_, T> {
    fn update_sleeping_timers(&mut self, elapsed: Microseconds) -> Option<Microseconds> {
        let mut sleepers = List::<WaitingTimer>::new();
        self.timer.sleeping_timers.move_to(&mut sleepers);
        let mut next_wait_time: Option<Microseconds> = None;

        while let Some(x) = sleepers.pop() {
            let x_ptr = x as *mut _;

            if let Some(owner) = x.owner_mut() {
                owner.time_until_timeout = Microseconds::new(owner.time_until_timeout.0.saturating_sub(elapsed.0));
                if owner.time_until_timeout > 0u32.microseconds() {
                    unsafe { self.timer.sleeping_timers.push(owner, x_ptr) };

                    next_wait_time = next_wait_time.and_then(|cur_min| {
                        Some(cur_min.min(owner.time_until_timeout))
                    }).or(Some(owner.time_until_timeout));
                } else {
                    owner.waker.take().expect("").wake();
                }
            } else {
                unsafe { unreachable_unchecked() };
            }
        }

        next_wait_time
    }

    fn append_new_timers(&mut self) -> Option<Microseconds> {
        let mut to_append = List::<WaitingTimer>::new();
        self.timer.new_timers.move_to(&mut to_append);

        let mut retval: Option<Microseconds> = None;

        while let Some(n) = to_append.pop() {
            let x_ptr = n as *mut _;

            if let Some(owner) = n.owner_mut() {
                retval = retval.and_then(|w| {
                    Some(w.min(owner.time_until_timeout))
                }).or(Some(owner.time_until_timeout));
                unsafe {
                    self.timer.sleeping_timers.push(owner, x_ptr);
                }
            } else {
                unsafe { unreachable_unchecked() };
            }
        }

        retval
    }
}

impl<T: TimerBackend> Future for TimerTaskRunner<'_, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe {
            self.get_unchecked_mut()
        };
        me.timer.waker = Some(cx.waker().clone());
        me.timer.backend.pause();
        let elapsed = me.timer.backend.elapsed_time();
        me.timer.backend.reset_elapsed_time();
        let mut next_wait_time: Option<Microseconds> = None;

        if elapsed.0 > 0 {
            next_wait_time = me.update_sleeping_timers(elapsed);
        } else {
            unsafe {
                for x in me.timer.sleeping_timers.iter() {
                    next_wait_time = next_wait_time.and_then(|w| {
                        Some(w.min(x.time_until_timeout))
                    }).or(Some(x.time_until_timeout));
                }
            }
        }

        let new_timers_min_timeout = me.append_new_timers();

        let next_wait_time = match (next_wait_time, new_timers_min_timeout) {
            (None, None) => None,
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, None) => x,
            (None, y) => y,
        };

        if let Some(next_wait_time) = next_wait_time {
            me.timer.backend.wake_in(cx.waker().clone(), next_wait_time);
        }

        // Always poll pending, this future will never complete!
        Poll::Pending
    }
}

pub struct TimerHandle<T: TimerBackend> {
    timer: RcRef<*mut Timer<T>>,
}

impl<T: TimerBackend> TimerHandle<T> {
    pub fn delay(&self, duration: Microseconds) -> DelayFuture<T> {
        DelayFuture {
            timer: RcRef::clone(&self.timer),
            link: Link::new(),
            waiter: WaitingTimer {
                waker: None,
                time_until_timeout: duration,
            }
        }
    }
}

impl<T: TimerBackend> Clone for TimerHandle<T> {
    fn clone(&self) -> Self {
        Self {
            timer: RcRef::clone(&self.timer)
        }
    }
}

pub struct DelayFuture<T: TimerBackend> {
    timer: RcRef<*mut Timer<T>>,
    waiter: WaitingTimer,
    link: Link<WaitingTimer>,
}

impl<T: TimerBackend> Future for DelayFuture<T>
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.waiter.waker.is_some() {
            Poll::Pending
        }
        else if self.waiter.time_until_timeout > 0u32.microseconds() {
            let me = unsafe { self.get_unchecked_mut() };
            let timer = unsafe { &mut **me.timer };
            unsafe { timer.new_timers.push(&mut me.waiter, &mut me.link) };
            me.waiter.waker = Some(cx.waker().clone());
            if let Some(x) = &timer.waker {
                x.wake_by_ref();
            }

            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

pub async fn run_timer<T: TimerBackend>(task: TimerTask<T>) {
    loop {
        let runner = TimerTaskRunner::<T> {
            timer: unsafe { &mut **task.timer }
        };
        runner.await;
    }
}