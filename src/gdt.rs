use core::cell::SyncUnsafeCell;
use core::mem::size_of;
use core::ptr::addr_of_mut;
use lazy_static::lazy_static;
use x86::current::task::TaskStateSegment;
use x86::segmentation::{Descriptor, DescriptorBuilder};
use x86_64::instructions::tables::load_tss;
// use x86::segmentation::Descriptor;
// use x86::current::task::TaskStateSegment;
// use x86::dtables::DescriptorTablePointer;
// use x86::Ring::Ring0;
// use x86::segmentation::{load_cs, SegmentSelector};
use x86_64::registers::segmentation::{CS, Segment};
use x86_64::structures::gdt::{GlobalDescriptorTable, SegmentSelector};
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: usize = 0;
const KERNEL_STACK_INDEX: usize = 0;

// FIXME: NOTE: We need to setup a separate GDT and TSS for every CPU core

static TSS: SyncUnsafeCell<TaskStateSegment> = SyncUnsafeCell::new(TaskStateSegment::new());

pub const KERNEL_CODE_SEGMENT_IDX: usize = 1;
pub const KERNEL_DATA_SEGMENT_IDX: usize = 0;
pub const USER_CODE_SEGMENT_IDX: usize = 2;
pub const USER_DATA_SEGMENT_IDX: usize = 3;

const GDT_ENTRIES: usize = 7;

const DESC_ATTRS_GENERIC: u64 = gdt::PRESENT | gdt::USER_SEGMENT | gdt::GRANULARITY;
static GDT_NEW: [Descriptor; GDT_ENTRIES] = [
    Descriptor::NULL,
    // kernel code
    desc_from_raw(DESC_ATTRS_GENERIC | gdt::EXECUTABLE | gdt::LONG_MODE | gdt::LIMIT_0_15 | gdt::LIMIT_16_19),
    // kernel data
    desc_from_raw(DESC_ATTRS_GENERIC | gdt::LIMIT_0_15 | gdt::LIMIT_16_19),
    // user code
    // FIXME: also is the conforming bit here senseful?
    desc_from_raw(DESC_ATTRS_GENERIC | gdt::EXECUTABLE | gdt::LONG_MODE | gdt::CONFORMING | gdt::dpl(gdt::Ring::R3) | gdt::LIMIT_0_15 | gdt::LIMIT_16_19),
    // user data
    // FIXME: reduce this limit!
    desc_from_raw(DESC_ATTRS_GENERIC | gdt::dpl(gdt::Ring::R3) | gdt::LIMIT_0_15 | gdt::LIMIT_16_19),
    // tss part 1
    desc_from_raw(gdt::PRESENT | gdt::tss::Type::TSSAvailable | gdt::tss::base_from_addr(&TSS as *const SyncUnsafeCell<TaskStateSegment> as *const ()).0 | gdt::LIMIT_0_15 | gdt::LIMIT_16_19),
    // tss part 2 (this isn't truly a descriptor on its own but we can just pretend it is)
    desc_from_raw(gdt::tss::base_from_addr(&TSS as *const SyncUnsafeCell<TaskStateSegment> as *const ()).1),
];
static GDT_PTR: GdtPtr = GdtPtr {
    ptr: (&GDT_NEW as *const _) as usize as *const () as *mut (),
    size: size_of::<usize>() * GDT_ENTRIES - 1,
};

#[inline]
const fn desc_from_raw(raw: u64) -> Descriptor {
    Descriptor { lower: raw as u32, upper: (raw >> 32) as u32 }
}

struct GdtPtr {
    ptr: *mut (),
    size: usize,
}

unsafe impl Send for GdtPtr {}
unsafe impl Sync for GdtPtr {}

static GDT: (GlobalDescriptorTable/*DescriptorTablePointer*/, Selectors) = { // FIXME: Use x86's descriptor table pointer struct
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
        (&mut *TSS.get()).ist[DOUBLE_FAULT_IST_INDEX] = {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = STACK.as_ptr().expose_addr();
            let stack_end = stack_start + STACK_SIZE;
            VirtAddr::new_unsafe(stack_end as u64) // FIXME: when switching to higher half ensure the trailing bits are set to 1
        };
        (&mut *TSS.get()).rsp[KERNEL_STACK_INDEX] = {
            const STACK_SIZE: usize = 4096/* * 5*/;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = STACK.as_ptr().expose_addr();
            let stack_end = stack_start + STACK_SIZE;
            VirtAddr::new_unsafe(stack_end as u64) // FIXME: when switching to higher half ensure the trailing bits are set to 1
        };
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
    TSS.get()
}

