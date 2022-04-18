use core::arch::asm;
use riscv::register::mstatus;

pub(in crate::arch) mod hal_impls {

    #[inline]
    pub(in crate::arch) fn nop() {
        unsafe { asm!("nop") }
    }

    #[inline]
    pub(in crate::arch) unsafe fn enable_interrupts() {
        riscv::interrupt::enable();
    }

    #[inline]
    pub(in crate::arch) unsafe fn disable_interrupts() {
        riscv::interrupt::disable();
    }

    pub(in crate::arch) fn is_interrupts_enabled() -> bool {
        mstatus::read().mie()
    }

    #[inline]
    pub(in crate::arch) unsafe fn wait_for_interrupt() {
        riscv::asm::wfi();
    }

    #[inline]
    pub(in crate::arch) unsafe fn break_point() {
        riscv::asm::ebreak();
    }

}
