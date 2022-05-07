use crate::mem::addr::{PhysAddr, VirtAddr};
use crate::mem::frame::PhysFrame;
use crate::mem::mapped_page_table::{FrameAllocator, Mapper, MapToError, OffsetPageTable};
use crate::mem::page_table::{PageTable, PageTableFlags};
use crate::{println, utils};
use bitflags::bitflags;
use bootloader::bootinfo::{MemoryMap, MemoryRegion, MemoryRegionType};
use core::arch::asm;
use core::borrow::{Borrow, BorrowMut};
use core::cmp::Ordering;
use core::mem::{size_of, transmute};
use core::ops::{BitAnd, BitAndAssign, BitOrAssign, Range, Shl, Shr};
use core::ptr;
use core::ptr::slice_from_raw_parts;
use intrusive_collections::{LinkedList, SinglyLinkedList};
use x86::controlregs::{cr4, Cr4};
use x86::current::paging::{PAddr, PT};
use x86_64::registers::control::Cr4Flags;
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
pub struct DefaultFrameAllocator {
    memory_map: &'static MemoryMap,
    map_offset: usize, // the offset in memory from 0 to where the frame allocator info is located at
    orders: [usize; ORDERS], // represents a list of addresses (in the compressed order format described below)
}

const USABLE_START: u64 = 1 * 1024 * 1024; // 1MB

// TODO: Instead of saving the frame data at the start of each frame itself, just have some frames at the beginning, which can be inserted into the paging structure
// TODO: and which can be used to save the frame information of all other frames in the system.

impl DefaultFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryMap, mapper: &mut OffsetPageTable) -> Self {
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

