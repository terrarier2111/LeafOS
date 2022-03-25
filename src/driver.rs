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

pub struct CharDriver<T, A>(Box<dyn CharDriverImpl<T>>, PhantomData<A>);

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
