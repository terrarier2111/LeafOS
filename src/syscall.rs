use alloc::string::String;
use core::arch::asm;
use core::mem;
use crate::error_codes::Error;
use crate::println;

#[no_mangle]
extern "C" fn handle_syscall(mut args: SyscallArgs) {
    match args.syscall_id {
        1 => handle_write(&mut args),
        _ => unimplemented!("syscall: {}, {}", args.syscall_id, args.error),
    }
    // we forget the args value as its on the stack and the assembly code calling this will handle it for us
    mem::forget(args);
}

#[repr(C)]
pub struct SyscallArgs {
    syscall_id: usize, // rax
    arg0: usize, // rdi
    arg1: usize, // rsi
    arg2: usize, // rdx
    arg3: usize, // (r10) - curr: rcx
    arg4: usize, // r8 - curr: r8
    arg5: usize, // r9 - curr: r9
    error: usize, // rax
}

fn handle_write(args: &mut SyscallArgs) {
    let result = _handle_write(args.arg0, args.arg1 as *mut _, args.arg2);
    args.error = result;
}

pub const STDOUT_FD: usize = 1;

fn _handle_write(fd: usize, msg: *const u8, msg_len: usize) -> usize {
    if fd == STDOUT_FD {
        let msg = core::ptr::from_raw_parts::<str>(msg as *const _, msg_len);
        let msg = String::from(unsafe { &*msg });
        // FIXME: Implement this better!
        println!("{}", msg);
        0
    } else {
        Error::EIO as usize
    }
}

fn handle_exit(args: &mut SyscallArgs) {
    _handle_exit(args.arg0);
}

fn _handle_exit(code: usize) {

}

fn handle_mmap(args: &mut SyscallArgs) {
    let result = _handle_write(args.arg0, args.arg1 as *mut _, args.arg2);
    args.error = result;
}

fn _handle_mmap(start: *mut u8, length: usize) -> usize {
    0
}

fn handle_munmap(args: &mut SyscallArgs) {
    let result = _handle_munmap(args.arg0 as *mut _, args.arg1);
    args.error = result;
}

fn _handle_munmap(start: *mut u8, length: usize) -> usize {
    0
}

pub unsafe extern "C" fn do_syscall_0(syscall_id: usize) -> usize {
    let result: usize;
    asm!(
    "int 0x80",
    inout("rax") syscall_id => result
    );
    result
}

pub unsafe extern "C" fn do_syscall_1(syscall_id: usize, arg0: usize) -> usize {
    let result: usize;
    asm!(
    "int 0x80",
    inout("rax") syscall_id => result,
    in("rdi") arg0,
    );
    result
}

pub unsafe extern "C" fn do_syscall_2(syscall_id: usize, arg0: usize, arg1: usize) -> usize {
    let result: usize;
    asm!(
    "int 0x80",
    inout("rax") syscall_id => result,
    in("rdi") arg0,
    in("rsi") arg1,
    );
    result
}

pub unsafe extern "C" fn do_syscall_3(syscall_id: usize, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let result: usize;
    asm!(
    "int 0x80",
    inout("rax") syscall_id => result,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    );
    result
}

pub unsafe extern "C" fn do_syscall_4(syscall_id: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize) -> usize {
    let result: usize;
    asm!(
    "int 0x80",
    inout("rax") syscall_id => result,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    in("rcx") arg3,
    );
    result
}

pub unsafe extern "C" fn do_syscall_5(syscall_id: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize, arg4: usize) -> usize {
    let result: usize;
    asm!(
    "int 0x80",
    inout("rax") syscall_id => result,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    in("rcx") arg3,
    in("r8") arg4,
    );
    result
}

pub unsafe extern "C" fn do_syscall_6(syscall_id: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> usize {
    let result: usize;
    asm!(
    "int 0x80",
    inout("rax") syscall_id => result,
    in("rdi") arg0,
    in("rsi") arg1,
    in("rdx") arg2,
    in("rcx") arg3,
    in("r8") arg4,
    in("r9") arg5,
    );
    result
}

pub const WRITE: usize = 1;
pub const MMAP: usize = 2;
pub const MUNMAP: usize = 3;
