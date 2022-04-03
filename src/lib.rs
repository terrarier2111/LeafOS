#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]
#![cfg_attr(test, no_main)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(alloc_error_handler)]
#![feature(const_mut_refs)]
#![no_std]
#![feature(stdsimd)]
#![feature(step_trait)]
#![feature(adt_const_params)] // used for checking for presence of cpuid instruction

extern crate alloc;

use alloc::boxed::Box;
use core::alloc::Layout;
use core::mem::{transmute, transmute_copy};
use core::panic::PanicInfo;
use core::ptr::addr_of;
use bootloader::{BootInfo, entry_point};
use lazy_static::lazy_static;
use pc_keyboard::DecodedKey;
use spin::Mutex;
use crate::events::KeyboardEvent;
use crate::shell::{has_shell, SHELL};

pub mod vga_buffer;
pub mod interrupts;
pub mod serial;
pub mod gdt;
pub mod memory;
pub mod print;
pub mod events;
pub mod shell;
pub(crate) mod allocators;
mod ring_buffer;
mod cpuid;
mod drivers;

pub fn init() {
    gdt::init();
    interrupts::init_idt();
    unsafe { interrupts::PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();
}

pub fn init_kb_handler() {
    events::EVENT_HANDLERS.lock().register_keyboard_handler(Box::new(|event| {
        // println!("keyee: {:?}", event.key);
        if has_shell() {
            SHELL.lock().key_event(event.key.clone());
        }
    }));
}

// Testing machinery

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

pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}

#[cfg(test)]
entry_point!(test_kernel_main);

/// Entry point for `cargo test`
#[cfg(test)]
fn test_kernel_main(_boot_info: &'static BootInfo) -> ! {
    init();
    test_main();
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
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

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}

