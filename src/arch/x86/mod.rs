use core::arch::asm;

pub mod cpuid;

pub(in crate::arch) mod hal_impls {
    use core::arch::asm;
    use crate::arch::x86::flags;

    #[inline]
    pub(in crate::arch) fn nop() {
        unsafe { asm!("nop") }
    }

    #[inline]
    pub(in crate::arch) unsafe fn enable_interrupts() {
        asm!("sti");
    }

    #[inline]
    pub(in crate::arch) unsafe fn disable_interrupts() {
        asm!("cli");
    }

    pub(in crate::arch) fn is_interrupts_enabled() -> bool {
        const INTERRUPT_FLAG: usize = 0x0200;
        flags() & INTERRUPT_FLAG != 0
    }

    #[inline]
    pub(in crate::arch) unsafe fn wait_for_interrupt() {
        x86::halt();
    }

    #[inline]
    pub(in crate::arch) unsafe fn break_point() {
        asm!("int3");
    }

    #[inline]
    pub(in crate::arch) fn page_size() -> usize {
        4096
    }

}

pub fn flags() -> usize {
    let flags: usize;
    unsafe { asm!(
    "pushf",
    "pop {}",
    out(reg) flags
    ) }
    flags
}
