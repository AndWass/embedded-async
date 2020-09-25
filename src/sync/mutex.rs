use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::intrusive::double_list::*;

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

struct Inner<T> {
    locked: bool,
    value: T,
    // The anchor will be initialized to self when
    // the reference is taken. It will be safe to derefence this
    // pointer as long as the the mutex isn't moved/dropped.
    rc: crate::intrusive::rc::RcAnchor<*const Mutex<T>>,
    waiting_wakers: List<MutexWaiter>,
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
pub struct Mutex<T> {
    inner: core::cell::UnsafeCell<Inner<T>>,
}

impl<T> Mutex<T> {
    fn inner(&self) -> &Inner<T> {
        unsafe { &*self.inner.get() }
    }

    fn inner_mut(&self) -> &mut Inner<T> {
        unsafe { &mut *self.inner.get() }
    }

    fn maybe_init_inner(&self) {
        if (*self.inner().rc).is_null() {
            self.inner_mut().rc = crate::intrusive::rc::RcAnchor::new(self);
        }
    }

    fn release_lock(&self) {
        self.inner_mut().locked = false;
        let mut waiting = List::<MutexWaiter>::new();
        self.inner_mut().waiting_wakers.move_to(&mut waiting);
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
    /// # use embedded_async::sync::Mutex;
    /// let mutex = Mutex::new(0);
    /// ```
    pub const fn new(value: T) -> Self {
        Self {
            inner: core::cell::UnsafeCell::new(Inner {
                locked: false,
                value,
                rc: crate::intrusive::rc::RcAnchor::new(core::ptr::null()),
                waiting_wakers: List::new(),
            }),
        }
    }

    /// Take a reference to the mutex. This requires the mutex to be pinned,
    /// and will consume the pin.
    ///
    /// ## Examples
    ///
    /// ```
    /// use embedded_async::sync::Mutex;
    /// let mutex = Mutex::new(0);
    /// pin_utils::pin_mut!(mutex); // Pin the mutex anchor in place
    /// let mutex_ref = mutex.take_ref();
    /// ```
    pub fn take_ref(self: Pin<&mut Self>) -> MutexRef<T> {
        let self_ref = unsafe { self.get_unchecked_mut() };

        self_ref.maybe_init_inner();

        MutexRef {
            rc_ref: unsafe { self_ref.inner_mut().rc.get_ref() },
        }
    }

    unsafe fn try_lock_impl(&self) -> Option<MutexGuard<T>> {
        if !&self.inner().locked {
            self.inner_mut().locked = true;
            Some(MutexGuard {
                rc_ref: self.inner_mut().rc.get_ref(),
            })
        } else {
            None
        }
    }

    unsafe fn lock_impl(&self) -> LockFuture<T> {
        LockFuture {
            link: Link::new(),
            waiter: MutexWaiter::default(),
            rc_ref: self.inner_mut().rc.get_ref(),
        }
    }
}

/// Reference type that refers to a pinned `MutexAnchor`.
///
/// MutexRef is ref-counted and cloneable. With these you lock the mutex and
/// get access to the stored data.
#[derive(Clone)]
pub struct MutexRef<T> {
    rc_ref: crate::intrusive::rc::RcRef<*const Mutex<T>>,
}

impl<T> MutexRef<T> {
    fn mutex(&self) -> &Mutex<T> {
        unsafe { &**self.rc_ref }
    }

    /// Acquires the mutex.
    ///
    /// Returns a guard that releases the mutex when dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// # smol::block_on(async {
    /// use embedded_async::sync::Mutex;
    ///
    /// let mutex = Mutex::new(10);
    /// pin_utils::pin_mut!(mutex);
    /// let mutex = mutex.take_ref();
    /// let guard = mutex.lock().await;
    /// assert_eq!(*guard, 10);
    /// # })
    pub fn lock(&self) -> LockFuture<T> {
        unsafe { self.mutex().lock_impl() }
    }

