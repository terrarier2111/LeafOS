use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU64, Ordering};

/// Returns the index where the target element is.
///
/// Returns the next smaller index on failure.
pub fn search_length_limited_nearest<T: PartialOrd>(container: &[T], target: T, length: usize) -> usize {
    let mut curr_pos = length / 2;
    let mut step_size = length / 4;
    let mut adapted = false;
    loop {
        if container[curr_pos] > target {
            if step_size == 0 {
                if !adapted && curr_pos != 0 && curr_pos != length - 1 {
                    adapted = true;
                    step_size += 1;
                } else {
                    if container[curr_pos] > target {
                        return curr_pos - 1;
                    }
                    return curr_pos;
                }
            }
            curr_pos -= step_size;
            step_size /= 2;
        } else if container[curr_pos] < target {
            if step_size == 0 {
                if !adapted && curr_pos != 0 && curr_pos != length - 1 {
                    adapted = true;
                    step_size += 1;
                } else {
                    if container[curr_pos] > target {
                        return curr_pos - 1;
                    }
                    return curr_pos;
                }
            }
            curr_pos += step_size;
            step_size /= 2;
        } else {
            return curr_pos;
        }
    }
}

// core::intrinsics::atomic_store_seqcst()
/*
pub struct AtomicInitCell<T: Default + Copy, A: AtomicallyLoadStore<T>> {
    inner: AtomicInit<T, A>,
}

impl<T: Default + Copy, A: AtomicallyLoadStore<T>> Default for AtomicInitCell<T, A> {
    fn default() -> Self {
        Self {
            inner: AtomicInit::Uninit(A::default()),
        }
    }
}

impl<T: Default + Copy, A: AtomicallyLoadStore<T>> AtomicInitCell<T, A> {

    pub fn try_init(&mut self, val: T) -> bool {
        if let AtomicInit::Uninit(atomic) = &self.inner {
            atomic.store(val);
            self.inner = AtomicInit::Init(atomic.load());
            true
        } else {
            false
        }
    }

    pub fn try_load(&self) -> Option<T> {
        if let AtomicInit::Init(val) = &self.inner {
            Some(*val)
        } else {
            None
        }
    }

}

enum AtomicInit<T: Default, A: AtomicallyLoadStore<T>> {
    Uninit(A),
    Init(T),
}

pub trait AtomicallyLoadStore<T>: Default {

    fn load(&self) -> T;

    fn store(&self, val: T);

}

impl AtomicallyLoadStore<u64> for AtomicU64 {
    fn load(&self) -> u64 {
        self.load(Ordering::Relaxed)
    }

    fn store(&self, val: u64) {
        self.store(val, Ordering::SeqCst);
    }
}*/

/*pub struct AtomicInitCell<T: Default> {
    val: T,
}

impl<T: Default> Default for AtomicInitCell<T> {
    fn default() -> Self {
        Self {
            val: T::default(),
        }
    }
}

impl<T: Default> AtomicInitCell<T> {

    pub unsafe fn init(&mut self, val: T) {
        core::intrinsics::atomic_store_seqcst(addr_of_mut!(self.val), val);
    }

    #[inline]
    pub fn get(&self) -> T {
        *self.val
    }

}*/
