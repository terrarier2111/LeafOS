use core::cell::SyncUnsafeCell;

/// This represents a single core Cell (thats still allowed in statics).
/// It can be set as long as there is only a single core but may not be mutated
/// thereafter.
pub struct SCCell<T: Copy + Send + Sync>(SyncUnsafeCell<T>);

impl<T: Copy + Send + Sync> SCCell<T> {

    #[inline]
    pub const fn new(val: T) -> Self {
        Self(SyncUnsafeCell::new(val))
    }

    #[inline]
    pub const unsafe fn set(&self, val: T) {
        unsafe { *self.0.get() = val; }
    }

    #[inline]
    pub const fn get(&self) -> T {
        unsafe { *self.0.get() }
    }

}