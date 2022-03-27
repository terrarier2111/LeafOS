#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_mut_refs)]

extern crate alloc;

mod serial;
mod filesystem;
mod scheduler;
mod process;

use alloc::boxed::Box;
use alloc::string::String;
use core::panic::PanicInfo;
use core::ptr::addr_of;
use bootloader::{BootInfo, entry_point};
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;
use x86_64::structures::paging::{Page, PageTable, Translate};
use x86_64::VirtAddr;
use LeafOS::{hlt_loop, memory, println, shell};
use LeafOS::memory::BootInfoFrameAllocator;
use crate::shell::{Shell, SHELL};
use LeafOS::vga_buffer::ColoredString;

// working build command:
// cargo bootimage --release --target x86_64_target.json -Z build-std=core,compiler_builtins,alloc -Z build-std-features=compiler-builtins-mem
// qemu-system-x86_64 -drive format=raw,file=target/x86_64_target/release/bootimage-LeafOS.bin

// issue: https://github.com/phil-opp/blog_os/discussions/998#discussioncomment-861868
// https://github.com/phil-opp/blog_os/discussions/998#discussioncomment-861968

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // this function is the entry point, since the linker looks for a function
    // named `_start` by default
    println!("Initializing...");

    // println!("Hello World{}", "!");
    // panic!("test!");

    // LeafOS::check_lazy();

    LeafOS::init();

    println!("Initialization succeeded!");

    let (table, allocator) = memory::setup(&boot_info.memory_map, boot_info.physical_memory_offset);

    #[cfg(test)]
    test_main();

    // LeafOS::check_lazy();
    // println!("events main: {:?}", addr_of!(*events::EVENT_HANDLERS));

    let shell_addr = addr_of!(*SHELL);
    unsafe { println!("main shell: {:?}", shell_addr) };

    // let shell = Shell::new(ColoredString::from_string(String::from("test: ")));
    // shell::SHELL.lock().replace(shell);

    let shell_addr = addr_of!(*SHELL);
    unsafe { println!("main shell: {:?}", shell_addr) };

    println!("Startup succeeded!");
    LeafOS::shell::SHELL.lock().init();

    LeafOS::init_kb_handler();

    hlt_loop();
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

// https://os.phil-opp.com/minimal-rust-kernel/#target-specification
