use core::arch::{asm, global_asm};
use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
use lazy_static::lazy_static;
use x86_64::instructions::segmentation::{CS, Segment};
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
const KERNEL_STACK_INDEX: usize = 0;

// FIXME: NOTE: We need to setup a separate GDT and TSS for every CPU core

pub static mut TSS: TaskStateSegment = TaskStateSegment::new();

pub const KERNEL_CODE_SEGMENT_IDX: usize = 1;
pub const KERNEL_DATA_SEGMENT_IDX: usize = 0;
pub const USER_CODE_SEGMENT_IDX: usize = 2;
pub const USER_DATA_SEGMENT_IDX: usize = 3;

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let kernel_code_selector = gdt.add_entry(Descriptor::kernel_code_segment()); // 2nd segment (at index 1)
        let user_code_selector = gdt.add_entry(Descriptor::user_code_segment());
        let user_data_selector = gdt.add_entry(Descriptor::user_data_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(unsafe { &TSS })); // 5th segment (at index 4)
        (gdt, Selectors {
            kernel_code_selector,
            user_code_selector,
            user_data_selector,
            tss_selector,
        })
    };
}

struct Selectors {
    kernel_code_selector: SegmentSelector,
    user_code_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
    tss_selector: SegmentSelector, // there's only ever a single tss selector/segment
}

pub fn init() {
    use x86_64::instructions::tables::load_tss;
    unsafe {
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(unsafe { &STACK });
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };
        TSS.privilege_stack_table[KERNEL_STACK_INDEX] = {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(unsafe { &STACK });
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };
        // TSS.privilege_stack_table[2] // FIXME: Add ring3 stack - is this ring3?
    }

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.kernel_code_selector);
        load_tss(GDT.1.tss_selector);
    }
}

#[no_mangle]
pub extern "C" fn tss_ptr() -> *mut TaskStateSegment {
    let mut tmp = unsafe { TSS };
    addr_of_mut!(tmp)
}

/*
unsafe extern "C" fn switch_to_user(target: u64) {
    asm!("mov ax, (4 * 8) | 3", // ring 3 data with bottom 2 bits set for ring 3
	"mov ds, ax",
	"mov es, ax",
	"mov fs, ax",
	"mov gs, ax", // SS is handled by iret

	// set up the stack frame iret expects
	"mov rax, rsp",
	"push (4 * 8) | 3", // data selector
	"push rax", // current esp
	"pushf", // eflags
	"push (3 * 8) | 3", // code selector (ring 3 code with bottom 2 bits set for ring 3)
	"push {}", // instruction address to return to
	"iret",
    out("rax") _,
    in(reg) target,
    );
}

extern {
    /// Behavior:
    /// uses:
    /// *rax
    ///
    /// parameters:
    /// *rbx: destination pointer
    ///
    /// Note: The final jump can be performed using the "iret" instruction - the stack shouldn't be modified thereafter (nor should any function be called)
    fn setup_jump_to();
}


global_asm!(r#"
.globl setup_jump_to
setup_jump_to:
    mov rax, rsp
    push (4 * 8) | 3 ; data selector
    push rax ; current esp
    pushf ; eflags
    push (3 * 8) | 3 ; code selector (ring 3 code with bottom 2 bits set for ring 3)
    push rbx
"#);*/



// global_asm!(core::include_str!("jump_pad.asm"));

/*
extern {

    pub fn jump_usermode();

    pub fn switch_task();

}*/

// expects address to jump to in rcx FIXME This is only true if we change the 0 before the iret call to rcx
/*
global_asm!(r#"
.globl jump_usermode
jump_usermode:
	mov ax, (3 * 8) | 3
	mov ds, ax
	mov es, ax
	mov fs, ax
	mov gs, ax

	mov rax, rsp
    push (3 * 8) | 3
    push rax
    pushf
    push (2 * 8) | 3
    push 0
    iret

"#);
*/


// FIXME: Replace TCB_RSP_OFFSET with real offset!
// FIXME: Check calculation!
// Expects the address of the current processor core's TCB to be passed in rcx
/*
global_asm!(r#"
.globl switch_task
switch_task:
    push rsi
    push rbx
    push rdi
    push rbp

    mov rdi, rcx
    mov [rdi + TCB_RSP_OFFSET], rsp


    mov rsi, [rsp + (4+1)*4]
    mov [rdi], rsi

    mov rsp, [rsi + TCB_RSP_OFFSET]
    mov rax, [rsi + TCB_CR3_OFFSET]
    mov rbx, [rsi + TCB_RSP0_OFFSET]
    mov [TSS_RSP0], rbx
    mov rcx, cr3

    cmp rax, rcx
    je .doneVAS
    mov cr3, rax
.doneVAS:

    pop rbp
    pop rdi
    pop rbx
    pop rsi

    ret
"#);*/

/*
/// The passed address has to be the end of the stack
pub fn set_stack(stack: *mut u8) {
    unsafe { TSS.privilege_stack_table[KERNEL_STACK_INDEX] = VirtAddr::from_ptr(unsafe { stack }); }
}*/