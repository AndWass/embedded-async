use core::task::{Waker, Context, Poll};
use core::future::Future;
use core::pin::Pin;

use crate::intrusive::double_list::List;

use embedded_time::duration;
use core::hint::unreachable_unchecked;

pub type DurationType = duration::Microseconds;

pub trait TimerBackend {
    /// Starts the timer.
    ///
    /// The timer backend must call `waker.wake()` or `waker.wake_by_ref()` when
    /// `micros` micro-seconds has elapsed.
    ///
    /// If the backend timer is already running it must discard the old waker
    /// and restart the timer again using the new waker and the new duration.
    fn wake_in(&mut self, waker: Waker, micros: DurationType);
    /// Pauses the timer.
    ///
    /// A paused timer must not call wake, and any calls to
    /// `TimerBackend::elapsed_time` must return the same value. Pausing an already paused timer
    /// should be a no-op to the best of the backends ability.
    fn pause(&mut self);
    /// Get the elapsed time since start.
    ///
    /// The value is only required to be valid when the timer is paused.
    fn elapsed_time(&self) -> DurationType;
    /// Reset the time since start.
    ///
    /// `elapsed_time` must always return 0 after this function has been called.
    fn reset(&mut self);
}

struct WaitingTimers {
    time_until_timeout: DurationType,
    waker: Waker,
}

pub struct TimerTask<Timer: TimerBackend> {
    backend: Timer,
    waker: Option<Waker>,
    sleeping_timers: List<WaitingTimers>,
    new_timers: List<WaitingTimers>,
}

impl<T: TimerBackend> Future for TimerTask<T> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe {
            self.get_unchecked_mut()
        };
        me.waker = Some(cx.waker().clone());
        me.backend.pause();
        let elapsed = me.backend.elapsed_time();
        let zero = DurationType::new(0);

        let mut next_wait_time: Option<DurationType> = None;
        if elapsed.0 > 0 {
            let mut sleepers = List::<WaitingTimers>::new();
            me.sleeping_timers.move_to(&mut sleepers);

            while let Some(x) = sleepers.pop() {
                let x_ptr = x as *mut _;

                if let Some(owner) = x.owner_mut() {
                    owner.time_until_timeout = DurationType::new(owner.time_until_timeout.0.saturating_sub(elapsed.0));
                    if owner.time_until_timeout > zero {
                        unsafe { me.sleeping_timers.push(owner, x_ptr) };

                        next_wait_time = next_wait_time.and_then(|cur_min| {
                            Some(cur_min.min(owner.time_until_timeout))
                        }).or(Some(owner.time_until_timeout));
                    }
                    else {
                        owner.waker.wake_by_ref();
                    }
                }
                else {
                    unsafe { unreachable_unchecked() };
                }
            }
        }

        if let Some(next_wait_time) = next_wait_time {
            me.backend.wake_in(cx.waker().clone(), next_wait_time);
        }


        Poll::Pending
    }
}