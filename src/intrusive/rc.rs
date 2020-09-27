use core::cell::UnsafeCell;
use core::ops::Deref;
use core::pin::Pin;

struct Inner<T> {
    ref_count: u32,
    data: T,
}

pub struct RcAnchor<T> {
    inner: UnsafeCell<Inner<T>>,
}

impl<T> RcAnchor<T> {
    fn increment_refcount(&self) {
        unsafe { &mut *self.inner.get() }.ref_count += 1;
    }

    fn decrement_refcount(&self) {
        unsafe { &mut *self.inner.get() }.ref_count -= 1;
    }

    fn data(&self) -> &T {
        &unsafe { &*self.inner.get() }.data
    }

    pub const fn new(value: T) -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                ref_count: 0,
                data: value,
            }),
        }
    }

    #[allow(dead_code)]
    pub fn take(self: Pin<&mut Self>) -> RcRef<T> {
        let ptr = unsafe { self.get_unchecked_mut() };
        ptr.increment_refcount();
        RcRef { pin: ptr as *mut _ }
    }

    pub unsafe fn get_ref(&mut self) -> RcRef<T> {
        self.increment_refcount();
        RcRef {
            pin: self as *mut _,
        }
    }

    pub fn ref_count(&self) -> u32 {
        unsafe { &*self.inner.get() }.ref_count
    }
}

impl<T> Drop for RcAnchor<T> {
    fn drop(&mut self) {
        if self.ref_count() != 0 {
            panic!("RcPin dropped with non-zero refcount");
        }
    }
}

impl<T> Deref for RcAnchor<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &unsafe { &*self.inner.get() }.data
    }
}

pub struct RcRef<T> {
    pin: *const RcAnchor<T>,
}

impl<T> RcRef<T> {
    pub fn clone(from: &RcRef<T>) -> Self {
        unsafe { &*from.pin }.increment_refcount();
        Self { pin: from.pin }
    }

    pub fn ref_count(&self) -> u32 {
        unsafe { &*self.pin }.ref_count()
    }
}

impl<T> Clone for RcRef<T> {
    fn clone(&self) -> Self {
        unsafe { &*self.pin }.increment_refcount();
        Self { pin: self.pin }
    }
}

impl<T> Drop for RcRef<T> {
    fn drop(&mut self) {
        unsafe { &*self.pin }.decrement_refcount();
    }
}

impl<T> Deref for RcRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.pin }.data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_count_updates() {
        let mut rc = RcAnchor::new(10);
        assert_eq!(rc.ref_count(), 0);
        let rc_ref = unsafe { Pin::new_unchecked(&mut rc) }.take();
        assert_eq!(rc.ref_count(), 1);
        let _rc_ref2 = RcRef::clone(&rc_ref);
        assert_eq!(rc.ref_count(), 2);
        {
            let _rc_ref3 = RcRef::clone(&rc_ref);
            assert_eq!(rc.ref_count(), 3);
        }
        assert_eq!(rc.ref_count(), 2);
        assert_eq!(*rc_ref, 10);
    }

    #[test]
    #[should_panic]
    fn panic_on_forget() {
        let mut rc = RcAnchor::new(10);
        assert_eq!(rc.ref_count(), 0);
        let rc_ref = unsafe { Pin::new_unchecked(&mut rc) }.take();
        assert_eq!(rc.ref_count(), 1);
        core::mem::forget(rc_ref);
        assert_eq!(rc.ref_count(), 1);
    }
}
