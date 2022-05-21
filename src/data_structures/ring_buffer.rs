use core::mem::MaybeUninit;

pub struct RingBuffer<T, const N: usize, const CHECKED: bool> {
    buffer: [MaybeUninit<T>; N],
    offset: usize, // offset for the next write
}

impl<T, const N: usize, const CHECKED: bool> RingBuffer<T, N, CHECKED> {

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        N
    }

    #[inline]
    pub fn length(&self) -> usize {
        self.offset
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.offset == 0
    }

}

impl<T, const N: usize> RingBuffer<T, N, true> {

    /// Returns whether the write was successful or not
    pub fn push(&mut self, elem: T) -> bool {
        if self.length() >= self.capacity() {
            return false;
        }
        self.buffer[self.offset].write(elem);
        self.offset += 1;
        true
    }

    pub fn pop(&mut self) -> bool {
        if self.length() <= 0 {
            return false;
        }

        self.offset -= 1;
        true
    }

}

impl<T, const N: usize> RingBuffer<T, N, false> {

    pub fn push(&mut self, elem: T) {
        self.buffer[self.offset].write(elem);
        self.offset += 1;
    }

    #[inline]
    pub fn pop(&mut self) {
        self.offset -= 1;
    }

}