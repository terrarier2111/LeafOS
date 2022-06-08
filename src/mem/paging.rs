use crate::mem::addr::{PhysAddr, VirtAddr};
use crate::mem::frame::PhysFrame;
use crate::mem::mapped_page_table::{FrameAllocator, Mapper, MapToError, OffsetPageTable};
use crate::mem::page_table::{PageTable, PageTableFlags};
use crate::{print, println, utils, wait_for_interrupt};
use bitflags::bitflags;
use bootloader::bootinfo::{MemoryMap, MemoryRegion, MemoryRegionType};
use core::arch::asm;
use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::mem::{size_of, transmute};
use core::ops::{BitAnd, BitAndAssign, BitOrAssign, Range, Shl, Shr};
use core::ptr;
use core::ptr::{addr_of, addr_of_mut, slice_from_raw_parts};
use intrusive_collections::{LinkedList, SinglyLinkedList};
use spin::Mutex;
use x86::controlregs::{cr4, Cr4};
use x86::current::paging::{PAddr, PT};
use x86_64::registers::control::Cr4Flags;
use crate::mem::MAPPER;
use crate::mem::page::{Page, Size4KiB};

static mut LEVEL_5_PAGING: bool = false; // FIXME: Don't make this static mut, this was just out of laziness (make this an AtomicBool)

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

unsafe fn curr_top_level_page_table(mem_offset: u64) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (top_level_table_frame, _) = Cr3::read();

    let phys = top_level_table_frame.start_address();
    let virt = mem_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = ptr::from_exposed_addr_mut(virt as usize);

    &mut *page_table_ptr // unsafe
}

/// Initialize a new OffsetPageTable.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must only be called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
pub unsafe fn init(physical_memory_offset: u64) -> OffsetPageTable<'static> {
    // unsafe { LEVEL_5_PAGING = cr4().contains(Cr4::CR4_ENABLE_LA57) }; // FIXME: Readd this once our used bootloader supports 5 level paging
    let top_level_table = curr_top_level_page_table(physical_memory_offset);
    OffsetPageTable::new(top_level_table, VirtAddr::new(physical_memory_offset))
}

pub const PAGE_SIZE: usize = 4096;
const MAX_ORDER: usize = 9; // 2 ^ MAX_ORDER * PAGE_SIZE will be the size of the biggest blocks
const ORDERS: usize = MAX_ORDER + 1;

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
#[repr(C)]
pub struct BuddyFrameAllocator {
    map_offset: usize, // the offset in memory from 0 to where the frame allocator info is located at
    last_pgt_entry: usize, // the last page which is contained in the page table
    orders: [usize; ORDERS], // represents a list of addresses (in the compressed order format described below)
}

const USABLE_START: u64 = 1 * 1024 * 1024; // 1MB

// TODO: Instead of saving the frame data at the start of each frame itself, just have some frames at the beginning, which can be inserted into the paging structure
// TODO: and which can be used to save the frame information of all other frames in the system.

impl BuddyFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryMap, mapper: &Mutex<OffsetPageTable>) -> Self {
        let mut mapper = mapper.lock();
        let mut orders = [0; ORDERS];
        // FIXME: Replace memory_map with our own or the one of stivale2 https://wiki.osdev.org/Detecting_Memory_(x86)

        fn find_matching_order(usable_frames: u64) -> Option<usize> {
            for i in (0..ORDERS).rev() {
                if (1 << i) <= usable_frames {
                    return Some(i);
                }
            }
            None
        }

        let mut setup_alloc: SetupFrameAllocator<3> = SetupFrameAllocator::new(memory_map);

        // find the highest frame
        let mut highest_frame = 0;
        for entry in memory_map.iter() {
            if entry.region_type == MemoryRegionType::Usable && entry.range.end_frame_number > highest_frame {
                highest_frame = entry.range.end_frame_number;
            }
        }
        let required_frames = (highest_frame * 10).div_ceil(4096);

        // find an appropriate space to put our mapping data into
        let map_dest = {
            let mut ret = 0..0;
            for entry in memory_map.iter() {
                if entry.region_type == MemoryRegionType::Usable && required_frames <= (entry.range.end_frame_number - entry.range.start_frame_number) {
                    let mut start_frame_number = entry.range.start_frame_number;
                    // skip all entries below our defined minimum
                    if USABLE_START >= entry.range.start_frame_number * 4096 {
                        if USABLE_START >= entry.range.end_frame_number * 4096 {
                            continue;
                        }
                        start_frame_number = USABLE_START.div_ceil(4096);
                    }

                    ret = entry.range.start_frame_number..(start_frame_number + required_frames);
                    break;
                }
            }
            ret
        };
        if map_dest.end == 0 {
            // FIXME: Return error and report to user
            panic!("Initial paging setup error!");
        }

