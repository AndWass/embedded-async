//! A condition variable
//!
//! This can be used to notify one or multiple waiters

use crate::intrusive::internal::*;
use crate::intrusive::rc::*;
use crate::sync::{MutexGuard, MutexRef};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

pub struct WaitError;

struct NotifyRcRef {
    condvar: RcRef<*mut Condvar>,
}

impl Drop for NotifyRcRef {
    fn drop(&mut self) {
        unsafe {
            let condvar = &mut **self.condvar;
            if condvar.rc_notifier.ref_count() == 1 {
                condvar.try_notify_all();
            }
        }
    }
}

struct Waiter {
    waker: Option<Waker>,
    notified: bool,
}

impl Default for Waiter {
    fn default() -> Self {
        Self {
            waker: None,
            notified: false,
        }
    }
}

impl Waiter {
    fn notify(&mut self) {
        self.notified = true;
        self.waker.take().and_then(|w| Some(w.wake()));
    }
}

pub struct Condvar {
    rc_waiter: RcAnchor<*mut Self>,
    rc_notifier: RcAnchor<*mut Self>,
    waiters: List<Waiter>,
}

impl Condvar {
    fn try_notify_one(&mut self) {
        self.waiters
            .pop_front()
            .and_then(|x| x.owner_mut())
            .and_then(|o| Some(o.notify()));
    }

    fn try_notify_all(&mut self) {
        while let Some(waiter) = self.waiters.pop_front() {
            waiter.owner_mut().and_then(|o| Some(o.notify()));
        }
    }
}

pub struct CondvarWaiter {
    condvar: RcRef<*mut Condvar>,
}

impl CondvarWaiter {
    pub async fn wait<T>(&self, mut mutex: MutexGuard<T>) -> Result<MutexGuard<T>, WaitError> {
        let mref = mutex.source();
        mutex.unlock();
        let wait_result = WaitFuture {
            node: Node::new(Waiter::default()),
            condvar: RcRef::clone(&self.condvar),
        }
        .await;

        match wait_result {
            Ok(()) => Ok(mref.lock().await),
            Err(err) => Err(err),
        }
    }

    pub async fn wait_until<T, F: FnMut(&mut T) -> bool>(
        &self,
        mut mutex: MutexGuard<T>,
        mut condition: F,
    ) -> Result<MutexGuard<T>, WaitError> {
        while !condition(&mut *mutex) {
            mutex = match self.wait(mutex).await {
                Ok(m) => m,
                Err(e) => return Err(e),
            };
        }

        Ok(mutex)
    }
}

pub struct WaitFuture {
    node: Node<Waiter>,
    condvar: RcRef<*mut Condvar>,
}

impl Future for WaitFuture {
    type Output = Result<(), WaitError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe { self.get_unchecked_mut() };
        let condvar = unsafe { &mut **me.condvar };
        if me.node.data.notified {
            Poll::Ready(Ok(()))
        } else if condvar.rc_notifier.ref_count() == 0 {
            Poll::Ready(Err(WaitError))
        } else {
            me.node.data.waker = Some(cx.waker().clone());
            condvar.waiters.push_node_back(&mut me.node);

            Poll::Pending
        }
    }
}

pub struct CondvarNotifier {
    condvar: RcRef<*mut Condvar>,
}

impl CondvarNotifier {
    pub fn notify_one(&self) {
        unsafe {
            (&mut **self.condvar).try_notify_one();
        }
    }

    pub fn notify_all(&self) {
        unsafe {
            let condvar = &mut **self.condvar;
            condvar.try_notify_all();
        }
    }
}
