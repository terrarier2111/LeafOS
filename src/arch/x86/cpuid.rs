use lazy_static::lazy_static;

lazy_static! {
    static ref CPU_ID: bool = {
        core::arch::x86_64::has_cpuid()
    };
}

#[inline]
pub fn has_cpuid() -> bool {
    *CPU_ID
}