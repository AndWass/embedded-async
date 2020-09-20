use crate::intrusive::{rc};

pub use heapless::consts as consts;

use core::mem::MaybeUninit;
use core::pin::Pin;

#[derive(PartialOrd, PartialEq,Debug)]
pub enum TryRecvError {
    Empty,
    Closed,
}

#[derive(PartialOrd, PartialEq, Debug)]
pub enum TrySendError<T> {
    Full(T),
    Closed(T),
}

struct WaitingReceiver {
    waker: core::task::Waker,
}

trait ChannelOps<T: 'static> {
    fn try_push(&mut self, value: T) -> Result<(), TrySendError<T>>;
    fn try_pop(&mut self) -> Result<T, TryRecvError>;

    fn len(&self) -> usize;
}

pub struct ChannelAnchor<T: 'static, L: 'static + heapless::ArrayLength<T>> {
    queue: heapless::spsc::Queue<T, L>,
    waiting_receivers: crate::intrusive::slist::List<WaitingReceiver>,
    tx_rx_rc: Option<(rc::RcAnchor<*mut dyn ChannelOps<T>>, rc::RcAnchor<*mut dyn ChannelOps<T>>)>,
}

impl<T: 'static, L: 'static + heapless::ArrayLength<T>> ChannelAnchor<T, L>
{
    fn receiver_ref_count(&self) -> u32 {
        if let Some((_, rx)) = &self.tx_rx_rc {
            rx.ref_count()
        }
        else {
            unreachable!("");
        }
    }

    fn sender_ref_count(&self) -> u32 {
        if let Some((tx, _)) = &self.tx_rx_rc {
            tx.ref_count()
        }
        else {
            unreachable!("");
        }
    }

    pub fn new() -> Self {
        Self {
            queue: heapless::spsc::Queue::new(),
            waiting_receivers: crate::intrusive::slist::List::new(),
            tx_rx_rc: None,
        }
    }

    pub fn split(self: Pin<&mut Self>) -> Option<(Sender<T>, Receiver<T>)> {
        if self.tx_rx_rc.is_some() {
            None
        }
        else {
            use crate::intrusive::rc::RcAnchor;
            let me = unsafe { self.get_unchecked_mut() };
            let me_ptr = me as *mut Self;
            me.tx_rx_rc = Some(
                (RcAnchor::new(me_ptr as _), RcAnchor::new(me_ptr as _))
            );

            let (tx, rx) = me.tx_rx_rc.as_mut().expect("");

            Some((Sender {
                channel: unsafe { tx.get_ref() },
            },
             Receiver {
                 channel: unsafe { rx.get_ref() }
             }))
        }
    }
}


impl<T, L: heapless::ArrayLength<T>> ChannelOps<T> for ChannelAnchor<T, L> {
    fn try_push(&mut self, value: T) -> Result<(), TrySendError<T>> {
        if self.receiver_ref_count() == 0 {
            Err(TrySendError::Closed(value))
        }
        else {
            match self.queue.enqueue(value)
            {
                Ok(_) => {
                    let mut waiting_receivers = self.waiting_receivers.take();
                    while let Some(r) = waiting_receivers.pop() {
                        if let Some(w) = r.owner() {
                            w.waker.wake_by_ref();
                        }
                    }

                    Ok(())
                },
                Err(v) => Err(TrySendError::Full(v)),
            }
        }
    }

    fn try_pop(&mut self) -> Result<T, TryRecvError> {
        if self.queue.is_empty() && self.sender_ref_count() == 0 {
            Err(TryRecvError::Closed)
        }
        else {
            match self.queue.dequeue() {
                Some(x) => Ok(x),
                None => Err(TryRecvError::Empty)
            }
        }
    }

    fn len(&self) -> usize {
        self.queue.len()
    }
}

pub struct Sender<T: 'static> {
    channel: rc::RcRef<*mut dyn ChannelOps<T>>,
}

impl<T: 'static> Sender<T> {
    fn mut_channel(&self) -> &mut dyn ChannelOps<T> {
        unsafe { &mut **self.channel }
    }
    fn channel(&self) -> &dyn ChannelOps<T> {
        unsafe { & **self.channel }
    }

    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.mut_channel().try_push(value)
    }

    pub fn len(&self) -> usize {
        self.channel().len()
    }
}

pub struct Receiver<T: 'static> {
    channel: rc::RcRef<*mut dyn ChannelOps<T>>,
}

impl<T: 'static> Receiver<T> {
    fn mut_channel(&self) -> &mut dyn ChannelOps<T> {
        unsafe { &mut **self.channel }
    }
    fn channel(&self) -> &dyn ChannelOps<T> {
        unsafe { & **self.channel }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.mut_channel().try_pop()
    }

    pub fn len(&self) -> usize {
        self.channel().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sets_ints_refs() {
        let mut chan: ChannelAnchor<i32, consts::U8> = ChannelAnchor::new();
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
            let mut chan: ChannelAnchor<i32, consts::U8> = ChannelAnchor::new();
            let pin = unsafe { Pin::new_unchecked(&mut chan) };
            let (sender, _receiver) = pin.split().unwrap();
            core::mem::forget(sender);
        }
    }

    #[test]
    #[should_panic]
    fn forgetting_receiver_panics() {
        {
            let mut chan: ChannelAnchor<i32, consts::U8> = ChannelAnchor::new();
            let pin = unsafe { Pin::new_unchecked(&mut chan) };
            let (_sender, receiver) = pin.split().unwrap();
            core::mem::forget(receiver);
        }
    }

    #[test]
    fn send_receive_one() {
        let chan: ChannelAnchor<i32, consts::U8> = ChannelAnchor::new();
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
        let chan: ChannelAnchor<i32, consts::U8> = ChannelAnchor::new();
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
        let chan: ChannelAnchor<i32, consts::U8> = ChannelAnchor::new();
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


