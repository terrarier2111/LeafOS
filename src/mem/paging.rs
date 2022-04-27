use crate::mem::addr::{PhysAddr, VirtAddr};
use crate::mem::frame::PhysFrame;
use crate::mem::mapped_page_table::OffsetPageTable;
use crate::mem::page_table::PageTable;
use crate::memory;
use bitflags::bitflags;
use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use core::arch::asm;
use core::borrow::{Borrow, BorrowMut};
use core::ops::Range;
use core::ptr;
use intrusive_collections::{LinkedList, SinglyLinkedList};
use x86::controlregs::{cr4, Cr4};
use x86::current::paging::{PAddr, PT};
use x86_64::registers::control::Cr4Flags;

static mut LEVEL_5_PAGING: bool = false;

bitflags! {
    /// Controls cache settings for the highest-level page table.
    ///
    /// Unused if paging is disabled or if [`PCID`](Cr4Flags::PCID) is enabled.
    pub struct Cr3Flags: u64 {
        /// Use a writethrough cache policy for the table (otherwise a writeback policy is used).
        const PAGE_LEVEL_WRITETHROUGH = 1 << 3;
        /// Disable caching for the table.
        const PAGE_LEVEL_CACHE_DISABLE = 1 << 4;
    }
}

fn read_cr3() -> (PAddr, Cr3Flags) {
    let value: u64;

    unsafe {
        asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }

    (
        PAddr::from(value & (!0xFFF)),
        Cr3Flags::from_bits(value & 0xFFF).unwrap(),
    )
}

#[inline(always)]
pub fn level_5_paging() -> bool {
    unsafe { LEVEL_5_PAGING }
}

unsafe fn curr_top_level_page_table(mem_offset: u64) -> &'static mut PageTable /*&'static mut PT*/ {
    /*
    // FIXME: Do we have to do any alignment adjustments?
    let (phys_pt, _) = read_cr3();
    let virt_pt = mem_offset + phys_pt.as_u64();
    let pt: *mut PT = ptr::from_exposed_addr_mut(virt_pt as usize);
    pt.as_mut().unwrap()*/
    use x86_64::registers::control::Cr3;

    let (top_level_table_frame, _) = Cr3::read();

    let phys = top_level_table_frame.start_address();
    let virt = mem_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr // unsafe
}

/// Initialize a new OffsetPageTable.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must only be called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
pub unsafe fn init(physical_memory_offset: u64) -> OffsetPageTable<'static> {
    unsafe { LEVEL_5_PAGING = cr4().contains(Cr4::CR4_ENABLE_LA57) };
    let top_level_table = curr_top_level_page_table(physical_memory_offset);
    OffsetPageTable::new(top_level_table, VirtAddr::new(physical_memory_offset))
}

const PAGE_SIZE: usize = 4096;
const MAX_ORDER: usize = 10; // 2 ^ MAX_ORDER * PAGE_SIZE will be the size of the biggest blocks

