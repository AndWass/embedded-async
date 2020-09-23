use core::pin::Pin;
use embedded_async::channel::{consts, Channel};

fn main() {
    {
        let mut chan: Channel<i32, consts::U8> = Channel::new();
        let pin = unsafe { Pin::new_unchecked(&mut chan) };
        let (sender, _receiver) = pin.split().unwrap();
        core::mem::forget(sender);
    }
}