        // put our mapping data into the space we allocated
        for page in map_dest.clone() {
            setup_alloc.refill();
            let frame: PhysFrame<Size4KiB> = PhysFrame::from_start_address(PhysAddr::new(page * 4096)).unwrap();
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
            unsafe {
                mapper.map_to(Page::from_start_address(VirtAddr::new(page * 4096)).unwrap(), frame, flags, &mut setup_alloc).unwrap().flush() // FIXME: break 'start when unwrap fails instead of panicking!
            };
        }
        let map_offset = (map_dest.start * 4096) as usize;

        let mut usable_start = 0;
        let mut last_usable = 0;

        'start: for entry in memory_map.iter() {
            let mut start_frame_number = entry.range.start_frame_number;
            if USABLE_START >= entry.range.start_frame_number * 4096 {
                if USABLE_START >= entry.range.end_frame_number * 4096 {
                    continue;
                }
                start_frame_number = USABLE_START.div_ceil(4096);
            }
            if map_dest.start >= entry.range.start_frame_number {
                if map_dest.start < entry.range.end_frame_number {
                    start_frame_number = map_dest.end;
                }
            }
            let mut end_frame_number = entry.range.end_frame_number;
            if LAST_USABLE_FRAMES.start != u64::MAX && LAST_USABLE_FRAMES.end <= end_frame_number {
                end_frame_number = LAST_USABLE_FRAMES.end - 1;
            }
            if LAST_USABLE_FRAMES.start != u64::MAX && LAST_USABLE_FRAMES.end <= start_frame_number {
                break;
            }
            if entry.region_type == MemoryRegionType::Usable {
                if usable_start == 0 {
                    usable_start = start_frame_number;
                }

                last_usable = end_frame_number;
            } else if usable_start != 0 {
                // FIXME: Try using free mem range if possible
                while let Some(order) = find_matching_order(last_usable - usable_start) {

                    // range of memory which should be checked when marking frames as free later on depending on if the start address
                    // of them is included in these ranges or not, tho this memory range will be reduced because it includes the frames
                    // we want to ommit because we have too much free memory.
                    let entry_addr = usable_start as usize;

                    let mut prev = Self::entry_glob((orders[order] * 4096) as u64, map_offset);
                    orders[order] = entry_addr;
                    let entry_addr = Self::entry_glob((entry_addr * 4096) as u64, map_offset);
                    let entry_addr = entry_addr.as_mut().unwrap();
                    if !prev.is_null() {
                        let prev = prev.as_mut().unwrap();
                        prev.set_prev(entry_addr, map_offset);
                    }
                    entry_addr.set_next(prev, map_offset);
                    entry_addr.set_prev(ptr::null_mut(), map_offset);

                    let buddy_size = 4096_u64 * (1 << order);
                    let base = entry_addr.assoc_page(map_offset).expose_addr();

                    let left = unsafe { &*BuddyFrameAllocator::entry_glob(base as u64 - buddy_size, map_offset) };
                    if memory_map.is_usable(base - buddy_size as usize, 1 << order) && left.is_first() {
                        // the left entry is usable and this frame's buddy
                        entry_addr.set_has_neighbor();
                    } else if memory_map.is_usable(base + buddy_size as usize, 1 << order) {
                        // the right entry is usable and this frame's buddy
                        entry_addr.set_has_neighbor();
                        // this frame is before the next, so this is the first frame
                        entry_addr.set_is_first();
                    }

                    // println!("curr: {} | buddy: {}", entry_addr_raw, other_buddy(PhysAddr::new(entry_addr_raw), order).as_u64());
                    usable_start += 1 << order;
                }
                usable_start = 0;
            }
        }

        if usable_start != 0 {
            // FIXME: Try using free mem range if possible
            while let Some(order) = find_matching_order(last_usable - usable_start) {

                // range of memory which should be checked when marking frames as free later on depending on if the start address
                // of them is included in these ranges or not, tho this memory range will be reduced because it includes the frames
                // we want to ommit because we have too much free memory.
                let entry_addr = usable_start as usize;

                let mut prev = Self::entry_glob((orders[order] * 4096) as u64, map_offset);
                orders[order] = entry_addr;

                let entry_addr = Self::entry_glob((entry_addr * 4096) as u64, map_offset);
                let entry_addr = entry_addr.as_mut().unwrap();
                if !prev.is_null() {
                    let prev = prev.as_mut().unwrap();
                    prev.set_prev(entry_addr, map_offset);
                }
                entry_addr.set_next(prev, map_offset);
                entry_addr.set_prev(ptr::null_mut(), map_offset);

                let buddy_size = 4096_u64 * (1 << order);
                let base = entry_addr.assoc_page(map_offset).expose_addr();

                let left = unsafe { &*BuddyFrameAllocator::entry_glob(base as u64 - buddy_size, map_offset) };
                if memory_map.is_usable(base - buddy_size as usize, 1 << order) && left.is_first() {
                    // the left entry is usable and this frame's buddy
                    entry_addr.set_has_neighbor();
                } else if memory_map.is_usable(base + buddy_size as usize, 1 << order) {
                    // the right entry is usable and this frame's buddy
                    entry_addr.set_has_neighbor();
                    // this frame is before the next, so this is the first frame
                    entry_addr.set_is_first();
                }

                // println!("curr: {} | buddy: {}", entry_addr_raw, other_buddy(PhysAddr::new(entry_addr_raw), order).as_u64());
                usable_start += 1 << order;
            }
        }

        // One pointer to the next free page inside each page suffices (for allocation) because to remove an allocated page from its parent
        // we simply do 2 next calls and mutate the page we get from the first next call to point to the 2nd page's successor
        // FIXME: What about merging of buddies? do we have to do a O(n) search of everything that comes before our desired merging page?
        // we could fix this by storing a doubly linked list instead of a singly linked one, this **should** be okay because we only need
        // 52 * 2 = 104 bits and because we know that our destination is another 4096 byte aligned address as well we should be able to
        // to cut this down by 13 * 2 bits because we can regain all information by clever multiplication and division with 4096
        // so this results in 78 bits we need, or in other terms 10 bytes also we still have 2 spare bits we can use for whatever we like
        // furthermore we should be able to easily compute one of the two addresses by simple bit masking and bit shifting,
        // the other one is a bit trickier to obtain, but should still be easy enough.
        // we should probably prefer allocation over deallocation because generally it is more likely that memory usage increases over time, so
        // there are slightly more allocations than deallocations and especially on program startups, massive amounts of memory will be allocated
        // so to improve startup times we should prefer allocations over deallocations, furthermore usually program startup times are way more important
        // "program stop times"
        // FIXME: BUT HOW TF SHOULD LARGER ALLOCATIONS (more than 1 page) WORK IF WE HAVE METADATA CONTAINED INSIDE PAGES?
        // could we just save necessary metadata at the beginning of each allocation and not every page?
        // i.e each allocation no matter what order should only ever contain metadata at the beginning
        // FIXME: How can we avoid having to insert every page into the pagetable to make it writable in order to write its linked list entries to it (and waste memory)?
        // We can simply allocate the required space (highest_usable_frame * 10) bytes in a couple of adjacent pages at the beginning of usable ram, so we don't have to
        // worry about every page being writable and we can simply make our "map" pages writable.

        // entry layout:
        // 39 bits: next entry data
        // 1 bit: flag whether or not this page has an usable neighbor
        // 39 bits: prev entry data
        // 1 bit: flag whether the current page is the first entry (of the two bodies) or not