struct FreeArea {
    pub list: SinglyLinkedList<u64>,
    map: u64, // bitmap which can be used to figure out which entries are used and which are free
}

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct DefaultFrameAllocator {
    memory_map: &'static MemoryMap,
    order_maps: [&'static mut [u64]; MAX_ORDER], // bit maps for different orders to determine which pages are still free
}

impl DefaultFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        let mut req_mem = [0; MAX_ORDER]; // FIXME: Do we need to make this static to not dynamically allocate memory

        // FIXME: Replace memory_map with our own or the one of stivale2 https://wiki.osdev.org/Detecting_Memory_(x86)
        /*let mut orders = [FreeArea {
            list: SinglyLinkedList::new(0), // FIXME: Can we use this with a default entry or should we somehow use no default entry?
            map: 0,
        }; MAX_ORDER];*/
        let mut last_entry = 0;
        for entry in memory_map.iter() {
            if entry.range.start_addr() > last_entry {
                last_entry = entry.range.start_addr();
            }
        }
        let frame_count = last_entry.div_ceil(4096); // we have to make sure we don't allocate too little memory (too much is okay), so we ceil

        req_mem[0] = frame_count / 64 + 1; // we have to make sure we don't allocate too little memory (too much is okay), so we add 1
        for i in 1..MAX_ORDER {
            req_mem[i] = req_mem[i - 1] / 2 + 1; // we have to make sure we don't allocate too little memory (too much is okay), so we add 1
        }
        let mut order_maps: [&'static mut [u64]; MAX_ORDER] =
            [ptr::null_mut() as &'static mut [u64]; MAX_ORDER];
        let mut found_orders = 0_u64; // FIXME: Replace this with some other solution if we ever want to use more than 64 orders
        let final_orders = *0_u64.set_bits(0..9);

        let mut used = SinglyLinkedList::new(0_u64..0_u64);
        let mut usable_entries = 0; // FIXME: Is this needed?
        let mut usable_start = 0;

        fn find_matching_order(req_mem: &mut [u64; MAX_ORDER], usable_mem: u64, found_orders: u64) -> Option<usize> {
            for req in req_mem.iter().enumerate() {
                if found_orders & (1 << req.0) == 0 && *req.1 < usable_mem {
                    return Some(req.0);
                }
            }
            None
        }

        'start: for entry in memory_map.iter() {
            if entry.region_type == MemoryRegionType::Usable {
                if usable_start == 0 {
                    usable_start = entry.range.start_addr();
                } else {
                    usable_entries += 1;
                }
            } else if usable_start != 0 {
                // FIXME: Try using free mem range if possible
                let usable_mem = (entry.range.start_addr() - 1) - usable_start;
                while let Some(order) = find_matching_order(&mut req_mem, usable_mem, found_orders) {
                    let extra_mem = usable_mem - req_mem[order];
                    // extra frames we can leave as they are
                    let extra_frames = extra_mem.div_floor(4096);

                    // range of memory which should be checked when marking frames as free later on depending on if the start address
                    // of them is included in these ranges or not, tho this memory range will be reduced because it includes the frames
                    // we want to ommit because we have too much free memory.
                    let max_mem_range = usable_start..(entry.range.start_addr() - 4096);
                    let used_mem_range = max_mem_range.start..(max_mem_range.end - (extra_frames * 4096));
                    let used_end = used_mem_range.end;
                    order_maps[order] = ptr::from_exposed_addr_mut(used_mem_range.start as usize) as &'static mut [u64];
                    used.borrow_mut().push_front(used_mem_range);
                    found_orders |= (1 << order);
                    if found_orders == final_orders {
                        break 'start;
                    }
                    usable_start = used_end + 4096;
                }
                usable_start = 0;
            }
        }

        if found_orders != final_orders && usable_start != 0 {
            let usable_mem = (entry.range.start_addr() - 1) - usable_start;
            while let Some(order) = find_matching_order(&mut req_mem, usable_mem, found_orders) {
                let extra_mem = usable_mem - req_mem[order];
                // extra frames we can leave as they are
                let extra_frames = extra_mem.div_floor(4096);

                // range of memory which should be checked when marking frames as free later on depending on if the start address
                // of them is included in these ranges or not, tho this memory range will be reduced because it includes the frames
                // we want to ommit because we have too much free memory.
                let max_mem_range = usable_start..(entry.range.start_addr() - 4096);
                let used_mem_range = max_mem_range.start..(max_mem_range.end - (extra_frames * 4096));
                let used_end = used_mem_range.end;
                order_maps[order] = ptr::from_exposed_addr_mut(used_mem_range.start as usize) as &'static mut [u64];
                used.borrow_mut().push_front(used_mem_range);
                found_orders |= (1 << order);
                if found_orders == final_orders {
                    break;
                }
                usable_start = used_end + 4096;
            }
        }

        'start: for entry in memory_map.iter() {
            if entry.region_type == MemoryRegionType::Usable {
                for used in used.borrow().iter() {
                    let used: &Range<u64> = used;
                    if used.contains(&entry.range.start_addr()) {
                        continue 'start;
                    }
                }
                // FIXME: Insert into correct order!
            }
        }

        // One pointer to the next free page inside each page suffices (for allocation) because to remove an allocated page from its parent
        // we simply do 2 next calls and mutate the page we get from the first next call to point to the 2nd page's successor
        // FIXME: What about merging of buddies? do we have to do a O(n) search of everything that comes before our desired merging page?
        // we could fix this by storing a doubly linked list instead of a singly linked one, this **should** be okay because we only need
        // 52 * 2 = 104 bits and because we know that our destination is another 4096 byte aligned address as well we should be able to
        // to cut this down by 13 * 2 bits because we can regain all information by clever multiplication and division with 4096
        // so this results in 78 bits we need, or in other terms 10 bytes also we still have 2 spare bits we can use for whatever we like
        // (or keep for future address expansions the x86 architecture might receive) furthermore we should be able to easily compute one
        // of the two addresses by simple bit masking and bit shifting, the other one is a bit trickier to obtain, but should still be easy enough
        // we should probably prefer allocation over deallocation because generally it is more likely that memory usage increases over time, so
        // there are slightly more allocations than deallocations and especially on program startups, massive amounts of memory will be allocated
        // so to improve startup times we should prefer allocations over deallocations, furthermore usually program startup times are way more important
        // "program stop times"

        // page layout:
        // 78 bits: doubly linked list data
        // 2 bits: unused, reserved for future usage
        // 4086 * 8 bits: reserved for allocator usage

        Self { memory_map, order_maps }
    }

    /*fn mark_used(&mut self, index: u64, order: u64, area: &mut FreeArea) {
        // __change_bit((index) >> (1+(order)), (area)->map)
        area.map ^= index >> (1 + order);
    }*/

    /// Returns an iterator over the usable frames specified in the memory map.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
        // transform to an iterator of frame start addresses
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }

    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        self.allocate_frames(0)
    }

    fn allocate_frames(&mut self, order: usize) -> Option<PhysFrame> {
        let mut curr_order = order;
        while MAX_ORDER > curr_order && self.orders[curr_order].list.borrow().is_empty() {
            curr_order += 1;
        }
        if curr_order == MAX_ORDER {
            return None;
        }

        None
    }
}

/// Conceptually this represents splitting a parent buddy into two smaller ones (children)
/// This function returns the address of the upper buddy, the lower buddy
/// is located at `base`.
///
/// `order` is the order of the parent buddy
fn split_buddy(base: PhysAddr, order: u64) -> PhysAddr {
    base + (2 << (order - 1))
}

/*fn other_buddy(curr_buddy: PhysAddr, order: u64) -> PhysAddr {

}*/

// FIXME: Linux only ever moves 2 smaller buddies into one bigger one if both smaller ones are useable by simply treating the unusable buddy as already usedy

pub fn setup(
    memory_map: &'static MemoryMap,
    physical_memory_offset: u64,
) -> (OffsetPageTable, DefaultFrameAllocator) {
    // initialize a mapper
    let mut mapper = unsafe { init(physical_memory_offset) };
    let mut frame_allocator = unsafe { DefaultFrameAllocator::init(memory_map) };
    crate::allocators::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    (mapper, frame_allocator)
}
