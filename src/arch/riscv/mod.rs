use core::arch::asm;
use riscv::register::mstatus;

#[inline]
pub fn nop() {
    unsafe { asm!("nop") }
}

#[inline]
pub unsafe fn enable_interrupts() {
    riscv::interrupt::enable();
}

#[inline]
pub unsafe fn disable_interrupts() {
    riscv::interrupt::disable();
}

pub fn is_interrupts_enabled() -> bool {
    mstatus::read().mie()
}