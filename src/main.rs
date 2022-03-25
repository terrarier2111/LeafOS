#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

mod vga_buffer;
mod serial;
pub mod driver;
mod filesystem;
mod scheduler;
mod process;
mod shell;

use alloc::boxed::Box;
use core::panic::PanicInfo;
use bootloader::{BootInfo, entry_point};
use volatile::Volatile;
use x86_64::structures::paging::{Page, PageTable, Translate};
use x86_64::VirtAddr;
use LeafOS::{allocator, hlt_loop, memory};
use LeafOS::memory::BootInfoFrameAllocator;

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
    LeafOS::init();

    println!("Initialization succeeded!");

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    // initialize a mapper
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_map)
    };
    allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");

    #[cfg(test)]
    test_main();

    println!("Startup succeeded!");

    hlt_loop();
}

fn overflow(mut x: Volatile<u64>) -> u64 {
    x.write(x.read() + 1);
    println!("{}", x.read());
    overflow(x)
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
