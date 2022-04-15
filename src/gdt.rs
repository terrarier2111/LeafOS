use core::ptr::addr_of_mut;
use lazy_static::lazy_static;
use x86_64::instructions::segmentation::{CS, Segment};
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
const KERNEL_STACK_INDEX: usize = 0;

// FIXME: NOTE: We need to setup a separate GDT and TSS for every CPU core

static mut TSS: TaskStateSegment = TaskStateSegment::new();

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

            let stack_start = VirtAddr::from_ptr(&STACK);
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };
        TSS.privilege_stack_table[KERNEL_STACK_INDEX] = {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(&STACK);
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
extern "C" fn tss_ptr() -> *mut TaskStateSegment {
    let mut tmp = unsafe { TSS };
    addr_of_mut!(tmp)
}
