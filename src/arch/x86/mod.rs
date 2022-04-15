use core::arch::asm;

mod cpuid;

#[inline]
pub fn nop() {
    unsafe { asm!("nop") }
}

#[inline]
pub unsafe fn enable_interrupts() {
    asm!("sti");
}

#[inline]
pub unsafe fn disable_interrupts() {
    asm!("cli");
}

pub fn is_interrupts_enabled() -> bool {
    const INTERRUPT_FLAG: usize = 0x0200;
    flags() & INTERRUPT_FLAG != 0
}

fn flags() -> usize {
    let flags: usize;
    unsafe { asm!(
    "pushf",
    "pop {}",
    out(reg) flags
    ) }
    flags
}
