use core::task::{Waker, Context, Poll};
use core::future::Future;
use core::pin::Pin;

use crate::intrusive::rc::*;
use crate::intrusive::double_list::{List, Node};

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
    /// This function will only called while the timer is paused and `elapsed_time` after this must
    /// return 0.
    fn reset_elapsed_time(&mut self);
}

struct WaitingTimer {
    time_until_timeout: Microseconds,
    waker: Option<Waker>,
}

struct TimerHandleRcRef<T: TimerBackend> {
    rc: RcRef<*mut Timer<T>>
}

impl<T: TimerBackend> Clone for TimerHandleRcRef<T> {
    fn clone(&self) -> Self {
        Self {
            rc: RcRef::clone(&self.rc)
        }
    }
}

impl<T: TimerBackend> Drop for TimerHandleRcRef<T> {
    fn drop(&mut self) {
        if self.rc.ref_count() == 1 {
            let timer = unsafe { &mut **self.rc };
            // About to drop the last handle, wake the runner task so that it can exit as well!
            timer.waker.as_ref().and_then(|w| Some(w.wake_by_ref()));
        }
    }
}

pub struct Timer<Timer: TimerBackend> {
    backend: Option<Timer>,
    waker: Option<Waker>,
    sleeping_timers: List<WaitingTimer>,
    new_timers: List<WaitingTimer>,
    rc_delays: RcAnchor<*mut Self>,
    rc_runner: RcAnchor<*mut Self>,
}

impl<T: TimerBackend> Timer<T> {
    fn backend(&self) -> &T {
        self.backend.as_ref().expect("Non-existing timer backend!")
    }

    fn backend_mut(&mut self) -> &mut T {
        self.backend.as_mut().expect("Non-existing timer backend!")
    }

    /// Construct a new timer from a timer backend.
    pub fn new(backend: T) -> Self {
        Self {
            backend: Some(backend),
            waker: None,
            sleeping_timers: List::new(),
            new_timers: List::new(),
            rc_delays: RcAnchor::new(core::ptr::null_mut()),
            rc_runner: RcAnchor::new(core::ptr::null_mut()),
        }
    }

    /// Split a pinned timer
    pub fn split(self: Pin<&mut Self>) -> Option<(TimerTask<T>, TimerHandle<T>)> {
        if (*self.rc_runner).is_null() {
            let me = unsafe { self.get_unchecked_mut() };
            me.rc_delays = RcAnchor::new(me);
            me.rc_runner = RcAnchor::new(me);
            unsafe {
                Some(
                    (TimerTask {
                        timer: me.rc_runner.get_ref(),
                    },
                     TimerHandle {
                         timer: TimerHandleRcRef { rc: me.rc_delays.get_ref() },
                     })
                )
            }
        } else {
            None
        }
    }

    pub fn consume(mut self) -> Option<T> {
        self.backend.take()
    }

    pub unsafe fn consume_task(mut task: TimerTask<T>) -> Option<T> {
        let timer = &mut **task.timer;
        timer.backend.take()
    }
}

pub struct TimerTask<T: TimerBackend> {
    timer: RcRef<*mut Timer<T>>,
}

impl<T: TimerBackend> TimerTask<T> {
    /// Runs the timer task
    ///
    /// If this task isn't running no timer handles will be serviced.
    ///
    /// The function returns once all timer handles are destroyed
    pub async fn run(self) -> Self {
        let runner = TimerTaskRunner::<T> {
            timer: unsafe { &mut **self.timer }
        };
        runner.await;
        self
    }
}

struct TimerTaskRunner<'a, T: TimerBackend> {
    timer: &'a mut Timer<T>,
}

impl<T: TimerBackend> TimerTaskRunner<'_, T> {
    fn update_sleeping_timers(&mut self, elapsed: Microseconds) -> Option<Microseconds> {
        // Loops through all currently sleeping timers, decrementing their time until timeout
        // with the elapsed time. If the timers have timed out they will be woken up.
        // otherwise they are re-added to the list of sleeping timers.

        let mut sleepers = List::<WaitingTimer>::new();
        self.timer.sleeping_timers.move_to_front_of(&mut sleepers);
        let mut next_wait_time: Option<Microseconds> = None;

        while let Some(x) = sleepers.pop_front() {
            let x_ptr = x as *mut _;

            if let Some(owner) = x.owner_mut() {
                owner.time_until_timeout = Microseconds::new(owner.time_until_timeout.0.saturating_sub(elapsed.0));
                if owner.time_until_timeout > 0u32.microseconds() {
                    unsafe { self.timer.sleeping_timers.push_link_back(owner, x_ptr) };

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
        // Takes all timers in the new_timers list and adds them to the list of sleeping timers
        let mut to_append = List::<WaitingTimer>::new();
        self.timer.new_timers.move_to_front_of(&mut to_append);

        let mut retval: Option<Microseconds> = None;

        while let Some(n) = to_append.pop_front() {
            let n_ptr = n as *mut _;

            let owner = n.owner_mut().expect("");
            if owner.time_until_timeout.0 > 0 {
                retval = retval.and_then(|w| {
                    Some(w.min(owner.time_until_timeout))
                }).or(Some(owner.time_until_timeout));
                unsafe {
                    self.timer.sleeping_timers.push_link_back(owner, n_ptr);
                }
            }
            else {
                owner.waker.as_ref().and_then(|x| Some(x.wake_by_ref()));
            }
        }

        retval
    }

    fn pause_backend(&mut self) -> Microseconds {
        let backend = self.timer.backend_mut();
        backend.pause();
        let elapsed = backend.elapsed_time();
        backend.reset_elapsed_time();
        elapsed
    }
}

impl<T: TimerBackend> Future for TimerTaskRunner<'_, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe {
            self.get_unchecked_mut()
        };

        if me.timer.rc_delays.ref_count() == 0 {
            return Poll::Ready(());
        }

        me.timer.waker = Some(cx.waker().clone());
        let elapsed = me.pause_backend();

        let mut next_wait_time: Option<Microseconds> = None;

        if elapsed.0 > 0 {
            // Need to update sleeping timers, wake any elapsed timers
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
            me.timer.backend_mut().wake_in(cx.waker().clone(), next_wait_time);
        }

        // Always poll pending, this future will never complete!
        Poll::Pending
    }
}

pub struct TimerHandle<T: TimerBackend> {
    timer: TimerHandleRcRef<T>,
}

impl<T: TimerBackend> Clone for TimerHandle<T> {
    fn clone(&self) -> Self {
        Self {
            timer: self.timer.clone()
        }
    }
}

impl<T: TimerBackend> TimerHandle<T> {
    pub fn delay(&self, duration: Microseconds) -> DelayFuture<T> {
        DelayFuture {
            timer: self.timer.clone(),
            intrusive_node: Node::new(WaitingTimer {
                waker: None,
                time_until_timeout: duration,
            }),
        }
    }
}

pub struct DelayFuture<T: TimerBackend> {
    timer: TimerHandleRcRef<T>,
    intrusive_node: Node<WaitingTimer>,
}

impl<T: TimerBackend> Future for DelayFuture<T>
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.intrusive_node.data.waker.is_some() {
            // Already installed and waiting on timeout!
            Poll::Pending
        }
        else if self.intrusive_node.data.time_until_timeout > 0u32.microseconds() {
            let me = unsafe { self.get_unchecked_mut() };
            let timer = unsafe { &mut **me.timer.rc };
            timer.new_timers.push_node_back(&mut me.intrusive_node);
            me.intrusive_node.data.waker = Some(cx.waker().clone());
            if let Some(x) = &timer.waker {
                x.wake_by_ref();
            }

            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}