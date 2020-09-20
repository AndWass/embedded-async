use core::pin::Pin;
use std::ops::Deref;

pub struct RcAnchor<T> {
    ref_count: u32,
    data: T,
}

impl<T> RcAnchor<T> {
    pub fn new(value: T) -> Self {
        Self {
            ref_count: 0,
            data: value,
        }
    }

    pub fn take(self: Pin<&mut Self>) -> RcRef<T> {
        let ptr = unsafe { self.get_unchecked_mut() };
        ptr.ref_count = 1;
        RcRef { pin: ptr as *mut _ }
    }

    pub unsafe fn get_ref(&mut self) -> RcRef<T> {
        self.ref_count += 1;
        RcRef { pin: self as *mut _ }
    }

    pub fn ref_count(&self) -> u32 {
        self.ref_count
    }
}

impl<T> Drop for RcAnchor<T> {
    fn drop(&mut self) {
        if self.ref_count != 0 {
            panic!("RcPin dropped with non-zero refcount");
        }
    }
}

pub struct RcRef<T> {
    pin: *mut RcAnchor<T>,
}

impl<T> RcRef<T> {
    pub fn clone(from: &RcRef<T>) -> Self {
        unsafe { &mut *from.pin }.ref_count += 1;
        Self { pin: from.pin }
    }
}

impl<T> Clone for RcRef<T> {
    fn clone(&self) -> Self {
        unsafe { &mut *self.pin }.ref_count += 1;
        Self { pin: self.pin }
    }
}

impl<T> Drop for RcRef<T> {
    fn drop(&mut self) {
        unsafe { &mut *self.pin }.ref_count -= 1;
    }
}

impl<T> Deref for RcRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &unsafe { &*self.pin }.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_count_updates() {
        let mut rc = RcAnchor::new(10);
        assert_eq!(rc.ref_count, 0);
        let rc_ref = unsafe { Pin::new_unchecked(&mut rc) }.take();
        assert_eq!(rc.ref_count, 1);
        let _rc_ref2 = RcRef::clone(&rc_ref);
        assert_eq!(rc.ref_count, 2);
        {
            let _rc_ref3 = RcRef::clone(&rc_ref);
            assert_eq!(rc.ref_count, 3);
        }
        assert_eq!(rc.ref_count, 2);
        assert_eq!(*rc_ref, 10);
    }

    #[test]
    #[should_panic]
    fn panic_on_forget() {
        let mut rc = RcAnchor::new(10);
        assert_eq!(rc.ref_count, 0);
        let rc_ref = unsafe { Pin::new_unchecked(&mut rc) }.take();
        assert_eq!(rc.ref_count, 1);
        core::mem::forget(rc_ref);
        assert_eq!(rc.ref_count, 1);
    }
}
