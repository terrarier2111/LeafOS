use core::{mem, ptr};
use core::ptr::{addr_of, slice_from_raw_parts, slice_from_raw_parts_mut};
use crate::mem::addr::PhysAddr;
use crate::mem::FRAME_ALLOCATOR;
use crate::mem::mapped_page_table::FrameAllocator;
use crate::mem::paging::{map_multi_order_page, map_single_order_page};
use crate::println;

// FIXME: create heap in order to have a place where prepared frames(ones that are already inserted in the page table) can stay
// FIXME: or otherwise find a different solution to how to avoid inserting frames into the page table twice

// FIXME: see: https://wiki.syslinux.org/wiki/index.php?title=Heap_Management





// FIXME: the solution probably is to allocate a heap which has at least size N but if we have enough memory on the system we can allocate
// FIXME: up to some relative max value but we *PROBABLY* should cap this limit

// FIXME: If we run out of heap space, we will simply allocate more frames on demand and insert them into the page table
// FIXME: and once we're done with them, we will clear the relevant page table entries (for that we can simply check if
// FIXME: a frame is contained in the page table on deallocation) - we should probably also zero out all frames which are
// FIXME: released and were previously used by the kernel


// FIXME: we could instead use a single heap at the start and "add" more heaps if we run out of space
// FIXME: this would allow for minimal cost on allocation/deallocation
// FIXME: though this would require us to track how many frames are free in a given heap
// FIXME: in order to be able to deallocate one heap once the others enough space to accommodate
// FIXME: it's content and have an additional x percent of their total memory or x bytes of memory or
// FIXME: x percent of the system's total memory free in order to not run into frequent
// FIXME: frequent allocations/deallocations of new heaps

const SHARDS_PER_FRAME: usize = 4096_usize.div_floor(mem::size_of::<HeapShard>());

// we don't store the next pointer in the shard but store shards accumulated inside a single array in the Heap struct
// because this should still allow for 0.5TB for each frame we use to store shards in (if shards have a size of 16 bytes)
pub struct Heap {
    shards: &'static mut [HeapShard; SHARDS_PER_FRAME],
    shard_count: usize,
    orders: [usize; 2], // 4KB and 2MB are the different orders
    // FIXME: unfortunately we probably have to use a buddy system in here as well (which is a lot of boilerplate code)
}

// FIXME: try making this hardware independent (currently we are only thinking about x86)
const TWO_MB: usize = 2 * 1024 * 1024;
const FOUR_KB: usize = 4096;

impl Heap {

    // FIXME: allocation and deallocation are O(log(n)) because we are using binary search
    // FIXME: to find the shard with the metadata associated with an address
    // FIXME: we find the metadata itself by subtracting the shard's start address
    // FIXME: from the address we want to find the metadata of and then dividing the address we get by 10
    // FIXME: at the end we add metadata_start and the result of that is our desired metadata's address

    /*
    log2(32768) = 15
    2^15 = 32768
    */

    const INITIAL_HEAP_SHARD_ORDER: usize = 12; // this allows us to only have to carry 10 frames of metadata per shard
                                                // FIXME: find some better value so that we don't have to waste 5-6 frames for each heap shard's metadata

    pub unsafe fn new() -> Self {
        // let heap_shard = FRAME_ALLOCATOR.lock().allocate_large_frames(Self::INITIAL_HEAP_SHARD_ORDER);
        let shards_frame = FRAME_ALLOCATOR.lock().allocate_frame(); // FIXME: we probably need more shards, so first calculate how many we need (in a const)
        // map_multi_order_page(heap_shard, Self::INITIAL_HEAP_SHARD_ORDER);
        map_single_order_page(shards_frame);
        let shards = slice_from_raw_parts_mut::<HeapShard>(ptr::from_exposed_addr_mut(shards_frame.unwrap().start_address.as_u64() as usize), SHARDS_PER_FRAME) as *mut [HeapShard; SHARDS_PER_FRAME];
        // let heap_shard = HeapShard::new(ptr::from_exposed_addr_mut(heap_shard.unwrap().as_u64() as usize)/*, Self::INITIAL_HEAP_SHARD_ORDER as u8*/);
        // shards[0] = heap_shard.unwrap();
        let mut ret = Self {
            shards: shards.as_mut().unwrap(), // FIXME: actually allocate memory instead of using null
            shard_count: 0,
            orders: [0, 0],
        };
        ret.alloc_new_shard();
        ret
    }

