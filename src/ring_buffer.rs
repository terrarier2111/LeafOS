use core::mem::MaybeUninit;

pub struct RingBuffer<T, const N: usize, const CHECKED: bool> {
    buffer: [MaybeUninit<T>; N],
    offset: usize, // offset for the next write
}

impl<T, const N: usize, const CHECKED: bool> RingBuffer<T, N, CHECKED> {

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn length(&self) -> usize {
        self.offset
    }

}

impl<T, const N: usize> RingBuffer<T, N, true> {

    /// Returns whether the write was successful or not
    pub fn push(&mut self, elem: T) -> bool {
        if self.length() >= self.capacity() {
            return false;
        }

        true
    }

}

impl<T, const N: usize> RingBuffer<T, N, false> {

    pub fn push(&mut self, elem: T) {

    }

}