/*
        for order in 0..ORDERS {
            let mut tmp = ptr::null_mut();
            let mut current = Self::entry_glob((orders[order] * 4096) as u64, map_offset);

            /* swap next and prev for all nodes of
             doubly linked list */
            while !current.is_null() {
                let derefed_current = unsafe {&mut *current };
                tmp = derefed_current.get_prev(map_offset);
                let new_prev = derefed_current.get_next(map_offset);
                derefed_current.set_prev(new_prev, map_offset);
                derefed_current.set_next(tmp, map_offset);
                current = new_prev;
            }

            /* Before changing head, check for the cases like
             empty list and list with only one node */
            if !tmp.is_null() {
                let prev = unsafe { &mut *tmp }.get_prev(map_offset);
                orders[order] = unsafe { &mut *prev }.assoc_page(map_offset).expose_addr() / 4096;
            }
        }*/


        Self { map_offset, last_pgt_entry: map_offset, orders }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self {
            map_offset: 0,
            last_pgt_entry: 0,
            orders: [0; ORDERS],
        }
    }

    /// Safety:
    /// `page_address` has to be a valid address to an unused page in memory.
    unsafe fn entry(&self, page_address: u64) -> *mut MapEntry {
        Self::entry_glob(page_address, self.map_offset)
    }

    /// Safety:
    /// `page_address` has to be a valid address to an unused page in memory.
    fn entry_glob(page_address: u64, map_offset: usize) -> *mut MapEntry {
        if page_address != 0 {
            let meta_addr = map_offset + (page_address.div_floor(4096) * 10) as usize; // FIXME: we probably gotta subtract the mapoffset somewhere - DO THAT!
            ptr::from_exposed_addr_mut::<MapEntry>(meta_addr)
        } else {
            println!("got zero param in entry func!");
            ptr::null_mut()
        }
    }

    fn entry_next_ptr(map_offset: usize, entry_addr: usize) -> *mut u8 {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        const MASK: usize = (1 << 39) - 1; // the 39 lower bits are set
        let metadata_part = unsafe { *(&*ptr::from_exposed_addr(dst) as &usize) };
        let link = (metadata_part & MASK) * 4096;
        link as *mut u8
    }

    fn entry_next_raw<const ORDER_ENTRY: bool>(map_offset: usize, raw_entry_addr: usize) -> usize {
        let dst = map_offset + (raw_entry_addr * 10);
        const MASK: usize = (1 << 39) - 1; // the 39 lower bits are set
        let metadata_part = unsafe { *(&*ptr::from_exposed_addr(dst) as &usize) };
        if ORDER_ENTRY {
            metadata_part
        } else {
            metadata_part & MASK
        }
    }

    #[inline]
    fn entry_prev_ptr(map_offset: usize, entry_addr: usize) -> *mut u8 {
        Self::entry_next_ptr(map_offset, entry_addr + 5)
    }

    #[inline]
    fn entry_has_neighbor(map_offset: usize, entry_addr: usize) -> bool {
        Self::entry_meta_first(map_offset, entry_addr) != 0
    }

    fn entry_meta_first(map_offset: usize, entry_addr: usize) -> usize {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        const MASK: usize = 1 << 39;
        let metadata_part = unsafe { *(&*ptr::from_exposed_addr(dst) as &usize) };
        let meta = metadata_part & MASK;
        meta >> 39
    }

    #[inline]
    fn entry_meta_second(map_offset: usize, entry_addr: usize) -> usize {
        Self::entry_meta_first(map_offset, entry_addr + 5)
    }

    #[inline]
    fn entry_meta_full(map_offset: usize, entry_addr: usize) -> usize {
        Self::entry_meta_first(map_offset, entry_addr) | (Self::entry_meta_second(map_offset, entry_addr) << 1)
    }

    #[inline(always)]
    fn write_entry_has_neighbor(map_offset: usize, entry_addr: usize, has_neighbor: bool) {
        Self::write_entry_meta_first(map_offset, entry_addr, has_neighbor);
    }

    #[inline]
    fn write_entry_meta_first(map_offset: usize, entry_addr: usize, val: bool) {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        let metadata_part = unsafe { &mut *ptr::from_exposed_addr_mut(dst) as &mut usize };
        let val = unsafe { transmute::<bool, u8>(val) } as usize;
        *metadata_part |= val << 39;
        *metadata_part &= !(val << 39);
    }

    #[inline(always)]
    fn set_entry_has_neighbor(map_offset: usize, entry_addr: usize) {
        Self::set_entry_meta_first(map_offset, entry_addr);
    }

    #[inline]
    fn set_entry_meta_first(map_offset: usize, entry_addr: usize) {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        const MASK: usize = 1 << 39;
        let metadata_part = unsafe { &mut *ptr::from_exposed_addr_mut(dst) as &mut usize };
        *metadata_part |= MASK;
    }

    fn write_entry_next<const RAW: bool>(map_offset: usize, entry_addr: usize, next_entry_addr: usize) {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        let metadata_part = {
            let mut base = if RAW {
                next_entry_addr
            } else {
                next_entry_addr / 4096
            };
            // let meta = unsafe { *(&*ptr::from_exposed_addr(dst + 4) as &u32) };
            // base | (((meta >> 7) as usize) << 39) // only keep the last 3 bytes (and one bit extra metadata)
            // const MASK: usize = !((1 << 39) - 1);
            // let meta = unsafe { *(&*ptr::from_exposed_addr(dst) as &usize) };
            // (meta & MASK) | base
            const MASK: u32 = !(u8::MAX as u32);
            let meta = unsafe { *(&*ptr::from_exposed_addr(dst + 4) as &u32) }; // skip the first four bytes as they only contain the old address
            let meta = (meta & MASK) as usize; // skip the fifth byte using masking as it only contains the old address
            let meta = meta << 32; // skip the first 32 bits as they should only contain zeros (our first 8 bits already contain zeros, so we shift 32 instead of 40)
            // FIXME: there is a bug in here! we have to retain the metadata bit! (maybe there are other bugs in here)
            meta | base
        };
        println!("to_fin: {} | value: {} final_val: {}", dst, next_entry_addr, metadata_part);
        // let metadata_part_addr = unsafe { &mut *ptr::from_exposed_addr_mut(dst) as &mut usize };
        unsafe { ptr::write_volatile::<usize>(ptr::from_exposed_addr_mut(dst), metadata_part); }
        // *metadata_part_addr = metadata_part;
    }

    #[inline]
    fn write_entry_prev<const RAW: bool>(map_offset: usize, entry_addr: usize, prev_entry_addr: usize) {
        Self::write_entry_next::<RAW>(map_offset, entry_addr + 5, prev_entry_addr)
       /* let entry_addr = entry_addr + 5;
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        let metadata_part = {
            let mut base = if RAW {
                prev_entry_addr
            } else {
                prev_entry_addr / 4096
            };
            // let meta = unsafe { *(&*ptr::from_exposed_addr(dst + 4) as &u32) };
            // base | (((meta >> 7) as usize) << 39) // only keep the last 3 bytes (and one bit extra metadata)
            // const MASK: usize = !((1 << 39) - 1);
            // let meta = unsafe { *(&*ptr::from_exposed_addr(dst) as &usize) };
            // (meta & MASK) | base
            const MASK: u32 = !(u8::MAX as u32);
            let meta = unsafe { *(&*ptr::from_exposed_addr(dst + 4) as &u32) }; // skip the first four bytes as they only contain the old address
            let meta = (meta & MASK) as usize; // skip the fifth byte using masking as it only contains the old address
            let meta = meta << 32; // skip the first 32 bits as they should only contain zeros (our first 8 bits already contain zeros, so we shift 32 instead of 40)
            // FIXME: there is a bug in here! we have to retain the metadata bit! (maybe there are other bugs in here)
            meta | base
        };
        println!("to_fin: {} | value: {} final_val: {}", dst, next_entry_addr, metadata_part);
        // let metadata_part_addr = unsafe { &mut *ptr::from_exposed_addr_mut(dst) as &mut usize };
        unsafe { ptr::write_volatile::<usize>(ptr::from_exposed_addr_mut(dst), metadata_part); }*/
        // *metadata_part_addr = metadata_part;
    }

    fn is_free(map_offset: usize, entry_addr: usize) -> bool {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        let first_part = ptr::from_exposed_addr(dst) as *const u64;
        let second_part = ptr::from_exposed_addr(dst + 8) as *const u16;
        // we have to check both the entire next ptr and prev ptr in order to not get in trouble
        // if we are at the very last entry in the free list
        unsafe { *first_part == 0 && *second_part == 0 }
    }

    fn free_entry(map_offset: usize, entry_addr: usize) {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        let first_part = ptr::from_exposed_addr_mut(dst) as *mut u64;
        let second_part = ptr::from_exposed_addr_mut(dst + 8) as *mut u16;
        unsafe {
            *first_part = 0;
            *second_part = 0;
        }
    }

    fn inner_allocate_frame(&mut self) -> Option<PhysFrame> {
        self.allocate_frames(0).map(|addr| PhysFrame::containing_address(addr))
    }

    pub fn allocate_frames(&mut self, order: usize) -> Option<PhysAddr> {
        let mut curr_order = order;
        while MAX_ORDER > curr_order && self.orders[curr_order] == 0 {
            curr_order += 1;
        }
        if curr_order >= MAX_ORDER {
            return None;
        }

        let entry_raw = self.orders[curr_order] * 4096;
        let entry = unsafe { self.entry(entry_raw as u64).as_mut().unwrap() };

        // retrieve next entry and update its metadata
        let next_entry = entry.get_next(self.map_offset);
        if !next_entry.is_null() {
            self.orders[curr_order] = unsafe { next_entry.as_mut().unwrap().assoc_page(self.map_offset).expose_addr() / 4096 };
        } else {
            self.orders[curr_order] = 0;
        }
        if !next_entry.is_null() {
            let next_entry = unsafe { next_entry.as_mut().unwrap() };
            next_entry.set_prev(ptr::null_mut(), self.map_offset);
        }

        // Split up the buddy until we have the desired size
        while curr_order > order {
            curr_order -= 1;
            let buddy_size = 4096 * (1 << curr_order);
            let other = unsafe { &mut *self.entry((entry_raw + buddy_size) as u64) };
            other.free();
            println!("other: {} | order: {} | dist: {}", other.assoc_page(self.map_offset).expose_addr(), curr_order, entry_raw.abs_diff(other.assoc_page(self.map_offset).expose_addr()));
            self.orders[curr_order] = other.assoc_page(self.map_offset).expose_addr() / 4096; // convert into internal repr and replace current list head
        }

        // println!("curr: {} | curr_aligned: {}", entry_raw, other_buddy(PhysAddr::new(entry_raw as u64), order).as_u64());

        // FIXME: The issue here is that we are returning the same address which we are storing
        println!("allocated frame: {:?} | curr order: {} | order: {}", PhysAddr::new(entry_raw as u64), curr_order, order);
        println!("curr_val: {} | ret {}", self.orders[order] * 4096, entry_raw);

        Some(PhysAddr::new(entry_raw as u64))
    }

    // FIXME: FOUND UNSOUNDNESS: when creating a local of type Option<mut POINTER> and then calling unwrap_unchecked we get issues
    // FIXME: SOMETHING LIKE THIS BUT VOLATILE:
    /*
        let mut tmp = None;
        tmp = Some(ptr::null_mut());
        tmp.unwrap();
     */

    pub fn allocate_frames_tlb(&mut self, order: usize) -> Option<PhysAddr> {
        let frames = self.allocate_frames(order);
        let curr_start = frames.map_or(0, |x| x.as_u64() / 4096);
        let curr_end = curr_start + (1 << order);
        if curr_end > self.last_pgt_entry as u64 {
            for fc in (self.last_pgt_entry + 1)..(curr_end as usize + 1) { // FIXME: is here some off by one error?
                let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
                let mut mapper = MAPPER.lock();
                let page: Page<Size4KiB> =
                    Page::from_start_address(VirtAddr::new(fc as u64 * 4096)).unwrap();
                println!("mapped: {:?}", VirtAddr::new(fc as u64 * 4096));
                let phys_frame: PhysFrame<Size4KiB> =
                    PhysFrame::from_start_address(PhysAddr::new(fc as u64 * 4096)).unwrap();
                unsafe {
                    mapper
                        .map_to::<BuddyFrameAllocator>(page, phys_frame, flags, self)
                        .unwrap()
                        .flush()
                };
            }
        }
        // self.last_pgt_entry = (curr_end - 1) as usize; // FIXME: is here some off by one error?
        self.last_pgt_entry = curr_end as usize; // FIXME: is here some off by one error?
        frames
    }

    pub fn deallocate_frame(&mut self, address: PhysAddr) {
        println!("deallocated frame: {:?}", address);
        self.deallocate_frames(address, 0);
    }

    pub fn order_from_size(size: usize) -> usize {
        let frames = size.div_ceil(4096);
        for i in 0..ORDERS {
            if (1 << i) > frames { // FIXME: should this be `>=`?
                return i;
            }
        }
        MAX_ORDER
    }

    pub fn deallocate_frames(&mut self, address: PhysAddr, order: usize) {
        // FIXME: FIX THIS METHOD - CURRENTLY IT DOESN'T WORK AT ALL!
        println!("deallocating!");
        let entry_raw = unsafe { self.entry(address.as_u64()) };
        let entry = unsafe { entry_raw.as_mut().unwrap() }; // FIXME: do some sanity checking!
        // Self::free_entry(self.map_offset, address.as_u64() as usize);
        entry.free();
        let mut new_order = order;
        while MAX_ORDER > order {
            if !entry.has_neighbor()/* || !other_buddy.is_free()*/ {
                break;
            }
            let offset = 4096 * (1 << order);
            // let other_buddy = other_buddy(address, order).as_u64();
            let other_addr = if entry.is_first() {
                address.as_u64() + offset
            } else {
                address.as_u64() - offset
            };
            let other_buddy = unsafe { self.entry(other_addr).as_mut().unwrap() };
            /*if !Self::entry_has_neighbor(self.map_offset, address.as_u64() as usize) || !Self::is_free(self.map_offset, other_buddy.as_u64() as usize) {
                break;
            }*/
            if !other_buddy.is_free() {
                break;
            }
            // Self::free_entry(self.map_offset, other_buddy.as_u64() as usize);
            new_order += 1;
        }
        let next = self.orders[new_order] * 4096;
        self.orders[new_order] = entry.assoc_page(self.map_offset).expose_addr() / 4096;
        let next = unsafe { self.entry(next as u64) };
        if !next.is_null() {
            entry.set_next(next, self.map_offset);
            unsafe { &mut *next }.set_prev(entry_raw, self.map_offset);
        }
        // FIXME: Actually fix deallocation!
    }
}

