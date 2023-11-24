use core::cell::SyncUnsafeCell;
use core::mem::{self, MaybeUninit};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicPtr, Ordering, AtomicU8};

pub struct ConcurrentOnceCell<T> {
    ptr: AtomicPtr<T>,
}

impl<T> ConcurrentOnceCell<T> {

    pub fn new() -> Self {
        Self {
            ptr: AtomicPtr::new(null_mut()),
        }
    }

    pub fn is_init(&self) -> bool {
        !self.ptr.load(Ordering::Acquire).is_null()
    }

    pub fn try_init(&self, val: T) -> Result<(), T> {
        let mut sized = crate::sized_box::SizedBox::new(val);
        let ptr = sized.as_mut() as *mut T;
        match self.ptr.compare_exchange(null_mut(), ptr, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => {
                mem::forget(sized);
                Ok(())
            }
            Err(_) => {
                Err(sized.into_inner())
            }
        }
    }

    pub fn get(&self) -> Option<&T> {
        unsafe { self.ptr.load(Ordering::Acquire).as_ref() }
    }

}

impl<T> Drop for ConcurrentOnceCell<T> {
    fn drop(&mut self) {
        let ptr = *self.ptr.get_mut();
        if !ptr.is_null() {
            unsafe { ptr.drop_in_place(); }
        }
    }
}

const GUARD_UNINIT: u8 = 0;
const GUARD_LOCKED: u8 = 1;
const GUARD_INIT: u8 = 2;

pub struct ConcurrentOnceCellNoAlloc<T> {
    inner: SyncUnsafeCell<MaybeUninit<T>>,
    guard: AtomicU8,
}

impl<T> ConcurrentOnceCellNoAlloc<T> {

    pub fn new() -> Self {
        Self {
            inner: SyncUnsafeCell::new(MaybeUninit::uninit()),
            guard: AtomicU8::new(GUARD_UNINIT),
        }
    }

    #[inline]
    pub fn is_init(&self) -> bool {
        self.guard.load(Ordering::Acquire) == GUARD_INIT
    }

    pub fn try_init(&self, val: T) -> Result<(), T> {
        match self.guard.compare_exchange(GUARD_UNINIT, GUARD_LOCKED, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => {
                unsafe { &mut *self.inner.get() }.write(val);
                self.guard.store(GUARD_INIT, Ordering::Release);
                Ok(())
            }
            Err(_) => Err(val),
        }
    }

    #[inline]
    pub fn get(&self) -> Option<&T> {
        if !self.is_init() {
            return None;
        }
        Some(unsafe { self.inner.get().as_ref().unwrap_unchecked().assume_init_ref() })
    }

    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        let mut guard = self.guard.load(Ordering::Acquire);
        if guard == GUARD_UNINIT {
            self.try_init(f());
        }
        while guard != GUARD_INIT {
            core::hint::spin_loop();
            guard = self.guard.load(Ordering::Acquire);
        }
        unsafe { self.get().unwrap_unchecked() }
    }

}

impl<T> Drop for ConcurrentOnceCellNoAlloc<T> {
    fn drop(&mut self) {
        let ptr = *self.ptr.get_mut();
        if !ptr.is_null() {
            unsafe { ptr.drop_in_place(); }
        }
    }
}
