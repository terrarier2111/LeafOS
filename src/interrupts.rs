use core::arch::{asm, global_asm};
use core::mem::transmute;
use core::ops::{Index, IndexMut};
use core::ptr::addr_of;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use pc_keyboard::{DecodedKey, HandleControl, Keyboard, layouts, ScancodeSet1};
use pic8259::ChainedPics;
use spin::{Mutex, Once};
use x2apic::lapic::{LocalApic, LocalApicBuilder, TimerDivide, TimerMode, xapic_base};
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::structures::paging::{OffsetPageTable, Translate};
use x86_64::structures::paging::mapper::TranslateResult;
use x86_64::VirtAddr;
use crate::{gdt, hlt_loop, print, println, scheduler, shell};
use crate::drivers::{pic, pit};
use crate::drivers::pit::PIT_DIVIDEND;
use crate::events::KeyboardEvent;
use crate::scheduler::SCHEDULER_TIMER_DELAY;
use crate::shell::{has_shell, SHELL};

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

static APIC_TIMER_FREQUENCY: AtomicUsize = AtomicUsize::new(0);

pub fn init() {
    unsafe {
        IDT.breakpoint.set_handler_fn(breakpoint_handler);
        IDT.overflow.set_handler_fn(overflow_handler);
        IDT.bound_range_exceeded.set_handler_fn(bound_range_exceeded_handler);
        IDT.general_protection_fault.set_handler_fn(general_protection_fault_handler);
        IDT.alignment_check.set_handler_fn(alignment_check_handler);
        IDT.divide_error.set_handler_fn(divide_error_handler);
        IDT.non_maskable_interrupt.set_handler_fn(non_maskable_interrupt_handler);
        IDT.invalid_opcode.set_handler_fn(invalid_opcode_handler);
        IDT.device_not_available.set_handler_fn(device_unavailable_handler);
        IDT.segment_not_present.set_handler_fn(segment_not_present_handler);
        IDT.stack_segment_fault.set_handler_fn(stack_segmentation_fault_handler);
        IDT.security_exception.set_handler_fn(security_handler);
        IDT.simd_floating_point.set_handler_fn(simd_floating_point_handler);
        IDT.x87_floating_point.set_handler_fn(x87_floating_point_handler);
        IDT.vmm_communication_exception.set_handler_fn(vmm_communication_handler);
        IDT.virtualization.set_handler_fn(virtualization_handler);
        // IDT.machine_check.set_handler_fn(machine_check_handler);
        IDT.debug.set_handler_fn(debug_handler);
        IDT.invalid_tss.set_handler_fn(invalid_tss_handler);
        IDT.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            IDT.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        IDT[InterruptIndex::Timer.as_usize()]
            .set_handler_fn(timer_interrupt_handler);
        IDT[InterruptIndex::Keyboard.as_usize()]
            .set_handler_fn(keyboard_interrupt_handler);
        IDT[InterruptIndex::ApicTimer.as_usize()].set_handler_fn(apic_timer_config_handler);
        IDT[InterruptIndex::ApicError.as_usize()].set_handler_fn(apic_error_handler);
        IDT[InterruptIndex::ApicSpurious.as_usize()].set_handler_fn(apic_spurious_handler);
    }
    unsafe { IDT.load(); }
}

pub unsafe fn init_apic(physical_memory_offset: u64) {
    const TIMER_DELAY: u16 = u16::MAX/*255*//*5000*/;
    let apic_physical_address: u64 = xapic_base();
    let apic_virtual_address = physical_memory_offset + apic_physical_address;
    let lapic = LocalApicBuilder::new()
        .timer_vector(InterruptIndex::ApicTimer.as_u8() as usize)
        .error_vector(InterruptIndex::ApicError.as_u8() as usize)
        .spurious_vector(InterruptIndex::ApicSpurious.as_u8() as usize)
        .set_xapic_base(apic_virtual_address)
        .build()
        .unwrap_or_else(|err| panic!("{}", err));
    LAPIC.replace(lapic);
    // lapic was enabled, we can now safely disable the pic
    {
        LAPIC.as_mut().unwrap().set_timer_divide(TimerDivide::Div2/*TimerDivide::Div64*/);
        LAPIC.as_mut().unwrap().set_timer_initial(TIMER_DELAY as u32);
        LAPIC.as_mut().unwrap().set_timer_mode(TimerMode::OneShot);
        pit::write_channel0_count(TIMER_DELAY);
    }
    LAPIC.as_mut().unwrap().enable();

    while LAPIC.as_mut().unwrap().timer_current() > 0 {
        println!("apicccc {}", LAPIC.as_mut().unwrap().timer_current());
    }

    let end = pit::read_pit_count() as usize;
    let frequency = (TIMER_DELAY as usize) / ((TIMER_DELAY as usize) - end) * PIT_DIVIDEND;
    APIC_TIMER_FREQUENCY.store(frequency, Ordering::Relaxed);
    pic::disable();
    // replace the IDT entry of the apic timer with a new one (for scheduling)
    IDT[InterruptIndex::ApicTimer.as_usize()].set_handler_fn(apic_timer_handler);

}