// FIXME: Try switching to using this struct instead of manually doing bit and pointer magic every time we need to modify stuff
// #[align(1)] // FIXME: Is this actually the most optimal way of aligning the data?
// #[repr(C, packed(2))]
#[repr(C, packed(1))]
struct MapEntry {
    first_data: u64,
    second_data: u16,
}

impl MapEntry {

    /// `next` should be a 'normal' address
    fn set_next(&mut self, next: *mut MapEntry, map_offset: usize) {
        const EXTRA_DATA_MASK: u64 = !((1 << 39) - 1);
        let next = if !next.is_null() {
            (next.expose_addr() - map_offset) as u64
        } else {
            0
        };
        let other = self.first_data & EXTRA_DATA_MASK;
        self.first_data = next.div_floor(10/*4096*/) | other;
    }

    fn set_prev(&mut self, prev: *mut MapEntry, map_offset: usize) {
        const FIRST_EXTRA_DATA_MASK: u64 = (1 << 40) - 1; // the bits from 1 to 40
        const SECOND_EXTRA_DATA_MASK: u16 = 1 << 15; // the 16th bit
        const PREV_DATA_FIRST_MASK: u64 = (1 << 24) - 1; // the bits from 1 to 24
        const PREV_DATA_SECOND_MASK: u64 = ((1 << 15) - 1) << 24; // the bits from 25 to 39

        let prev = if !prev.is_null() {
            (prev.expose_addr() - map_offset) as u64
        } else {
            0
        };
        // FIXME: Try improving the performance of this using raw pointers! (if this actually works)
        let prev = prev.div_floor(10/*4096*/); // FIXME: There is probably a fatal flaw in how we do divisions and multiplications with 4096 and 10
        let other = self.first_data & FIRST_EXTRA_DATA_MASK;
        self.first_data = ((prev & PREV_DATA_FIRST_MASK) << 40) | other;
        let other = self.second_data & SECOND_EXTRA_DATA_MASK;
        self.second_data = ((prev & PREV_DATA_SECOND_MASK) >> 24) as u16 | other;
    }

