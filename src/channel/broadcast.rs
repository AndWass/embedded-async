use core::task::{Waker, Poll};
use core::future::Future;
use core::pin::Pin;

use crate::intrusive::double_list::*;
use crate::intrusive::rc::*;
use core::hint::unreachable_unchecked;
use core::task::Context;

struct SenderRef<T: Clone + 'static>(RcRef<*mut Broadcast<T>>);

impl<T: Clone + 'static> SenderRef<T> {
    fn channel(&self) -> & Broadcast<T> {
        unsafe { &**self.0 }
    }

    unsafe fn channel_mut(&self) -> &mut Broadcast<T> {
        &mut **self.0
    }
}

impl<T: Clone + 'static> Clone for SenderRef<T> {
    fn clone(&self) -> Self {
        Self(RcRef::clone(&self.0))
    }
}

impl<T: Clone + 'static> Drop for SenderRef<T> {
    fn drop(&mut self) {
        if self.0.ref_count() == 1 {
            // Last sender reference about to be dropped, notify all senders so that they
            // can get an error if applicable
            unsafe { self.channel_mut().close_senders() };
        }
    }
}

struct ReceiverRef<T: Clone + 'static>(RcRef<*mut Broadcast<T>>);

impl<T: Clone + 'static> Clone for ReceiverRef<T> {
    fn clone(&self) -> Self {
        Self(RcRef::clone(&self.0))
    }
}

impl<T: Clone + 'static> ReceiverRef<T> {
    fn channel(&self) -> &Broadcast<T> {
        unsafe { &**self.0 }
    }

    unsafe fn channel_mut(&self) -> &mut Broadcast<T> {
        &mut **self.0
    }
}

pub struct ReceiveError;
pub struct SendError;

struct WaitingReceiver<T: Clone + 'static> {
    waker: Waker,
}

pub struct Broadcast<T: Clone + 'static> {
    subscribers: List<WaitingReceiver<T>>,
    tx_rx_rc: Option<(
        RcAnchor<*mut Self>,
        RcAnchor<*mut Self>,
    )>,
    value: Option<T>,
}

impl<T: Clone + 'static> Broadcast<T> {
    pub fn new() -> Self {
        Self {
            subscribers: List::new(),
            tx_rx_rc: None,
        }
    }

    pub fn split(self: Pin<&mut Self>) -> Option<(Sender<T>, Subscriber<T>)> {
        if self.tx_rx_rc.is_some() {
            None
        }
        else {
            unsafe {
                use crate::intrusive::rc::RcAnchor;
                let me = self.get_unchecked_mut();
                let me_ptr = me as *mut Self;
                me.tx_rx_rc = Some((RcAnchor::new(me_ptr as _), RcAnchor::new(me_ptr as _)));

                let (tx, rx) = me.tx_rx_rc.as_mut().expect("");
                Some((
                    Sender {
                        channel: SenderRef(tx.get_ref()),
                    },
                    Subscriber {
                        channel: ReceiverRef(rx.get_ref()),
                    },
                ))
            }
        }
    }

    fn poll_receive(&mut self, subscriber: &mut WaitingReceiver<T>,
                    link: *mut Link<WaitingReceiver<T>>) ->  Poll<Result<T, ReceiveError>>{
        if let Some((tx, rx)) = &self.tx_rx_rc {
            if tx.ref_count() == 0 {
                Poll::Ready(Err(ReceiveError))
            }
            else {
                unsafe { self.subscribers.push(subscriber, link) };
                Poll::Pending
            }
        }
        else {
            unreachable!("");
        }
    }

    fn publish_value(&mut self, value: T) -> Result<(), SendError> {
        if let Some((tx, rx)) = &self.tx_rx_rc {
            if rx.ref_count() == 0 {
                Err(SendError)
            }
            else {
                let mut subs = List::<WaitingReceiver<T>>::new();
                self.subscribers.move_to(&mut subs);
                while let Some(s) = subs.pop() {
                    if let Some(x) = s.owner_mut() {
                        x.value = Some(Ok(value.clone()));
                        x.waker.wake_by_ref();
                    }
                }
                Ok(())
            }
        }
        else {
            unreachable!("")
        }
    }

    fn close_senders(&mut self) {
        let mut subs = List::<WaitingReceiver<T>>::new();
        self.subscribers.move_to(&mut subs);
        while let Some(s) = subs.pop() {
            if let Some(x) = s.owner_mut() {
                x.value = Some(Err(ReceiveError));
                x.waker.wake_by_ref();
            }
        }
    }
}

pub struct Sender<T: Clone + 'static> {
    channel: SenderRef<T>,
}

pub struct Subscriber<T: Clone + 'static> {
    channel: ReceiverRef<T>,
}

pub struct SubscribeFuture<T: Clone + 'static> {
    rc: ReceiverRef<T>,
    waiter: WaitingReceiver<T>,
    link: Link<WaitingReceiver<T>>
}

impl<T: Clone + 'static> Future for SubscribeFuture<T> {
    type Output = Result<T, ReceiveError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe { self.get_unchecked_mut() };
        unsafe { me.rc.channel_mut() }.poll_receive(&mut me.waiter, &mut me.link)
    }
}