    /// Attempts to acquire the mutex. Returns `None` if the mutex couldn't be acquired. Otherwise
    /// a guard is returned that releases the mutex when dropped.
    ///
    /// ## Examples
    ///
    /// ```
    /// use embedded_async::sync::Mutex;
    ///
    /// let mutex = Mutex::new(10);
    /// pin_utils::pin_mut!(mutex);
    /// let mutex = mutex.take_ref();
    /// let guard = mutex.try_lock().unwrap();
    /// assert_eq!(*guard, 10);
    /// ```
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        unsafe { self.mutex().try_lock_impl() }
    }
}

/// A guard that releases the mutex when dropped.
pub struct MutexGuard<T> {
    rc_ref: crate::intrusive::rc::RcRef<*const Mutex<T>>,
}

impl<T> MutexGuard<T> {
    /// Manually unlock the mutex by consuming the guard.
    ///
    /// ## Examples
    ///
    /// ```
    /// use embedded_async::sync::Mutex;
    /// let mutex = Mutex::new(10);
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
    /// use embedded_async::sync::Mutex;
    /// let mutex = Mutex::new(10);
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
            rc_ref: crate::intrusive::rc::RcRef::clone(&self.rc_ref),
        }
    }
}

impl<T> Deref for MutexGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &unsafe { &**self.rc_ref }.inner().value
    }
}

impl<T> DerefMut for MutexGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut unsafe { &**self.rc_ref }.inner_mut().value
    }
}

impl<T> Drop for MutexGuard<T> {
    fn drop(&mut self) {
        unsafe {
            (&**self.rc_ref).release_lock();
        }
    }
}

pub struct LockFuture<T> {
    link: Link<MutexWaiter>,
    waiter: MutexWaiter,
    rc_ref: crate::intrusive::rc::RcRef<*const Mutex<T>>,
}

impl<T> core::future::Future for LockFuture<T> {
    type Output = MutexGuard<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mutex = unsafe { &**self.rc_ref };
        if mutex.inner().locked {
            self.waiter.waker = MaybeUninit::new(cx.waker().clone());
            let link = &mut self.link as *mut _;
            let waiter = &mut self.waiter;
            unsafe { mutex.inner_mut().waiting_wakers.push_link(waiter, link) };
            Poll::Pending
        } else {
            mutex.inner_mut().locked = true;
            Poll::Ready(MutexGuard {
                rc_ref: crate::intrusive::rc::RcRef::clone(&self.rc_ref),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::Ordering;
    use pin_utils::core_reexport::sync::atomic::AtomicBool;

    #[test]
    fn new_mutex_unlocked() {
        let mutex = Mutex::new(10);
        assert_eq!(mutex.inner().locked, false);
        assert_eq!(mutex.inner().value, 10);
    }

    #[test]
    fn unlock_works() {
        let mutex = Mutex::new(10);
        pin_utils::pin_mut!(mutex);
        let mutex = mutex.take_ref();
        {
            assert!(!mutex.mutex().inner().locked);
            let lock = mutex.try_lock().unwrap();
            assert!(mutex.mutex().inner().locked);
            lock.unlock();
            assert!(!mutex.mutex().inner().locked);
        }
    }

    #[test]
    fn lock_blocks() {
        smol::block_on(async {
            let mutex = Mutex::new(10);
            pin_utils::pin_mut!(mutex);
            let mptr = unsafe { mutex.as_mut().get_unchecked_mut() as *mut Mutex<i32> };
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

            assert!(mptr.inner().waiting_wakers.is_empty());
            assert!(executor.try_tick());
            assert!(STARTED.load(Ordering::Acquire));
            assert!(!mptr.inner().waiting_wakers.is_empty());
            assert_eq!(*lock, 10);
            lock.unlock();
            assert!(mptr.inner().waiting_wakers.is_empty());
            executor.tick().await;
            let lock = mref2.try_lock();
            assert!(lock.is_some());
            assert_eq!(*lock.unwrap(), 40);
        });
    }
}
