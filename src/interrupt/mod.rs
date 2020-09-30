//! Interrupt utilities
//!
//! This module is the only part of this crate that can safely be used
//! between interrupts and applicatin context (or between interrupts with different priorities).

use core::sync::atomic::{AtomicU8, Ordering};
use core::task;
use core::mem::MaybeUninit;
use core::cell::UnsafeCell;

const HAS_WAKER_FLAG: u8 = 0b0000_0001;
const LOCKED_FLAG: u8 = 0b0000_0010;
const WAKE_REQUESTED: u8 = 0b0000_0100;

/// Marker struct for IRQ context functions
pub struct IrqCtx;
/// Marker struct for application context functions
pub struct AppCtx;

/// An interrupt-safe waker
///
/// The waker can be woken up from interrupts and set from application context.
///
/// The waker can only be used with static references.
pub struct Waker {
    waker: UnsafeCell<MaybeUninit<task::Waker>>,
    flags: AtomicU8,
}

unsafe impl Sync for Waker {}

impl Waker {
    unsafe fn take_waker(&self) -> task::Waker {
        let waker = &mut *self.waker.get();
        let waker = core::mem::replace(waker, MaybeUninit::uninit()).assume_init();
        waker
    }

    /// Create a new waker
    pub const fn new() -> Self {
        Self {
            waker: UnsafeCell::new(MaybeUninit::uninit()),
            flags: AtomicU8::new(0),
        }
    }

    /// Wake up the stored waker.
    pub fn wake(&'static self, _ctx: IrqCtx) {
        let flag = self.flags.load(Ordering::Acquire);
        if flag & LOCKED_FLAG == LOCKED_FLAG {
            let new_flags = flag | WAKE_REQUESTED;
            self.flags.store(new_flags, Ordering::Release);
        }
        else if flag & HAS_WAKER_FLAG == HAS_WAKER_FLAG {
            unsafe {
                self.take_waker().wake();
                self.flags.fetch_and(!HAS_WAKER_FLAG, Ordering::AcqRel);
            }
        }
    }

    /// Set a new waker. If the waker is woken up while stored it will be woken up
    /// from this function.
    pub fn set_waker(&'static self, waker: task::Waker, _ctx: AppCtx) {
        let mut flag = self.flags.load(Ordering::Acquire);
        while self.flags.compare_and_swap(flag, flag | LOCKED_FLAG, Ordering::AcqRel) != flag {
            flag = self.flags.load(Ordering::Acquire);
        }
        unsafe {
            // Locked so waker and HAS_WAKER bit will not be modified at the moment
            if flag & HAS_WAKER_FLAG == HAS_WAKER_FLAG {
                self.take_waker();
                let stored_waker = &mut *self.waker.get();
                *stored_waker = MaybeUninit::new(waker.clone());
            }
            else {
                let stored_waker = &mut *self.waker.get();
                *stored_waker = MaybeUninit::new(waker.clone());
                flag = self.flags.fetch_or(HAS_WAKER_FLAG, Ordering::AcqRel) | HAS_WAKER_FLAG;
            }
        }

        flag = flag | LOCKED_FLAG;
        while self.flags.compare_and_swap(flag, flag & !LOCKED_FLAG, Ordering::AcqRel) != flag {
            flag = self.flags.load(Ordering::Acquire);
        }

        if flag & WAKE_REQUESTED == WAKE_REQUESTED {
            waker.wake();
            self.flags.fetch_and(!WAKE_REQUESTED, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn set_waker() {
        static WAKER: Waker = Waker::new();
        assert_eq!(WAKER.flags.load(Ordering::Acquire), 0);
        let mut wake_flag = false;
        WAKER.set_waker(crate::test::get_waker(&mut wake_flag), AppCtx);
        assert_eq!(WAKER.flags.load(Ordering::Acquire), HAS_WAKER_FLAG);
        WAKER.wake(IrqCtx);
        assert_eq!(WAKER.flags.load(Ordering::Acquire), 0);
        assert!(wake_flag);
    }

    #[test]
    fn wake_while_setting() {
        static WAKER: Waker = Waker::new();
        assert_eq!(WAKER.flags.load(Ordering::Acquire), 0);
        WAKER.flags.store(LOCKED_FLAG, Ordering::Release);
        WAKER.wake(IrqCtx);
        let mut wake_flag = false;
        assert!(!wake_flag);
        assert_eq!(WAKER.flags.load(Ordering::Acquire) & WAKE_REQUESTED, WAKE_REQUESTED);
        WAKER.set_waker(crate::test::get_waker(&mut wake_flag), AppCtx);
        assert!(wake_flag);
    }
}
