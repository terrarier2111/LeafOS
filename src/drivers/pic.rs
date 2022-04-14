use core::arch::{asm, global_asm};

pub unsafe fn disable() {
    asm!(
    "mov al, 0xff",
    "out 0xa1, al",
    "out 0x21, al",
    out("eax") _ // we can only use eax (which includes al)
    );
}