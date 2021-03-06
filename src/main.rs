#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_mut_refs)]
#![feature(strict_provenance)]

extern crate alloc;

mod serial;

use core::panic::PanicInfo;
use bootloader::{BootInfo, entry_point};
use x86::syscall;
use LeafOS::{hlt_loop, memory, println, scheduler};
use LeafOS::drivers::pit;
use LeafOS::interrupts::init_apic;
use LeafOS::scheduler::SCHEDULER_TIMER_DELAY;
use LeafOS::syscall::{do_syscall_3, STDOUT_FD, WRITE};

// FIXME: Fix the keyboard handling

// working build command:
// cargo bootimage --release --target x86_64_target.json -Z build-std=core,compiler_builtins,alloc -Z build-std-features=compiler-builtins-mem
// qemu-system-x86_64 -d int -D ./qemu_logs -no-reboot -M smm=off -drive format=raw,file=target/x86_64_target/release/bootimage-LeafOS.bin

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // we disable interrupts for the start so no unexpected shinanigans can occour
    // x86_64::instructions::interrupts::disable();
    // this function is the entry point, since the linker looks for a function
    // named `_start` by default
    println!("Initializing...");

    LeafOS::init();

    println!("Initialization succeeded!");

    let (table, allocator) = memory::setup(&boot_info.memory_map, boot_info.physical_memory_offset);
    scheduler::init();
    unsafe { init_apic(boot_info.physical_memory_offset); }
    pit::init();
    LeafOS::interrupts::start_timer_one_shot(SCHEDULER_TIMER_DELAY);

    scheduler::start_proc(test_fn, true);
    scheduler::start_proc(test_fn_hello, true);

    #[cfg(test)]
    test_main();

    println!("Startup succeeded!");
    LeafOS::shell::SHELL.lock().init();

    LeafOS::init_kb_handler();

    // x86_64::instructions::interrupts::enable();

    hlt_loop();
}

fn test_fn() {
    loop {
        // println!("test1");
        // syscall!()
        static MSG: &str = "TESTeee!";
        unsafe { do_syscall_3(WRITE, STDOUT_FD, MSG.as_ptr().expose_addr(), MSG.len()); }
    }
}

fn test_fn_hello() {
    loop {
        // println!("HELLO");
    }
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    hlt_loop();}

/// This function is called on test failure or when a panic occurs during testing.
#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub trait Testable {
    fn run(&self) -> ();
}


impl<T> Testable for T
    where
        T: Fn(),
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}
