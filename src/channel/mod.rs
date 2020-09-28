//pub mod broadcast;

pub use heapless::consts;

use crate::intrusive::internal::*;
use crate::intrusive::rc;

use core::future::Future;
use core::hint::unreachable_unchecked;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Error returned when trying to receive data from the channel
#[derive(PartialOrd, PartialEq, Debug)]
pub enum TryRecvError {
    /// No data is available, but there are senders available
    /// that can send values in the future.
    Empty,
    /// No data is available and all senders have been dropped, so no future values will be
    /// added.
    Closed,
}

#[derive(Debug)]
pub struct RecvError;

#[derive(PartialOrd, PartialEq, Debug)]
pub enum TrySendError<T> {
    Full(T),
    Closed(T),
}

#[derive(Debug)]
pub struct SendError<T>(pub T);

struct WaitingReceiver {
    waker: core::task::Waker,
}

struct WaitingSender {
    waker: core::task::Waker,
}

trait ChannelOps<T: 'static> {
    fn try_send(&mut self, value: T) -> Result<(), TrySendError<T>>;
    fn try_recv(&mut self) -> Result<T, TryRecvError>;

    unsafe fn add_waiting_sender(
        &mut self,
        waiter: &mut WaitingSender,
        link: *mut Link<WaitingSender>,
    );
    unsafe fn add_waiting_receiver(
        &mut self,
        waiter: &mut WaitingReceiver,
        link: *mut Link<WaitingReceiver>,
    );

    fn notify_waiting_senders(&mut self);
    fn notify_waiting_receivers(&mut self);

    fn len(&self) -> usize;
}

struct SenderRef<T: 'static>(rc::RcRef<*mut dyn ChannelOps<T>>);

impl<T: 'static> Clone for SenderRef<T> {
    fn clone(&self) -> Self {
        Self(rc::RcRef::clone(&self.0))
    }
}

impl<T: 'static> SenderRef<T> {
    fn channel(&self) -> &dyn ChannelOps<T> {
        unsafe { &**self.0 }
    }

    unsafe fn channel_mut(&self) -> &mut dyn ChannelOps<T> {
        &mut **self.0
    }
}

impl<T: 'static> Drop for SenderRef<T> {
    fn drop(&mut self) {
        if self.0.ref_count() == 1 {
            // Last sender reference about to be dropped, notify all senders so that they
            // can get an error if applicable
            unsafe { self.channel_mut().notify_waiting_receivers() };
        }
    }
}

struct ReceiverRef<T: 'static>(rc::RcRef<*mut dyn ChannelOps<T>>);

impl<T: 'static> ReceiverRef<T> {
    fn channel(&self) -> &dyn ChannelOps<T> {
        unsafe { &**self.0 }
    }
    unsafe fn channel_mut(&self) -> &mut dyn ChannelOps<T> {
        &mut **self.0
    }
}

impl<T: 'static> Clone for ReceiverRef<T> {
    fn clone(&self) -> Self {
        Self(rc::RcRef::clone(&self.0))
    }
}

impl<T: 'static> Drop for ReceiverRef<T> {
    fn drop(&mut self) {
        if self.0.ref_count() == 1 {
            unsafe { self.channel_mut().notify_waiting_senders() };
        }
    }
}

pub struct Channel<T: 'static, L: 'static + heapless::ArrayLength<T>> {
    queue: heapless::spsc::Queue<T, L>,
    waiting_receivers: List<WaitingReceiver>,
    waiting_senders: List<WaitingSender>,
    tx_rx_rc: Option<(
        rc::RcAnchor<*mut dyn ChannelOps<T>>,
        rc::RcAnchor<*mut dyn ChannelOps<T>>,
    )>,
}

impl<T: 'static, L: 'static + heapless::ArrayLength<T>> Channel<T, L> {
    fn receiver_ref_count(&self) -> u32 {
        if let Some((_, rx)) = &self.tx_rx_rc {
            rx.ref_count()
        } else {
            unreachable!("");
        }
    }

    fn sender_ref_count(&self) -> u32 {
        if let Some((tx, _)) = &self.tx_rx_rc {
            tx.ref_count()
        } else {
            unreachable!("");
        }
    }

    pub fn new() -> Self {
        Self {
            queue: heapless::spsc::Queue::new(),
            waiting_receivers: List::new(),
            waiting_senders: List::new(),
            tx_rx_rc: None,
        }
    }

