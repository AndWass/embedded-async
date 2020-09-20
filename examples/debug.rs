use embedded_async::channel::{ChannelAnchor, consts};
use core::pin::Pin;

fn main() {
    {
        let mut chan: ChannelAnchor<i32, consts::U8> = ChannelAnchor::new();
        let pin = unsafe { Pin::new_unchecked(&mut chan) };
        let (sender, _receiver) = pin.split().unwrap();
        core::mem::forget(sender);
    }
}