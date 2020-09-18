use core::pin::Pin;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::task::{Context, Poll};

struct MutexWaiter {
    waker: MaybeUninit<core::task::Waker>,
}

impl Default for MutexWaiter {
    fn default() -> Self {
        Self {
            waker: MaybeUninit::uninit(),
        }
    }
}

/// This is the data-holder half of the async mutex. This must be kept alive
/// for as long as there are references alive.
///
/// Note that the mutex is not safe to use across threads, it must only be used
/// from a single thread to synchronize tasks on that thread.
///
/// ## Safety
///
/// Dropping a `MutexAnchor` while there are still `MutexRef` instances to this anchor
/// alive will cause a panic.
pub struct MutexAnchor<T> {
    locked: bool,
    value: T,
    rc_pin: crate::intrusive::rc::RcAnchor<()>,
    waiting_wakers: crate::intrusive::slist::List<MutexWaiter>,
}

impl<T> MutexAnchor<T> {
    fn release_lock(&mut self) {
        self.locked = false;

        let mut waiting = self.waiting_wakers.take();
        while let Some(x) = waiting.pop() {
            x.owner().and_then(|x| {
                unsafe { &*x.waker.as_ptr() }.wake_by_ref();
                Some(())
            });
        }
    }

    /// Create a new mutex anchor with a starting value
    ///
    /// ## Examples
    ///
    /// ```
    /// # use embedded_async::sync::MutexAnchor;
    /// let mutex = MutexAnchor::new(0);
    /// ```
    pub fn new(value: T) -> Self {
        Self {
            locked: false,
            value,
            rc_pin: crate::intrusive::rc::RcAnchor::new(()),
            waiting_wakers: crate::intrusive::slist::List::new(),
        }
    }

    /// Take a reference to the mutex.
    ///
    /// ## Examples
    ///
    /// ```
    /// use embedded_async::sync::MutexAnchor;
    /// let mutex = MutexAnchor::new(0);
    /// pin_utils::pin_mut!(mutex); // Pin the mutex anchor in place
    /// let mutex_ref = mutex.take_ref();
    /// ```
    pub fn take_ref(self: Pin<&mut Self>) -> MutexRef<T> {
        let ptr = unsafe { self.get_unchecked_mut() };
        let ret_ptr = ptr as *mut Self;
        let rc_ref_pin = unsafe { Pin::new_unchecked(&mut ptr.rc_pin) };

        MutexRef {
            mutex: ret_ptr,
            rc_ref: rc_ref_pin.take(),
        }
    }
}

/// Reference type that refers to a pinned `MutexAnchor`.
///
/// MutexRef is ref-counted and cloneable. With these you lock the mutex and
/// get access to the stored data.
#[derive(Clone)]
pub struct MutexRef<T> {
    mutex: *mut MutexAnchor<T>,
    rc_ref: crate::intrusive::rc::RcRef<()>,
}

impl<T> MutexRef<T> {
    fn mutex_mut(&self) -> &mut MutexAnchor<T> {
        unsafe { &mut *self.mutex }
    }

    /// Acquires the mutex.
    ///
    /// Returns a guard that releases the mutex when dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// # smol::block_on(async {
    /// use embedded_async::sync::MutexAnchor;
    ///
    /// let mutex = MutexAnchor::new(10);
    /// pin_utils::pin_mut!(mutex);
    /// let mutex = mutex.take_ref();
    /// let guard = mutex.lock().await;
    /// assert_eq!(*guard, 10);
    /// # })
    pub async fn lock(&self) -> MutexGuard<T> {
        if !self.mutex_mut().locked {
            self.mutex_mut().locked = true;
            MutexGuard {
                mutex: self.mutex,
                rc_ref: crate::intrusive::rc::RcRef::clone(&self.rc_ref),
            }
        } else {
            LockFuture {
                link: crate::intrusive::slist::Link::new(),
                waiter: MutexWaiter::default(),
                mutex: self.mutex,
            }
            .await;
            // We have obtained the lock
            MutexGuard {
                mutex: self.mutex,
                rc_ref: crate::intrusive::rc::RcRef::clone(&self.rc_ref),
            }
        }
    }

    /// Attempts to acquire the mutex. Returns `None` if the mutex couldn't be acquired. Otherwise
    /// a guard is returned that releases the mutex when dropped.
    ///
    /// ## Examples
    ///
    /// ```
    /// use embedded_async::sync::MutexAnchor;
    ///
    /// let mutex = MutexAnchor::new(10);
    /// pin_utils::pin_mut!(mutex);
    /// let mutex = mutex.take_ref();
    /// let guard = mutex.try_lock().unwrap();
    /// assert_eq!(*guard, 10);
    /// ```
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        if !self.mutex_mut().locked {
            self.mutex_mut().locked = true;
            Some(MutexGuard {
                mutex: self.mutex,
                rc_ref: crate::intrusive::rc::RcRef::clone(&self.rc_ref),
            })
        } else {
            None
        }
    }
}

