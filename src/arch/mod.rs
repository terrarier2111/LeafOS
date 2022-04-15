mod aarch64;
mod riscv;
mod x86;

#[inline]
pub fn nop() {
    #[cfg(target_arch = "x86_64")]
    x86::nop();
    #[cfg(target_arch = "riscv")]
    riscv::nop();
    #[cfg(target_arch = "aarch64")]
    aarch64::nop();
}

#[inline]
pub unsafe fn enable_interrupts() {
    #[cfg(target_arch = "x86_64")]
    x86::enable_interrupts();
    #[cfg(target_arch = "riscv")]
    riscv::enable_interrupts();
    #[cfg(target_arch = "aarch64")]
    aarch64::enable_interrupts();
}

#[inline]
pub unsafe fn disable_interrupts() {
    #[cfg(target_arch = "x86_64")]
    x86::disable_interrupts();
    #[cfg(target_arch = "riscv")]
    riscv::disable_interrupts();
    #[cfg(target_arch = "aarch64")]
    aarch64::disable_interrupts();
}

#[inline]
pub fn is_interrupts_enabled() -> bool {
    #[cfg(target_arch = "x86_64")]
    return x86::is_interrupts_enabled();
    #[cfg(target_arch = "riscv")]
    return riscv::is_interrupts_enabled();
    #[cfg(target_arch = "aarch64")]
    return aarch64::is_interrupts_enabled();
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


