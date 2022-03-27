use alloc::vec::Vec;
use crate::process::Process;

pub struct Scheduler {
    tasks: Vec<(Process, ProcessState)>,
}

impl Scheduler {

    pub fn run(&mut self) {

    }

}


// This process state should be saved onto the kernel stack when we enter kernel mode so we don't have
// to care about this when context switching
struct ProcessState {
    // FIXME: Add registers
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64, // base pointer
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,

    rsp: u64, // stack pointer (general purpose register)
    rip: u64, // instruction pointer (aka. program counter)
    // FIXME: Add segmentation tables
    // FIXME: Add page tables
}
