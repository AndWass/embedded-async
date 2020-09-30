use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

fn poll_to_option<T>(res: Poll<T>) -> Option<T> {
    match res {
        Poll::Ready(x) => Some(x),
        _ => None,
    }
}

fn poll_fut<T: Future>(fut: &mut T, cx: &mut Context<'_>) -> Poll<T::Output> {
    let fut = unsafe { Pin::new_unchecked(fut) };
    fut.poll(cx)
}

#[must_use]
pub struct Race<T: Future, U: Future> {
    fut1: T,
    fut2: U,
    res: (Option<T::Output>, Option<U::Output>),
}

impl<T: Future, U: Future> Future for Race<T, U> {
    type Output = (Option<T::Output>, Option<U::Output>);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe { self.get_unchecked_mut() };

        if me.res.0.is_none() && me.res.1.is_none() {
            me.res.0 = poll_to_option(poll_fut(&mut me.fut1, cx));
            if me.res.0.is_none() {
                me.res.1 = poll_to_option(poll_fut(&mut me.fut2, cx));
            }
        }

        match &mut me.res {
            (None, None) => Poll::Pending,
            x => Poll::Ready((x.0.take(), x.1.take())),
        }
    }
}

#[must_use]
pub struct Join<T: Future, U: Future> {
    fut1: T,
    fut2: U,
    res: (Option<T::Output>, Option<U::Output>),
}

impl<T: Future, U: Future> Future for Join<T, U> {
    type Output = (T::Output, U::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe { self.get_unchecked_mut() };

        if me.res.0.is_none() {
            me.res.0 = poll_to_option(poll_fut(&mut me.fut1, cx));
        }
        if me.res.1.is_none() {
            me.res.1 = poll_to_option(poll_fut(&mut me.fut2, cx));
        }

        if me.res.0.is_some() && me.res.1.is_some() {
            Poll::Ready((me.res.0.take().expect(""), me.res.1.take().expect("")))
        } else {
            Poll::Pending
        }
    }
}

#[derive(Debug)]
pub struct TimeoutError;

#[must_use]
pub struct TimeoutFuture<T: Future, B: crate::timer::TimerBackend> {
    fut: Race<T, crate::timer::DelayFuture<B>>,
}

impl<T: Future, B: crate::timer::TimerBackend> Future for TimeoutFuture<T, B> {
    type Output = Result<T::Output, TimeoutError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pin = unsafe {
            let me = self.get_unchecked_mut();
            Pin::new_unchecked(&mut me.fut)
        };

        match pin.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready((Some(x), _)) => Poll::Ready(Ok(x)),
            _ => Poll::Ready(Err(TimeoutError))
        }
    }
}

#[must_use]
pub struct DelayFuture<T: Future, B: crate::timer::TimerBackend> {
    fut: T,
    delay: Option<crate::timer::DelayFuture<B>>,
}

impl<T: Future, B: crate::timer::TimerBackend> DelayFuture<T, B> {
    fn poll_fut(&mut self, cx: &mut Context<'_>) -> Poll<T::Output> {
        let pin = unsafe { Pin::new_unchecked(&mut self.fut) };
        pin.poll(cx)
    }
}

impl<T: Future, B: crate::timer::TimerBackend> Future for DelayFuture<T, B> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe {
            self.get_unchecked_mut()
        };

        if let Some(d) = &mut me.delay {
            let pin = unsafe { Pin::new_unchecked(d) };
            match pin.poll(cx) {
                Poll::Pending => Poll::Pending,
                _ => {
                    me.delay.take();
                    me.poll_fut(cx)
                }
            }
        } else {
            me.poll_fut(cx)
        }
    }
}

pub trait FutureExt {
    fn join<F>(self, fut: F) -> Join<Self, F>
        where
            Self: Future + Sized,
            F: Future,
    {
        Join {
            fut1: self,
            fut2: fut,
            res: (None, None),
        }
    }

    fn race<F>(self, fut: F) -> Race<Self, F>
        where
            Self: Future + Sized,
            F: Future,
    {
        Race {
            fut1: self,
            fut2: fut,
            res: (None, None),
        }
    }

    fn delay<B>(self, timer: crate::timer::TimerHandle<B>, dur: B::Duration) -> DelayFuture<Self, B>
        where
            Self: Future + Sized,
            B: crate::timer::TimerBackend,
    {
        DelayFuture {
            fut: self,
            delay: Some(timer.delay(dur))
        }
    }

    fn timeout<B>(self, timer: crate::timer::TimerHandle<B>, dur: B::Duration) -> TimeoutFuture<Self, B>
        where
            Self: Future + Sized,
            B: crate::timer::TimerBackend,
    {
        TimeoutFuture {
            fut: self.race(timer.delay(dur))
        }
    }
}

impl<T> FutureExt for T
    where T: Future + ?Sized {}
