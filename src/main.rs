#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(const_mut_refs)]
#![feature(strict_provenance)]
#![feature(sync_unsafe_cell)]

extern crate alloc;

mod serial;

use core::panic::PanicInfo;
use core::ptr::NonNull;
use x86::{syscall, halt};
use LeafOS::{hlt_loop, memory, println, scheduler, vga_buffer};
use LeafOS::drivers::pit;
use LeafOS::interrupts::init_apic;
use LeafOS::scheduler::SCHEDULER_TIMER_DELAY;
use LeafOS::syscall::{do_syscall_3, STDOUT_FD, WRITE};

// FIXME: Fix the keyboard handling

// working build command:
// cargo bootimage --release --target x86_64_target.json -Z build-std=core,compiler_builtins,alloc -Z build-std-features=compiler-builtins-mem
// qemu-system-x86_64 -d int -D ./qemu_logs -no-reboot -M smm=off -drive format=raw,file=target/x86_64_target/release/bootimage-LeafOS.bin

static MEM_REQUEST: limine::MemmapRequest = limine::MemmapRequest::new(0);
static ADDRESS_REQUEST: limine::KernelAddressRequest = limine::KernelAddressRequest::new(0);
static FRAMEBUFFER_REQUEST: limine::FramebufferRequest = limine::FramebufferRequest::new(0);
/// Sets the base revision to 1, this is recommended as this is the latest base revision described
/// by the Limine boot protocol specification. See specification for further info.
static BASE_REVISION: limine::BaseRevision = limine::BaseRevision::new(1);

#[no_mangle]
unsafe extern "C" fn _start() -> ! {
    assert!(BASE_REVISION.is_supported());

    if let Some(framebuffer_response) = FRAMEBUFFER_REQUEST.get_response().get() {
        if framebuffer_response.framebuffer_count < 1 {
            loop {}
        }

        // Get the first framebuffer's information.
        let framebuffer = &framebuffer_response.framebuffers()[0];

        for i in 0..100_usize {
            // Calculate the pixel offset using the framebuffer information we obtained above.
            // We skip `i` scanlines (pitch is provided in bytes) and add `i * 4` to skip `i` pixels forward.
            let pixel_offset = i * framebuffer.pitch as usize + i * 4;

            // Write 0xFFFFFFFF to the provided pixel offset to fill it white.
            // We can safely unwrap the result of `as_ptr()` because the framebuffer address is
            // guaranteed to be provided by the bootloader.
            unsafe {
                *(framebuffer.address.as_ptr().unwrap().add(pixel_offset) as *mut u32) = 0xFFFFFFFF;
            }
        }
    }

    let phys_reponse = ADDRESS_REQUEST.get_response().get().unwrap();
    let physical_offset = phys_reponse.physical_base;
    let virtual_offset = phys_reponse.virtual_base;

    // vga_buffer::setup(physical_offset as usize);

    // we disable interrupts for the start so no unexpected shinanigans can occour
    // x86_64::instructions::interrupts::disable();
    // this function is the entry point, since the linker looks for a function
    // named `_start` by default
    println!("Initializing...");

    loop {
        halt();
    }

    LeafOS::init();

    println!("Initialization succeeded!");

    let mem = MEM_REQUEST.get_response().get().unwrap();
    let (table, allocator) = memory::setup(mem.entry_count as usize, unsafe { NonNull::new_unchecked(mem.entries.as_ptr()) }, physical_offset);
    scheduler::init();
    unsafe { init_apic(physical_offset); }
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
