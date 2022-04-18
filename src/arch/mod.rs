#[cfg(target_arch = "x86_64")]
pub mod x86;
#[cfg(target_arch = "riscv")]
pub mod riscv;
#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[inline]
pub fn nop() {
    #[cfg(target_arch = "x86_64")]
    x86::hal_impls::nop();
    #[cfg(target_arch = "riscv")]
    riscv::hal_impls::nop();
    #[cfg(target_arch = "aarch64")]
    aarch64::hal_impls::nop();
}

#[inline]
pub unsafe fn enable_interrupts() {
    #[cfg(target_arch = "x86_64")]
    x86::hal_impls::enable_interrupts();
    #[cfg(target_arch = "riscv")]
    riscv::hal_impls::enable_interrupts();
    #[cfg(target_arch = "aarch64")]
    aarch64::hal_impls::enable_interrupts();
}

#[inline]
pub unsafe fn disable_interrupts() {
    #[cfg(target_arch = "x86_64")]
    x86::hal_impls::disable_interrupts();
    #[cfg(target_arch = "riscv")]
    riscv::hal_impls::disable_interrupts();
    #[cfg(target_arch = "aarch64")]
    aarch64::hal_impls::disable_interrupts();
}

#[inline]
pub fn is_interrupts_enabled() -> bool {
    #[cfg(target_arch = "x86_64")]
    return x86::hal_impls::is_interrupts_enabled();
    #[cfg(target_arch = "riscv")]
    return riscv::hal_impls::is_interrupts_enabled();
    #[cfg(target_arch = "aarch64")]
    return aarch64::hal_impls::is_interrupts_enabled();
}

pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    if is_interrupts_enabled() {
        unsafe { disable_interrupts() }

        let result = f();

        unsafe { enable_interrupts() }
        result
    } else {
        f()
    }
}

/// Safety:
/// This is only safe to call from ring0
pub unsafe fn wait_for_interrupt() {
    #[cfg(target_arch = "x86_64")]
    x86::hal_impls::wait_for_interrupt();
    #[cfg(target_arch = "riscv")]
    riscv::hal_impls::wait_for_interrupt();
    #[cfg(target_arch = "aarch64")]
    aarch64::hal_impls::wait_for_interrupt();
}

pub unsafe fn break_point() {
    #[cfg(target_arch = "x86_64")]
    x86::hal_impls::break_point();
    #[cfg(target_arch = "riscv")]
    riscv::hal_impls::break_point();
    #[cfg(target_arch = "aarch64")]
    aarch64::hal_impls::wait_for_interrupt();
}