extern "x86-interrupt" fn breakpoint_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn divide_error_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: DIVIDE ERROR\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn debug_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: DEBUG\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn non_maskable_interrupt_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: NON MASKABLE INTERRUPT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn overflow_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: OVERFLOW\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn bound_range_exceeded_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: OOB\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn invalid_opcode_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: INVALID OP CODE\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn device_unavailable_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: DEVICE UNAVAILABLE\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn invalid_tss_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    panic!("EXCEPTION: INVALID TSS\n{:#?}\nERROR CODE: {}", stack_frame, error_code);
}

extern "x86-interrupt" fn alignment_check_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    panic!("EXCEPTION: ALIGNMENT ERROR\n{:#?}\nERROR CODE: {}", stack_frame, error_code);
}

extern "x86-interrupt" fn segment_not_present_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    panic!("EXCEPTION: SEGMENT NOT PRESENT\n{:#?}\nERROR CODE: {}", stack_frame, error_code);
}

extern "x86-interrupt" fn x87_floating_point_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: X87 FLOATING POINT ERROR\n{:#?}", stack_frame);
}

/*
extern "x86-interrupt" fn machine_check_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: MACHINE CHECK ERROR\n{:#?}", stack_frame)
}*/

extern "x86-interrupt" fn simd_floating_point_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: SIMD FLOATING POINT ERROR\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn virtualization_handler(
    stack_frame: InterruptStackFrame)
{
    panic!("EXCEPTION: VIRTUALIZATION ERROR\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn vmm_communication_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    panic!("EXCEPTION: VMM COMMUNICATION ERROR\n{:#?}\nERROR CODE: {}", stack_frame, error_code);
}

extern "x86-interrupt" fn security_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    panic!("EXCEPTION: SECURITY ERROR\n{:#?}\nERROR CODE: {}", stack_frame, error_code);
}

extern "x86-interrupt" fn stack_segmentation_fault_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    panic!("EXCEPTION: STACK SEGMENTATION FAULT\n{:#?}\nERROR CODE: {}", stack_frame, error_code);
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    panic!("EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}\nError code: {}\n", stack_frame, error_code);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame, error_code: u64) -> !
{
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}\nError code: {}\n", stack_frame, error_code);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    hlt_loop();
}

extern "x86-interrupt" fn apic_timer_config_handler(
    _stack_frame: InterruptStackFrame)
{
    TRIGGERED_ONCE.store(true, Ordering::SeqCst);
    unsafe { LAPIC.as_mut().unwrap().end_of_interrupt(); }
}

static TRIGGERED_ONCE: AtomicBool = AtomicBool::new(false);

// https://lwn.net/Articles/484932/

#[no_mangle]
pub fn restart_apic() {
    unsafe { LAPIC.as_mut().unwrap().end_of_interrupt(); }

    start_timer_one_shot(SCHEDULER_TIMER_DELAY);
}

#[no_mangle]
#[naked]
pub extern "x86-interrupt" fn apic_timer_handler(_interrupt_stack_frame: InterruptStackFrame) {
    unsafe {
        asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push rbp",

        "call restart_apic",

        "call current_task_ptr",
        "mov [rax], rsp",

        "call select_next_task",

        "mov rsp, [rax]",
        "mov rbx, [rax + 8]",

        "push rbx",
        "call tss_ptr",
        "pop rbx",

        "mov [rax + 4], rbx",


        // "mov ax, (3 * 8) | 3", // ring 3 data with bottom 2 bits set for ring 3
        "mov ax, (0 * 8) | 0", // ring 0 data
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax", // SS is handled by iretq

        "pop rbp",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "iretq",
        options(noreturn));
    }
}

/*
global_asm!(r#"
.globl apic_timer_handler
apic_timer_handler:
	push rax
	push rbx
	push rcx
	push rdx
	push rsi
	push rdi
	push r8
	push r9
	push r10
	push r11
	push r12
	push r13
	push r14
	push r15
	push rbp

    call restart_apic # restarts the apic timer

    call current_task_ptr
    mov [rax], rsp

    call select_next_task # moves a pointer to the next task in rax (64 bit rsp field, 64 bit rsp_top field)

    mov rsp, [rax] # switch to new task's stack
    mov rbx, [rax + 8] # store stack top

    push rbx
    call tss_ptr # a get a pointer to the TSS
    pop rbx

    mov [rax + 4], rbx # set the privilege level 0 stack to the a pointer to the top of the new task's stack

    mov ax, (0 * 8) | 0 # ring 0 data segment
	mov ds, ax
	mov es, ax
	mov fs, ax
	mov gs, ax # SS is handled by iretq

	pop rbp
	pop r15
	pop r14
	pop r13
	pop r12
	pop r11
	pop r10
	pop r9
	pop r8
	pop rdi
	pop rsi
	pop rdx
	pop rcx
	pop rbx
	pop rax
	iretq
	"#);*/