    fn alloc_new_shard(&mut self) {
        // FIXME: check if we still have shard space left to put out next shard in
        let heap_shard = unsafe { FRAME_ALLOCATOR.lock().allocate_large_frames(Self::INITIAL_HEAP_SHARD_ORDER) };
        // we need 10 frames so we unfortunately have to round up to 16 (order 5)
        let shard_metadata = unsafe { FRAME_ALLOCATOR.lock().allocate_frames(5) };
        map_multi_order_page(heap_shard, Self::INITIAL_HEAP_SHARD_ORDER);
        map_multi_order_page(shard_metadata, 5);
        let heap_shard = HeapShard::new(ptr::from_exposed_addr_mut(heap_shard.unwrap().as_u64() as usize),
                                        ptr::from_exposed_addr_mut(shard_metadata.unwrap().as_u64() as usize)/*, Self::INITIAL_HEAP_SHARD_ORDER as u8*/);
        self.shards[self.shard_count] = heap_shard;
        self.shard_count += 1;
    }

    fn try_free_shard(&mut self) {
        let mut all_usable = 0;
        for shard in self.shards.iter() {
            let free = shard.free[0] + shard.free[1] * (TWO_MB / FOUR_KB);
            all_usable += free;
        }
        // FIXME: try free shard if some criteria is fulfilled
    }

    fn head_shard(&self) -> HeapShard {
        self.shards[0]
    }

}

#[derive(Debug, Copy, Clone)]
pub struct HeapShard {
    // data: usize, // FIXME: do we actually need to pack the heap shard's order into the data pointer?
                 // FIXME: because this could hurt performance and not bring any significant gains.
    start: *mut u8,
    metadata_start: *mut u8,
    free: [usize; 2],
}

impl HeapShard {

    /*
    pub fn new(start: *mut u8, size: u8) -> Self {
        // FIXME: insert every frame into the page table
        Self {
            data: start | (size as usize),
            // next: ptr::null_mut(),
        }
    }*/

    pub fn new(start: *mut u8, metadata_start: *mut u8) -> Self {
        // FIXME: insert every frame into the page table
        Self {
            start/* | (size as usize)*/,
            // next: ptr::null_mut(),
            metadata_start,
            free: [0, (1 << Heap::INITIAL_HEAP_SHARD_ORDER) * FOUR_KB / TWO_MB], // calculate the number of free 2MB entries
        }
    }

    /*
    fn get_start(&self) -> *mut u8 {
        const MASK: usize = !((u8::MAX as usize) << (usize::BIT_LENGTH - 8)); // the upper 8 bits are NOT set, everything else is
        ptr::from_exposed_addr_mut(self.data & MASK)
    }

    /// returns the order of this heap shard which can be used to retrieve its size
    fn get_order(&self) -> u8 {
        const MASK: usize = (u8::MAX as usize) << (usize::BIT_LENGTH - 8); // only the upper 8 bits are set
        (self.data & MASK) as u8
    }*/

    fn get_start(&self) -> *mut u8 {
        self.start
    }

    fn get_metadata_start(&self) -> *mut u8 {
        self.metadata_start
    }

}

impl Default for HeapShard {
    fn default() -> Self {
        Self {
            start: ptr::null_mut(),
            metadata_start: ptr::null_mut(),
            free: [0, 0],
        }
    }
}

// our layour allows for 2GB of memory per heap
// entry layout:
// 31 bits: next entry data
// 1 bit: flag whether or not this page has an usable neighbor
// 31 bits: prev entry data
// 1 bit: flag whether the current page is the first entry (of the two bodies) or not

struct BuddyMemSystem<const ORDERS: usize> {
    start: *mut u8,
}

impl<const ORDERS: usize> BuddyMemSystem<ORDERS> {

    const fn max_val(&self) -> usize {
        4096 * (1 << ORDERS)
    }

    const fn max_used_bits(&self) -> usize {
        // FIXME: find highest set bit of max_val
        todo!()
    }

    /// Safety:
    /// `page_address` has to be a valid address to an unused page in memory.
    fn entry(&self, page_address: u64) -> *mut MapEntry {
        Self::entry_glob(page_address, self.start.expose_addr())
    }

    /// Safety:
    /// `page_address` has to be a valid address to an unused page in memory.
    fn entry_glob(page_address: u64, map_offset: usize) -> *mut MapEntry {
        if page_address != 0 {
            let meta_addr = map_offset + (page_address.div_floor(4096) * 10) as usize; // FIXME: we probably gotta subtract the mapoffset somewhere - DO THAT!
            ptr::from_exposed_addr_mut::<MapEntry>(meta_addr)
        } else {
            // println!("got zero param in entry func!");
            ptr::null_mut()
        }
    }

}

#[repr(transparent)]
struct DoublyLinkedListHead(usize);

impl DoublyLinkedListHead {

    fn get_next<const ORDERS: usize>(&self, offset: usize) -> *mut MapEntry {
        let entry_raw = self.0 * 4096;
        BuddyMemSystem::<ORDERS>::entry_glob(entry_raw as u64, offset)
    }

    fn set_next(&mut self, next: *mut MapEntry, offset: usize) {
        if !next.is_null() {
            self.0 = unsafe { next.as_mut().unwrap().assoc_page(offset).expose_addr() / 4096 };
        } else {
            self.0 = 0;
        }
    }

}

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