    pub fn split(self: Pin<&mut Self>) -> Option<(Sender<T>, Receiver<T>)> {
        if self.tx_rx_rc.is_some() {
            None
        } else {
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
                    Receiver {
                        channel: ReceiverRef(rx.get_ref()),
                    },
                ))
            }
        }
    }
}

impl<T, L: heapless::ArrayLength<T>> ChannelOps<T> for Channel<T, L> {
    fn try_send(&mut self, value: T) -> Result<(), TrySendError<T>> {
        if self.receiver_ref_count() == 0 {
            Err(TrySendError::Closed(value))
        } else {
            match self.queue.enqueue(value) {
                Ok(_) => {
                    self.notify_waiting_receivers();
                    Ok(())
                }
                Err(v) => Err(TrySendError::Full(v)),
            }
        }
    }

    fn try_recv(&mut self) -> Result<T, TryRecvError> {
        if self.queue.is_empty() && self.sender_ref_count() == 0 {
            Err(TryRecvError::Closed)
        } else {
            match self.queue.dequeue() {
                Some(x) => {
                    self.notify_waiting_senders();
                    Ok(x)
                }
                None => Err(TryRecvError::Empty),
            }
        }
    }

    unsafe fn add_waiting_sender(
        &mut self,
        waiter: &mut WaitingSender,
        link: *mut Link<WaitingSender>,
    ) {
        self.waiting_senders.push_link_back(waiter, link);
    }

    unsafe fn add_waiting_receiver(
        &mut self,
        waiter: &mut WaitingReceiver,
        link: *mut Link<WaitingReceiver>,
    ) {
        self.waiting_receivers.push_link_back(waiter, link);
    }

    fn notify_waiting_senders(&mut self) {
        let mut waiting_senders = List::<WaitingSender>::new();
        self.waiting_senders.move_to_front_of(&mut waiting_senders);
        while let Some(s) = waiting_senders.pop_front() {
            if let Some(w) = s.owner() {
                w.waker.wake_by_ref();
            } else {
                unreachable!("Waiting senders must have owner!");
            }
        }
    }

    fn notify_waiting_receivers(&mut self) {
        let mut waiting_receivers = List::<WaitingReceiver>::new();
        self.waiting_receivers
            .move_to_front_of(&mut waiting_receivers);
        while let Some(r) = waiting_receivers.pop_front() {
            if let Some(w) = r.owner() {
                w.waker.wake_by_ref();
            } else {
                unreachable!("Waiting receivers must have owner!")
            }
        }
    }
    fn len(&self) -> usize {
        self.queue.len()
    }
}

#[derive(Clone)]
pub struct Sender<T: 'static> {
    channel: SenderRef<T>,
}

impl<T: 'static> Sender<T> {
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        unsafe { self.channel.channel_mut().try_send(value) }
    }

    pub fn send(&self, value: T) -> SendFuture<T> {
        SendFuture {
            value: Some(value),
            waiter: None,
            link: Link::new(),
            channel: self.channel.clone(),
        }
    }

    pub fn len(&self) -> usize {
        self.channel.channel().len()
    }
}

pub struct SendFuture<T: 'static> {
    value: Option<T>,
    waiter: Option<WaitingSender>,
    link: Link<WaitingSender>,
    channel: SenderRef<T>,
}

impl<T: 'static> Future for SendFuture<T> {
    type Output = Result<(), SendError<T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe { self.get_unchecked_mut() };
        let channel = unsafe { &mut **me.channel.0 };
        let value = me.value.take().expect("");
        match channel.try_send(value) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(TrySendError::Closed(value)) => Poll::Ready(Err(SendError(value))),
            Err(TrySendError::Full(value)) => {
                unsafe {
                    me.value = Some(value);
                    me.waiter = Some(WaitingSender {
                        waker: cx.waker().clone(),
                    });
                    if let Some(x) = &mut me.waiter {
                        // Safe since x and me.link are both stored inside the future
                        channel.add_waiting_sender(x, &mut me.link);
                    } else {
                        // Safe since we just assigned to me.waiter
                        unreachable_unchecked();
                    }
                }
                Poll::Pending
            }
        }
    }
}

#[derive(Clone)]
pub struct Receiver<T: 'static> {
    channel: ReceiverRef<T>,
}

impl<T: 'static> Receiver<T> {
    fn mut_channel(&self) -> &mut dyn ChannelOps<T> {
        unsafe { self.channel.channel_mut() }
    }
    fn channel(&self) -> &dyn ChannelOps<T> {
        self.channel.channel()
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.mut_channel().try_recv()
    }

    pub fn recv(&self) -> ReceiveFuture<T> {
        ReceiveFuture {
            waiter: None,
            link: Link::new(),
            channel: self.channel.clone(),
        }
    }

    pub fn len(&self) -> usize {
        self.channel().len()
    }
}

