use core::arch::asm;

pub(in crate::arch) mod hal_impls {

    #[inline]
    pub(in crate::arch) fn nop() {
        cortex_a::asm::nop();
    }

    #[inline]
    pub(in crate::arch) unsafe fn enable_interrupts() {
        // for ARMv7
        asm!("cpsie if");
    }

    #[inline]
    pub(in crate::arch) unsafe fn disable_interrupts() {
        // for ARMv7
        asm!("cpsid if");
    }

    pub(in crate::arch) fn is_interrupts_enabled() -> bool {
        todo!()
    }

    #[inline]
    pub(in crate::arch) unsafe fn wait_for_interrupt() {
        cortex_a::asm::wfi();
    }

    pub(in crate::arch) unsafe fn break_point() {
        todo!()
    }

    #[inline]
    pub(in crate::arch) unsafe fn page_size() -> usize {
        todo!()
    }

}