/*
extern "x86-interrupt" fn apic_timer_handler(
    _stack_frame: InterruptStackFrame)
{
    unsafe { LAPIC.as_mut().unwrap().end_of_interrupt(); }
    // https://github.com/rust-lang/rust/issues/40180


    start_timer_one_shot(SCHEDULER_TIMER_DELAY);
    // println!("apic timer!\n{:?}", _stack_frame);
    // while true {}
    // scheduler::run();
    unsafe {
        // asm!("push rax");

        asm!("push rbx");
        asm!(
    // Save registers
    /*"push rax",
    "push rbx",
    "push rcx",
    "push rdx",
    "push rsi",
    "push rdi",
    "push r8",
    "push r9",
    "push r10",
    "push r11",
    "push r12",
    "push r13",
    "push r14",
    "push r15",*/
    // save cr3
    /*"mov rcx, cr3", // FIXME: Support virtual address spaces!
    "push rcx",*/
    "push rbp",

    "call current_task_ptr",
    "mov [rax], rsp", // save current task's stack pointer

    // prepare next task
    "call select_next_task", // next task goes into rax
        "mov rsp, [rax]", // load stack pointer
        "mov rbx, [rax + 8]", // get the top of the stack
        // "push rbx", // FIXME: Do we even need this?
        "call tss_ptr",
        // "pop rbx", // FIXME: Do we even need this?
        "mov [rax + 4], rbx", // update tss kernel stack to point to the top of the new task's stack

    // "cmp rax, rcx", // FIXME: Support virtual address spaces!
    // FIXME: Move the page table of the new process's virtual address space


    // "2: jmp 2b", // FIXME: Is this correct?


    "pop rbp",
    // load cr3
    /*"pop rax", // FIXME: Support virtual address spaces!
    "mov cr3, rax",*/
    // load other registers
    /*"pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop r11",
    "pop r10",
    "pop r9",
    "pop r8",
    "pop rdi",
    "pop rsi",
    "pop rdx",
    "pop rcx",
    "pop rbx",
    "pop rax",*/
    // FIXME: Add in/out
    // inout("rax") _ => _,
    out("rax") _,
    // out("rbx") _,
    out("rcx") _,
    out("rdx") _,
        out("rsi") _,
        out("rdi") _,
        out("r8") _,
        out("r9") _,
        out("r10") _,
        out("r11") _,
        out("r12") _,
        out("r13") _,
        out("r14") _,
        out("r15") _,
    );
        // asm!("pop rax");
        asm!("pop rbx");

        asm!("iret");
    }
}*/

extern "x86-interrupt" fn apic_error_handler(
    _stack_frame: InterruptStackFrame)
{
    println!("apic error handler!");
    unsafe { LAPIC.as_mut().unwrap().end_of_interrupt(); }
}

extern "x86-interrupt" fn apic_spurious_handler(
    _stack_frame: InterruptStackFrame)
{
    println!("apic spurious handler!");
    unsafe { LAPIC.as_mut().unwrap().end_of_interrupt(); }
}

// pic stuff

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

// FIXME: Make this per-core
static mut LAPIC: Option<LocalApic> = None;

pub fn start_timer_one_shot(us: usize) {
    unsafe {
        LAPIC.as_mut().unwrap().set_timer_divide(TimerDivide::Div2);
        LAPIC.as_mut().unwrap().set_timer_mode(TimerMode::OneShot);
        LAPIC.as_mut().unwrap().set_timer_initial((us * (APIC_TIMER_FREQUENCY.load(Ordering::SeqCst) / 1000000)) as u32);
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    ApicTimer = 33,
    ApicError = 34,
    ApicSpurious = 35,
    Keyboard,
    Invalid = 255,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    pit::handle_timer();

    // This notifies the cpu that the interrupt was processed and that it can send the next one as soon as it's ready/triggered
    unsafe {
        end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    lazy_static! {
        static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
            Mutex::new(Keyboard::new(layouts::Us104Key, ScancodeSet1,
                HandleControl::Ignore)
            );
    }

    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(0x60);

    let scancode: u8 = unsafe { port.read() };
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            crate::events::EVENT_HANDLERS.lock().call_keyboard_event(KeyboardEvent {
                key,
            });
        }
    }
    // This notifies the cpu that the interrupt was processed and that it can send the next one as soon as it's ready/triggered
    unsafe {
        end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

fn has_lapic() -> bool {
    unsafe { LAPIC.is_some() }
}

unsafe fn end_of_interrupt(interrupt_id: u8) {
    if has_lapic() {
        LAPIC.as_mut().unwrap().end_of_interrupt();
    } else {
        PICS.lock().notify_end_of_interrupt(interrupt_id);
    }
}

#[test_case]
fn test_breakpoint_exception() {
    // invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}
