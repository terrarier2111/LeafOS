use std::alloc::{alloc, dealloc, Layout};
use std::{mem, ptr};
use std::ptr::NonNull;

pub struct SizedBox<T> {
    // FIXME: NOTE: this thing could basically be replaced by Box::leak
    alloc_ptr: NonNull<T>,
}

impl<T> SizedBox<T> {
    const LAYOUT: Layout = Layout::new::<T>();

    pub fn new(val: T) -> Self {
        // SAFETY: The layout we provided was checked at compiletime, so it has to be initialized correctly
        let alloc = unsafe { alloc(Self::LAYOUT) }.cast::<T>();
        // FIXME: add safety comment
        unsafe {
            alloc.write(val);
        }
        Self {
            alloc_ptr: NonNull::new(alloc).unwrap(), // FIXME: can we make this unchecked?
        }
    }

    pub fn as_ref(&self) -> &T {
        // SAFETY: This is safe because we know that alloc_ptr can't be zero
        // and because we know that alloc_ptr has to point to a valid
        // instance of T in memory
        unsafe { self.alloc_ptr.as_ptr().as_ref().unwrap_unchecked() }
    }

    pub fn as_mut(&mut self) -> &mut T {
        // SAFETY: This is safe because we know that alloc_ptr can't be zero
        // and because we know that alloc_ptr has to point to a valid
        // instance of T in memory
        unsafe { self.alloc_ptr.as_ptr().as_mut().unwrap_unchecked() }
    }

    pub fn into_ptr(self) -> NonNull<T> {
        let ret = self.alloc_ptr;
        mem::forget(self);
        ret
    }

    pub fn into_inner(self) -> T {
        let ret = unsafe { self.alloc_ptr.as_ptr().read() };
        mem::forget(self);
        ret
    }

}

impl<T> Drop for SizedBox<T> {
    fn drop(&mut self) {
        // SAFETY: This is safe to call because SizedBox can only be dropped once
        unsafe {
            ptr::drop_in_place(self.alloc_ptr.as_ptr());
        }
        // FIXME: add safety comment
        unsafe {
            dealloc(self.alloc_ptr.as_ptr().cast::<u8>(), SizedBox::<T>::LAYOUT);
        }
    }
}
