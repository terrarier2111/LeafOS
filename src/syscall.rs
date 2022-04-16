pub fn handle_syscall(args: *const SyscallArgs) {

}

#[repr(C)]
pub struct SyscallArgs {
    syscall_id: u64, // rax
    rdi: u64,
    rsi: u64,
    rdx: u64,
    r10: u64,
    r8: u64,
    r9: u64,
}