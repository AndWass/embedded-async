use crate::intrusive::double_list::{List, Node};
use crate::intrusive::rc::*;
use core::task::{Waker, Context, Poll};
use core::pin::Pin;
use core::future::Future;

struct Waiter {
    waker: Option<Waker>,
    notified: bool,
}

struct Condvar {
    rc_waiter: RcAnchor<*mut Self>,
    rc_notifier: RcAnchor<*mut Self>,
    waiters: List<Waiter>,
}

struct CondvarWaiter {
    condvar: RcRef<*mut Condvar>,
    node: Node<Waiter>,
}

struct CondvarNotifier {
    condvar: RcRef<*mut Condvar>,
}

impl CondvarNotifier {
    pub fn notify_one(&self) {
        unsafe {
            (&mut **self.condvar).waiters.pop_front().and_then(|x| {
                x.owner_mut()
            }).and_then(|o| {
                o.notified = true;
                o.waker.take().and_then(|w| Some(w.wake()))
            });
        }
    }
}