    fn get_next(&self, map_offset: usize) -> *mut MapEntry {
        const MASK: u64 = (1 << 39) - 1;
        let raw = (self.first_data & MASK) as usize * 10;
        if raw != 0 {
            ptr::from_exposed_addr_mut::<MapEntry>(map_offset + raw)
        } else {
            ptr::null_mut()
        }
    }

    fn get_prev(&self, map_offset: usize) -> *mut MapEntry {
        const FIRST_DATA_MASK: u64 = !((1 << 40) - 1); // the bits from 41 to 64
        const SECOND_DATA_MASK: u16 = (1 << 15) - 1; // the bits from 1 to 15
        let first = (self.first_data & FIRST_DATA_MASK) >> 40;
        let second = (self.second_data & SECOND_DATA_MASK) as u64;
        let raw = (first | (second << 24)) as usize * 10;
        if raw != 0 {
            ptr::from_exposed_addr_mut::<MapEntry>(map_offset + raw)
        } else {
            ptr::null_mut()
        }
    }

    fn set_has_neighbor(&mut self) {
        const MASK: u64 = 1 << 39;
        self.first_data |= MASK;
    }

    fn has_neighbor(&self) -> bool {
        const MASK: u64 = 1 << 39;
        (self.first_data & MASK) != 0
    }