pub struct ReceiveFuture<T: 'static> {
    waiter: Option<WaitingReceiver>,
    link: Link<WaitingReceiver>,
    channel: ReceiverRef<T>,
}

impl<T: 'static> Future for ReceiveFuture<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = unsafe { self.get_unchecked_mut() };
        let channel = unsafe { me.channel.channel_mut() };
        match channel.try_recv() {
            Ok(value) => Poll::Ready(Ok(value)),
            Err(TryRecvError::Closed) => Poll::Ready(Err(RecvError)),
            Err(TryRecvError::Empty) => {
                unsafe {
                    me.waiter = Some(WaitingReceiver {
                        waker: cx.waker().clone(),
                    });
                    if let Some(x) = &mut me.waiter {
                        // This is safe since both x and me.link are stored inside the future
                        channel.add_waiting_receiver(x, &mut me.link);
                    } else {
                        // This is safe since we just assigned Some(..) to me.waiter
                        unreachable_unchecked();
                    }
                }
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sets_ints_refs() {
        let mut chan: Channel<i32, consts::U8> = Channel::new();
        let pin = unsafe { Pin::new_unchecked(&mut chan) };
        let (sender, receiver) = pin.split().unwrap();
        assert_eq!(chan.receiver_ref_count(), 1);
        assert_eq!(chan.sender_ref_count(), 1);

        core::mem::drop(sender);
        assert_eq!(chan.receiver_ref_count(), 1);
        assert_eq!(chan.sender_ref_count(), 0);

        core::mem::drop(receiver);
        assert_eq!(chan.receiver_ref_count(), 0);
        assert_eq!(chan.sender_ref_count(), 0);
    }

    #[test]
    #[should_panic]
    fn forgetting_sender_panics() {
        {
            let mut chan: Channel<i32, consts::U8> = Channel::new();
            let pin = unsafe { Pin::new_unchecked(&mut chan) };
            let (sender, _receiver) = pin.split().unwrap();
            core::mem::forget(sender);
        }
    }

    #[test]
    #[should_panic]
    fn forgetting_receiver_panics() {
        {
            let mut chan: Channel<i32, consts::U8> = Channel::new();
            let pin = unsafe { Pin::new_unchecked(&mut chan) };
            let (_sender, receiver) = pin.split().unwrap();
            core::mem::forget(receiver);
        }
    }

    #[test]
    fn send_receive_one() {
        let chan: Channel<i32, consts::U8> = Channel::new();
        pin_utils::pin_mut!(chan);
        let (sender, rx) = chan.split().unwrap();
        assert_eq!(sender.len(), 0);
        assert_eq!(rx.len(), 0);
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Empty);
        assert!(sender.try_send(10).is_ok());
        assert_eq!(sender.len(), 1);
        assert_eq!(rx.len(), 1);
        assert_eq!(rx.try_recv().unwrap(), 10);
        assert_eq!(sender.len(), 0);
        assert_eq!(rx.len(), 0);
    }

    #[test]
    fn receive_closed() {
        let chan: Channel<i32, consts::U8> = Channel::new();
        pin_utils::pin_mut!(chan);
        let (sender, rx) = chan.split().unwrap();
        assert_eq!(sender.len(), 0);
        assert_eq!(rx.len(), 0);
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Empty);
        assert!(sender.try_send(10).is_ok());
        assert_eq!(sender.len(), 1);
        assert_eq!(rx.len(), 1);
        core::mem::drop(sender);

        assert_eq!(rx.try_recv().unwrap(), 10);
        assert_eq!(rx.len(), 0);
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Closed);
    }

    #[test]
    fn send_closed() {
        let chan: Channel<i32, consts::U8> = Channel::new();
        pin_utils::pin_mut!(chan);
        let (sender, rx) = chan.split().unwrap();
        assert_eq!(sender.len(), 0);
        assert_eq!(rx.len(), 0);
        assert_eq!(rx.try_recv().unwrap_err(), TryRecvError::Empty);
        assert!(sender.try_send(10).is_ok());
        assert_eq!(sender.len(), 1);
        assert_eq!(rx.len(), 1);
        core::mem::drop(rx);

        assert_eq!(sender.try_send(20).unwrap_err(), TrySendError::Closed(20));
        assert_eq!(sender.len(), 1);
    }
}
