use alloc::boxed::Box;
use core::marker::PhantomData;
use x86_64::structures::idt::InterruptDescriptorTable;

// TODO: Implement event system to detect driver/device events

pub unsafe trait Driver {

    unsafe fn init(&mut self, idt: &mut InterruptDescriptorTable) -> bool;

    unsafe fn exit(&mut self);

}

pub struct ReadOnly;
pub struct WriteOnly;
pub struct ReadWrite;

pub unsafe trait CharDriverImpl<T/*, I*/>: Driver { // FIXME: MAYBE: Generic index parameter

    unsafe fn write_char(&mut self, char: &T);

    unsafe fn write_char_indexed(&mut self, index: usize, char: &T);

    unsafe fn read_char(&mut self) -> T;

    unsafe fn read_char_indexed(&mut self, index: usize) -> T;

}

pub struct CharDriver<T, A, const B: usize = 38>(Box<dyn CharDriverImpl<T>>, PhantomData<A>);

impl<T, A> CharDriver<T, A> {

    pub fn new_read_write(inner_impl: Box<dyn CharDriverImpl<T>>) -> CharDriver<T, ReadWrite> {
        CharDriver(inner_impl, PhantomData/*Default::default()*/)
    }

    pub fn new_read_only(inner_impl: Box<dyn CharDriverImpl<T>>) -> CharDriver<T, ReadOnly> {
        CharDriver(inner_impl, PhantomData/*Default::default()*/)
    }

    pub fn new_write_only(inner_impl: Box<dyn CharDriverImpl<T>>) -> CharDriver<T, WriteOnly> {
        CharDriver(inner_impl, PhantomData/*Default::default()*/)
    }

}

unsafe impl<T, A> Driver for CharDriver<T, A> {
    #[inline]
    unsafe fn init(&mut self, idt: &mut InterruptDescriptorTable) -> bool {
        self.0.init(idt)
    }

    #[inline]
    unsafe fn exit(&mut self) {
        self.0.exit()
    }
}

impl<T> CharDriver<T, ReadOnly> {

    #[inline]
    pub unsafe fn read_char(&mut self) -> T {
        self.0.read_char()
    }

    #[inline]
    pub unsafe fn read_char_indexed(&mut self, index: usize) -> T {
        self.0.read_char_indexed(index)
    }

}

impl<T> CharDriver<T, WriteOnly> {

    #[inline]
    pub unsafe fn write_char(&mut self, char: &T) {
        self.0.write_char(char)
    }

    #[inline]
    pub unsafe fn write_char_indexed(&mut self, index: usize, char: &T) {
        self.0.write_char_indexed(index, char)
    }

}

impl<T> CharDriver<T, ReadWrite> {

    #[inline]
    pub unsafe fn read_char(&mut self) -> T {
        self.0.read_char()
    }

    #[inline]
    pub unsafe fn read_char_indexed(&mut self, index: usize) -> T {
        self.0.read_char_indexed(index)
    }

    #[inline]
    pub unsafe fn write_char(&mut self, char: &T) {
        self.0.write_char(char)
    }

    #[inline]
    pub unsafe fn write_char_indexed(&mut self, index: usize, char: &T) {
        self.0.write_char_indexed(index, char)
    }

}

pub unsafe trait BlockDriverImpl<T/*, I*/>: Driver { // FIXME: MAYBE: Generic index parameter

    unsafe fn write_block(&mut self, block: &[T]);

    unsafe fn write_block_indexed(&mut self, index: usize, block: &[T]);

    unsafe fn read_block(&mut self, block_size: usize) -> Box<[T]>;

    unsafe fn read_block_indexed(&mut self, index: usize, block_size: usize) -> Box<[T]>;

}

pub struct BlockDriver<T, A>(Box<dyn BlockDriverImpl<T>>, PhantomData<A>);

impl<T, A> BlockDriver<T, A> {

    pub fn new_read_write(inner_impl: Box<dyn BlockDriverImpl<T>>) -> BlockDriver<T, ReadWrite> {
        BlockDriver(inner_impl, PhantomData/*Default::default()*/)
    }

    pub fn new_read_only(inner_impl: Box<dyn BlockDriverImpl<T>>) -> BlockDriver<T, ReadOnly> {
        BlockDriver(inner_impl, PhantomData/*Default::default()*/)
    }

    pub fn new_write_only(inner_impl: Box<dyn BlockDriverImpl<T>>) -> BlockDriver<T, WriteOnly> {
        BlockDriver(inner_impl, PhantomData/*Default::default()*/)
    }

}

unsafe impl<T, A> Driver for BlockDriver<T, A> {
    #[inline]
    unsafe fn init(&mut self, idt: &mut InterruptDescriptorTable) -> bool {
        self.0.init(idt)
    }

    #[inline]
    unsafe fn exit(&mut self) {
        self.0.exit()
    }
}

impl<T> BlockDriver<T, ReadOnly> {

    #[inline]
    pub unsafe fn read_block(&mut self, block_size: usize) -> Box<[T]> {
        self.0.read_block(block_size)
    }

    #[inline]
    pub unsafe fn read_block_indexed(&mut self, index: usize, block_size: usize) -> Box<[T]> {
        self.0.read_block_indexed(index, block_size)
    }

}

impl<T> BlockDriver<T, WriteOnly> {

    #[inline]
    pub unsafe fn write_block(&mut self, block: &[T]) {
        self.0.write_block(block)
    }

    #[inline]
    pub unsafe fn write_block_indexed(&mut self, index: usize, block: &[T]) {
        self.0.write_block_indexed(index, block)
    }

}

impl<T> BlockDriver<T, ReadWrite> {

    #[inline]
    pub unsafe fn read_block(&mut self, block_size: usize) -> Box<[T]> {
        self.0.read_block(block_size)
    }

    #[inline]
    pub unsafe fn read_block_indexed(&mut self, index: usize, block_size: usize) -> Box<[T]> {
        self.0.read_block_indexed(index, block_size)
    }

    #[inline]
    pub unsafe fn write_block(&mut self, block: &[T]) {
        self.0.write_block(block)
    }

    #[inline]
    pub unsafe fn write_block_indexed(&mut self, index: usize, block: &[T]) {
        self.0.write_block_indexed(index, block)
    }

}