    fn set_is_first(&mut self) {
        const MASK: u16 = 1 << 15;
        self.second_data |= MASK;
    }

    fn is_first(&self) -> bool {
        const MASK: u16 = 1 << 15;
        (self.second_data & MASK) != 0
    }

    fn free(&mut self) {
        const FIRST_MASK: u64 = 1 << 39;
        const SECOND_MASK: u16 = 1 << 15;
        self.first_data &= FIRST_MASK;
        self.second_data &= SECOND_MASK;
    }

    fn is_free(&self) -> bool {
        const FIRST_MASK: u64 = !(1 << 39);
        const SECOND_MASK: u16 = !(1 << 15);
        (self.first_data & FIRST_MASK) == 0 && (self.second_data & SECOND_MASK) == 0
    }

    fn assoc_page(&self, map_offset: usize) -> *mut u8 {
        ptr::from_exposed_addr_mut::<u8>((addr_of!(self.first_data).expose_addr() - map_offset) / 10 * 4096)
    }

}

unsafe impl FrameAllocator<Size4KiB> for BuddyFrameAllocator {
    #[inline]
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.inner_allocate_frame()
    }
}

static mut LAST_USABLE_FRAMES: Range<u64> = u64::MAX..u64::MAX; // FIXME: Don't make this static mut, this was just out of laziness (make this a local variable and pass it through via &mut)