mod gdt {
    use crate::util::build_bit_mask;

    pub(crate) const ACCESSED: usize          = 1 << 40;
    /// For 32-bit data segments, sets the segment as writable. For 32-bit code segments,
    /// sets the segment as _readable_. In 64-bit mode, ignored for all segments.
    pub(crate) const WRITABLE: usize          = 1 << 41;
    /// For code segments, sets the segment as “conforming”, influencing the
    /// privilege checks that occur on control transfers. For 32-bit data segments,
    /// sets the segment as "expand down". In 64-bit mode, ignored for data segments.
    pub(crate) const CONFORMING: usize        = 1 << 42;
    /// This flag must be set for code segments and unset for data segments.
    pub(crate) const EXECUTABLE: usize        = 1 << 43;
    /// This flag must be set for user segments (in contrast to system segments).
    pub(crate) const USER_SEGMENT: usize      = 1 << 44;
    /// Must be set for any segment, causes a segment not present exception if not set.
    pub(crate) const PRESENT: usize           = 1 << 47;
    /// This bit is ignored and available for use by the Operating System
    pub(crate) const AVAILABLE: usize         = 1 << 52;
    /// Must be set for 64-bit code segments, unset otherwise.
    pub(crate) const LONG_MODE: usize         = 1 << 53;
    /// Use 32-bit (as opposed to 16-bit) operands. If [`LONG_MODE`][Self::LONG_MODE] is set,
    /// this must be unset. In 64-bit mode, ignored for data segments.
    pub(crate) const DEFAULT_SIZE: usize      = 1 << 54;
    /// Limit field is scaled by 4096 bytes. In 64-bit mode, ignored for all segments.
    pub(crate) const GRANULARITY: usize       = 1 << 55;

    /// Bits `0..=15` of the limit field (ignored in 64-bit mode)
    pub(crate) const LIMIT_0_15: usize        = build_bit_mask(0, 16);
    /// Bits `16..=19` of the limit field (ignored in 64-bit mode)
    pub(crate) const LIMIT_16_19: usize       = build_bit_mask(48, 4);
    /// Bits `0..=23` of the base field (ignored in 64-bit mode, except for fs and gs)
    pub(crate) const BASE_0_23: usize         = build_bit_mask(16, 24);
    /// Bits `24..=31` of the base field (ignored in 64-bit mode, except for fs and gs)
    pub(crate) const BASE_24_31: usize        = build_bit_mask(56, 8);

    #[repr(u8)]
    pub(crate) enum Ring {
        R0 = 0,
        R1 = 1,
        R2 = 2,
        R3 = 3,
    }

    /// These two bits encode the Descriptor Privilege Level (DPL) for this descriptor.
    /// If both bits are set, the DPL is Ring 3, if both are unset, the DPL is Ring 0.
    #[inline]
    pub(crate) const fn dpl(ring: Ring) -> usize {
        (ring as u8 as usize) << 45
    }

    pub(crate) mod tss {
        use crate::util::build_bit_mask;

        const BASE_0_23: usize = 16;
        const BASE_24_31: usize = 56;
        // this is in the second word, but the offset is to the left in this case
        const BASE_32_63: usize = 32;

        const RAW_0_23: usize = build_bit_mask(0, 24);
        const RAW_24_31: usize = build_bit_mask(24, 8);
        const RAW_32_63: usize = build_bit_mask(32, 32);

        #[repr(u64)]
        pub(crate) enum Type {
            LDT = 0x2 << 40,
            TSSAvailable = 0x9 << 40,
            TSSBusy = 0xB << 40,
        }

        #[inline]
        pub(crate) const fn base_from_addr(addr: *mut ()) -> (usize, usize) {
            let raw = addr as usize;
            (((raw & RAW_0_23) << BASE_0_23) | ((raw & RAW_24_31) << BASE_24_31), (raw & RAW_32_63) >> BASE_32_63)
        }

    }

}
