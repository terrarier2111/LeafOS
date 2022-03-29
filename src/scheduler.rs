use alloc::vec::Vec;
use x86_64::registers::control::Cr3Flags;
use x86_64::structures::paging::PhysFrame;
use crate::process::Process;

pub trait Scheduler {

    fn run(&mut self);

}

struct RoundRobinScheduler {
    tasks: Vec<(Process, ProcessState)>,
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
    cr3: Option<(PhysFrame, Cr3Flags)>,
}

struct SchedulerEntry {
    process: Process,
    state: ProcessState,
    balance: u64,
}

impl SchedulerEntry {

    fn is_kernel_owned(&self) -> bool {
        self.state.cr3.is_none()
    }

}