struct SetupFrameAllocator<const ENTRIES: usize> {
    frames: [u64; ENTRIES],
    next: usize,
    memory_map: &'static MemoryMap,
}

unsafe impl<const ENTRIES: usize> FrameAllocator<Size4KiB> for SetupFrameAllocator<ENTRIES> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if self.next >= ENTRIES {
            return None;
        }
        let ret = self.frames[self.next];
        self.next += 1;
        Some(PhysFrame::from_start_address(PhysAddr::new((ret * 4096) as u64)).unwrap())
    }
}

impl<const ENTRIES: usize> SetupFrameAllocator<ENTRIES> {

    #[inline]
    fn new(memory_map: &'static MemoryMap) -> Self {
        Self {
            frames: [0; ENTRIES],
            next: ENTRIES,
            memory_map
        }
    }

    fn refill(&mut self) {
        if self.next != 0 {
            let usable_frames = unsafe { LAST_USABLE_FRAMES.clone() };
            if (usable_frames.end - usable_frames.start) < self.next as u64 {
                for frame in usable_frames.rev() {
                    self.frames[self.next - 1] = frame;
                    self.next -= 1;
                }
                let mut last_range = u64::MAX..u64::MAX;
                for entry in self.memory_map.iter() {

                    if entry.region_type == MemoryRegionType::Usable && entry.range.end_frame_number < unsafe { LAST_USABLE_FRAMES.start }
                    && (entry.range.start_frame_number > last_range.end || last_range.start == u64::MAX) {
                        last_range = entry.range.start_frame_number..entry.range.end_frame_number;
                    }
                }
                unsafe { LAST_USABLE_FRAMES = last_range };

                // call refill again to refill the missing frames which couldn't be refilled in this run
                self.refill();
            } else {
                for frame in ((usable_frames.end - self.next as u64)..usable_frames.end).rev() {
                    self.frames[self.next - 1] = frame;
                }
                unsafe { LAST_USABLE_FRAMES.end -= self.next as u64 };
                self.next = 0;
            }
        }
    }

}