        let mut highest_frame = 0;
        for entry in memory_map.iter() {
            if entry.region_type == MemoryRegionType::Usable && entry.range.end_frame_number > highest_frame {
                highest_frame = entry.range.end_frame_number;
            }
        }
        let required_frames = (highest_frame * 10).div_ceil(4096);

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
        }

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

                    let prev = orders[order] * 4096;
                    orders[order] = entry_addr;
                    let entry_addr = entry_addr * 4096;
                    if prev != 0 {
                        Self::write_entry_prev::<false>(map_offset, prev, entry_addr);
                    }
                    Self::write_entry_next::<false>(map_offset, entry_addr, prev);

                    if memory_map.is_usable(other_buddy(PhysAddr::new(entry_addr as u64), order).as_u64() as usize) {
                        Self::set_entry_has_neighbor(map_offset, entry_addr);
                    }

                    usable_start += 1 << order;
                }
                usable_start = 0;
            }
        }

        // FIXME: Fix the size calculation as not every region is frame sized but we are currently assuming that (probably)
        if usable_start != 0 {
            // FIXME: Try using free mem range if possible
            while let Some(order) = find_matching_order(last_usable - usable_start) {
                // range of memory which should be checked when marking frames as free later on depending on if the start address
                // of them is included in these ranges or not, tho this memory range will be reduced because it includes the frames
                // we want to ommit because we have too much free memory.
                let entry_addr = usable_start as usize;

                let prev = orders[order] * 4096;
                orders[order] = entry_addr;
                let entry_addr = entry_addr * 4096;
                if prev != 0 {
                    Self::write_entry_prev::<false>(map_offset, prev, entry_addr);
                }
                Self::write_entry_next::<false>(map_offset, entry_addr, prev);

                if memory_map.is_usable(other_buddy(PhysAddr::new(entry_addr as u64), order).as_u64() as usize) {
                    Self::set_entry_has_neighbor(map_offset, entry_addr);
                }

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
        // 1 bit: unused, reserved for future usage

        Self { memory_map, map_offset: map_dest.start as usize * 4096, orders }
    }

    fn entry_next_ptr(map_offset: usize, entry_addr: usize) -> *mut u8 {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        const MASK: usize = (1 << 39) - 1; // the 39 lower bits are set
        let metadata_part = unsafe { *(&*ptr::from_exposed_addr(dst) as &usize) };
        let link = (metadata_part & MASK) * 4096;
        link as *mut u8
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
        const MASK: usize = 1 << 40;
        let metadata_part = unsafe { *(&*ptr::from_exposed_addr(dst) as &usize) };
        let meta = metadata_part & MASK;
        meta >> 40
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
        const MASK_OFFSET: usize = 40;
        let metadata_part = unsafe { &mut *ptr::from_exposed_addr_mut(dst) as &mut usize };
        let val = unsafe { transmute::<bool, u8>(val) } as usize;
        *metadata_part |= val << 40;
        *metadata_part &= !(val << 4);
    }

    #[inline(always)]
    fn set_entry_has_neighbor(map_offset: usize, entry_addr: usize) {
        Self::set_entry_meta_first(map_offset, entry_addr);
    }

    #[inline]
    fn set_entry_meta_first(map_offset: usize, entry_addr: usize) {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        const MASK: usize = 1 << 40;
        let metadata_part = unsafe { &mut *ptr::from_exposed_addr_mut(dst) as &mut usize };
        *metadata_part |= MASK;
    }

    fn write_entry_next<const RAW: bool>(map_offset: usize, entry_addr: usize, next_entry_addr: usize) {
        let dst = map_offset + (entry_addr.div_floor(4096) * 10);
        let metadata_part = {
            let mut tmp = if RAW {
                next_entry_addr
            } else {
                next_entry_addr / 4096
            };
            unsafe { tmp | ((((*(&*ptr::from_exposed_addr(dst + 4) as &u32)) >> 7) as usize) << 39) } // only keep the last 3 bytes (and one bit extra metadata)
        };
        let metadata_part_addr = unsafe { &mut *ptr::from_exposed_addr_mut(dst) as &mut usize };
        *metadata_part_addr = metadata_part;
    }

    #[inline]
    fn write_entry_prev<const RAW: bool>(map_offset: usize, entry_addr: usize, prev_entry_addr: usize) {
        Self::write_entry_next::<RAW>(map_offset, entry_addr + 5, prev_entry_addr)
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

    // FIXME: Create metadata write functions!

    fn inner_allocate_frame(&mut self) -> Option<PhysFrame> {
        self.allocate_frames(0).map(|addr| PhysFrame::containing_address(addr))
    }

    fn allocate_frames(&mut self, order: usize) -> Option<PhysAddr> {
        let mut curr_order = order;
        while MAX_ORDER > curr_order && self.orders[curr_order] == 0 {
            curr_order += 1;
        }
        if curr_order >= MAX_ORDER {
            return None;
        }

        let entry = self.orders[curr_order] * 4096;

        // retrieve next entry and update its metadata
        let next_entry = Self::entry_next_ptr(self.map_offset, entry);
        self.orders[curr_order] = next_entry.expose_addr() / 4096;
        if !next_entry.is_null() {
            Self::write_entry_prev::<true>(self.map_offset, next_entry.expose_addr(), 0);
        }

        // Split up the buddy until we have the desired size
        while curr_order > order {
            curr_order -= 1;
            let other = split_buddy(entry, curr_order + 1);
            Self::write_entry_next::<true>(self.map_offset, other, 0);
            Self::write_entry_prev::<true>(self.map_offset, other, 0);
            self.orders[curr_order] = other / 4096; // convert into internal repr and replace current list head
        }

        Some(PhysAddr::new(entry as u64))
    }

    pub fn deallocate_frame(&mut self, address: PhysAddr) {
        self.deallocate_frames(address, 0);
    }

    pub fn deallocate_frames(&mut self, address: PhysAddr, order: usize) {
        Self::free_entry(self.map_offset, address.as_u64() as usize);
        let mut new_order = order;
        while MAX_ORDER > order {
            let other_buddy = other_buddy(address, order);
            if !Self::entry_has_neighbor(self.map_offset, address.as_u64() as usize) || !Self::is_free(self.map_offset, other_buddy.as_u64() as usize) {
                break;
            }
            Self::free_entry(self.map_offset, other_buddy.as_u64() as usize);
            new_order += 1;
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for DefaultFrameAllocator {
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

/// Conceptually this represents splitting a parent buddy into two smaller ones (children)
/// This function returns the address of the upper buddy, the lower buddy
/// is located at `base`.
///
/// `order` is the order of the parent buddy
#[inline]
fn split_buddy(base: usize, order: usize) -> usize {
    base + (2 << (order - 1))
}

/// `order` is the order of the child (probably - CHECK THIS!)
fn other_buddy(curr_buddy: PhysAddr, order: usize) -> PhysAddr {
    let buddy_size = 4096_u64 * (1 << order);
    let base = curr_buddy.align_down(buddy_size * 2);

    if base == curr_buddy {
        curr_buddy + buddy_size
    } else {
        curr_buddy
    }
}

// FIXME: Linux only ever moves 2 smaller buddies into one bigger one if both smaller ones are usable by simply treating the unusable buddy as already used

trait MemoryMapFunctions {

    fn is_usable(&self, addr: usize) -> bool;

}

impl MemoryMapFunctions for MemoryMap {
    fn is_usable(&self, addr: usize) -> bool {
        let regions = unsafe { &*slice_from_raw_parts(self.as_ptr(), 64) as &[MemoryRegion] };
        let idx = search_length_limited_nearest(regions, addr as u64 / 4096, regions.len());
        regions[idx].region_type == MemoryRegionType::Usable
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
