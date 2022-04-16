use core::ptr::addr_of_mut;
use lazy_static::lazy_static;
use x86_64::instructions::tables::load_tss;
// use x86::segmentation::Descriptor;
// use x86::current::task::TaskStateSegment;
// use x86::dtables::DescriptorTablePointer;
// use x86::Ring::Ring0;
// use x86::segmentation::{load_cs, SegmentSelector};
use x86_64::registers::segmentation::{CS, Segment};
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: usize = 0;
const KERNEL_STACK_INDEX: usize = 0;

// FIXME: NOTE: We need to setup a separate GDT and TSS for every CPU core

static mut TSS: TaskStateSegment = TaskStateSegment::new(); // FIXME: Use x86's TSS struct

pub const KERNEL_CODE_SEGMENT_IDX: usize = 1;
pub const KERNEL_DATA_SEGMENT_IDX: usize = 0;
pub const USER_CODE_SEGMENT_IDX: usize = 2;
pub const USER_DATA_SEGMENT_IDX: usize = 3;

lazy_static! {
    static ref GDT: (GlobalDescriptorTable/*DescriptorTablePointer*/, Selectors) = { // FIXME: Use x86's descriptor table pointer struct
        // let mut gdt = DescriptorTablePointer::new();
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
    unsafe {
        /*TSS.set_ist(DOUBLE_FAULT_IST_INDEX, {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = STACK.as_ptr().expose_addr();
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        } as u64);
        TSS.set_rsp(Ring0, {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = STACK.as_ptr().expose_addr();
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        } as u64);*/
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX] = {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = STACK.as_ptr().expose_addr();
            let stack_end = stack_start + STACK_SIZE;
            VirtAddr::new_unsafe(stack_end as u64)
        };
        TSS.privilege_stack_table[KERNEL_STACK_INDEX] = {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = STACK.as_ptr().expose_addr();
            let stack_end = stack_start + STACK_SIZE;
            VirtAddr::new_unsafe(stack_end as u64)
        };
        // TSS.privilege_stack_table[2] // FIXME: Add ring3 stack - is this ring3?
    }

    GDT.0.load();
    unsafe {
        // load_cs();
        CS::set_reg(GDT.1.kernel_code_selector);
        load_tss(GDT.1.tss_selector);
    }
}

#[no_mangle]
extern "C" fn tss_ptr() -> *mut TaskStateSegment {
    let mut tmp = unsafe { TSS };
    addr_of_mut!(tmp)
}
