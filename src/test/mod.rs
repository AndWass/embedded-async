//! Internal module used to test futures
//!

use core::task::{RawWakerVTable, RawWaker, Waker, Poll, Context};
use core::future::Future;

#[allow(dead_code)]
fn wake(data: *const ()) {
    unsafe {
        let flag = &mut *(data as *const bool as *mut bool);
        *flag = true;
    }
}

#[allow(dead_code)]
fn raw_drop(_: *const ()) {}

fn clone(data: *const ()) -> RawWaker
{
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, raw_drop);
    RawWaker::new(data, &VTABLE)
}

pub fn poll_once<R>(flag: &mut bool, future: &mut impl core::future::Future<Output=R>) -> Poll<R>
{
    let pinned = unsafe { core::pin::Pin::new_unchecked(future) };

    let waker = clone(flag as *mut bool as *const _);
    let waker = unsafe { Waker::from_raw(waker) };
    let mut cx = Context::from_waker(&waker);

    pinned.poll(&mut cx)
}

pub struct ManualPoll<R, F: Future<Output=R>> {
    future: F,
    flag: bool,
}

impl<R, F: Future<Output=R>> ManualPoll<R, F> {
    #[allow(dead_code)]
    pub fn new(future: F) -> Self {
        Self {
            future,
            flag: false,
        }
    }

    #[allow(dead_code)]
    pub fn poll(&mut self) -> Option<R> {
        self.flag = false;
        match poll_once(&mut self.flag, &mut self.future) {
            Poll::Pending => None,
            Poll::Ready(x) => Some(x)
        }
    }

    #[allow(dead_code)]
    pub fn is_woken_up(&self) -> bool {
        self.flag
    }
}