/// This function should be used after the mappings are initialized.
/// `order` is the order of each buddy (probably - CHECK THIS!)
fn other_buddy(curr_buddy: PhysAddr, order: usize) -> PhysAddr {
    // FIXME: Maybe this doesn't get the other buddy properly!
    let buddy_size = 4096_u64 * (1 << order);
    let base = curr_buddy.align_down(buddy_size * 2);

    if base == curr_buddy {
        println!("added");
        curr_buddy + buddy_size
    } else {
        println!("didn't add");
        base
    }
}

// FIXME: Linux only ever moves 2 smaller buddies into one bigger one if both smaller ones are usable by simply treating the unusable buddy as already used

trait MemoryMapFunctions {

    fn is_usable(&self, addr: usize, frames: usize) -> bool;

}

impl MemoryMapFunctions for MemoryMap {
    fn is_usable(&self, addr: usize, frames: usize) -> bool {
        let regions = unsafe { &*slice_from_raw_parts(self.as_ptr(), 64) as &[MemoryRegion] };
        let idx = search_length_limited_nearest(regions, addr as u64 / 4096, regions.len());
        for x in 0..frames {
            if regions[idx + x].region_type != MemoryRegionType::Usable {
                return false;
            }
        }
        true
    }
}

pub fn search_length_limited_nearest(container: &[MemoryRegion], target: u64, length: usize) -> usize {
    let mut curr_pos = length / 2;
    let mut step_size = length / 4;
    let mut adapted = false;
    loop {
        if container[curr_pos].range.start_frame_number > target {
            if step_size == 0 {
                if !adapted && curr_pos != 0 && curr_pos != length - 1 {
                    adapted = true;
                    step_size += 1;
                } else {
                    if container[curr_pos].range.start_frame_number > target {
                        return curr_pos - 1;
                    }
                    return curr_pos;
                }
            }
            curr_pos -= step_size;
            step_size /= 2;
        } else if container[curr_pos].range.start_frame_number < target {
            if step_size == 0 {
                if !adapted && curr_pos != 0 && curr_pos != length - 1 {
                    adapted = true;
                    step_size += 1;
                } else {
                    if container[curr_pos].range.start_frame_number > target {
                        return curr_pos - 1;
                    }
                    return curr_pos;
                }
            }
            curr_pos += step_size;
            step_size /= 2;
        } else {
            return curr_pos;
        }
    }
}
