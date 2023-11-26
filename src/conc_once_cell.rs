use core::cell::SyncUnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{Ordering, AtomicU8};

const GUARD_UNINIT: u8 = 0;
const GUARD_LOCKED: u8 = 1;
const GUARD_INIT: u8 = 2;

pub struct ConcurrentOnceCellNoAlloc<T> {
    inner: SyncUnsafeCell<MaybeUninit<T>>,
    guard: AtomicU8,
}

impl<T> ConcurrentOnceCellNoAlloc<T> {

    pub const fn new() -> Self {
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
        if self.is_init() {
            unsafe { (&mut *self.inner.get()).assume_init_drop(); }
        }
    }
}