/// A guard that releases the mutex when dropped.
pub struct MutexGuard<T> {
    mutex: *mut MutexAnchor<T>,
    rc_ref: crate::intrusive::rc::RcRef<()>,
}

impl<T> MutexGuard<T> {
    /// Manually unlock the mutex by consuming the guard.
    ///
    /// ## Examples
    ///
    /// ```
    /// use embedded_async::sync::MutexAnchor;
    /// let mutex = MutexAnchor::new(10);
    /// pin_utils::pin_mut!(mutex);
    /// let mutex = mutex.take_ref();
    /// let lock = mutex.try_lock().unwrap();
    /// lock.unlock();
    /// ```
    pub fn unlock(self) {
        // Drop code will ensure that the mutex is mutex is unlocked.
    }

    /// Get a `MutexRef<T>` from this MutexGuard. The reference will refer to the same mutex
    /// but will not be lockable since the mutex is locked by the guard.
    ///
    /// ## Examples
    /// ```
    /// use embedded_async::sync::MutexAnchor;
    /// let mutex = MutexAnchor::new(10);
    /// pin_utils::pin_mut!(mutex);
    /// let mutex = mutex.take_ref();
    /// let lock = mutex.try_lock().unwrap();
    /// {
    ///     let mutex_ref2 = lock.mutex_ref();
    ///     assert!(mutex_ref2.try_lock().is_none());
    ///     lock.unlock();
    ///     assert!(mutex_ref2.try_lock().is_some());
    /// }
    /// ```
    pub fn mutex_ref(&self) -> MutexRef<T> {
        MutexRef {
            mutex: self.mutex,
            rc_ref: crate::intrusive::rc::RcRef::clone(&self.rc_ref),
        }
    }
}

impl<T> Deref for MutexGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &unsafe { &*self.mutex }.value
    }
}

impl<T> DerefMut for MutexGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut unsafe { &mut *self.mutex }.value
    }
}

impl<T> Drop for MutexGuard<T> {
    fn drop(&mut self) {
        unsafe { &mut *self.mutex }.release_lock();
    }
}

struct LockFuture<T> {
    link: crate::intrusive::slist::Link<MutexWaiter>,
    waiter: MutexWaiter,
    mutex: *mut MutexAnchor<T>,
}

impl<T> core::future::Future for LockFuture<T> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mutex = unsafe { &mut *self.mutex };
        if mutex.locked {
            self.waiter.waker = MaybeUninit::new(cx.waker().clone());
            let link = &mut self.link as *mut _;
            let waiter = &mut self.waiter;
            unsafe { mutex.waiting_wakers.push(waiter, link) };
            Poll::Pending
        } else {
            mutex.locked = true;
            Poll::Ready(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pin_utils::core_reexport::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    #[test]
    fn new_mutex_unlocked() {
        let mutex = MutexAnchor::new(10);
        assert_eq!(mutex.locked, false);
        assert_eq!(mutex.value, 10);
    }

    #[test]
    fn unlock_works() {
        let mutex = MutexAnchor::new(10);
        pin_utils::pin_mut!(mutex);
        let mutex = mutex.take_ref();
        {
            assert!(!mutex.mutex_mut().locked);
            let lock = mutex.try_lock().unwrap();
            assert!(mutex.mutex_mut().locked);
            lock.unlock();
            assert!(!mutex.mutex_mut().locked);
        }
    }

    #[test]
    fn lock_blocks() {
        smol::block_on(async {
            let mutex = MutexAnchor::new(10);
            pin_utils::pin_mut!(mutex);
            let mptr = unsafe { mutex.as_mut().get_unchecked_mut() as *mut MutexAnchor<i32> };
            let mref = mutex.take_ref();

            let mptr = unsafe { &mut *mptr };

            let lock = mref.lock().await;
            let mref2 = mref.clone();
            static STARTED: AtomicBool = AtomicBool::new(false);
            let flag = &STARTED;

            let executor = smol::LocalExecutor::new();

            let _task = executor.spawn(async move {
                flag.store(true, Ordering::Release);
                let mut lock = mref.lock().await;
                *lock = 40;
            });

            assert!(mptr.waiting_wakers.is_empty());
            assert!(executor.try_tick());
            assert!(STARTED.load(Ordering::Acquire));
            assert!(!mptr.waiting_wakers.is_empty());
            assert_eq!(*lock, 10);
            lock.unlock();
            assert!(mptr.waiting_wakers.is_empty());
            executor.tick().await;
            let lock = mref2.try_lock();
            assert!(lock.is_some());
            assert_eq!(*lock.unwrap(), 40);
        });
    }
}
