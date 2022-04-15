use core::arch::asm;

#[inline]
pub fn nop() {
    cortex_a::asm::nop();
}

#[inline]
pub unsafe fn enable_interrupts() {
    // for ARMv7
    asm!("cpsie if");
}

#[inline]
pub unsafe fn disable_interrupts() {
    // for ARMv7
    asm!("cpsid if");
}

pub fn is_interrupts_enabled() -> bool {
    todo!()
}