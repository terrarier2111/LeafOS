use alloc::sync::Arc;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};
use bootloader::bootinfo::MemoryMap;
use lazy_static::lazy_static;
use spin::Mutex;
use crate::mem::frame::PhysFrame;
use crate::mem::mapped_page_table::{FrameAllocator, Mapper, OffsetPageTable};
use crate::mem::page_table::PageTable;
use crate::mem::paging::{BuddyFrameAllocator, init};
use crate::println;

pub mod paging;
pub mod addr;
pub mod page_table;
pub mod frame;
pub mod page;
pub mod mapped_page_table;
pub mod heap;

// FIXME: look at this: https://softwareengineering.stackexchange.com/questions/207386/how-are-the-size-of-the-stack-and-heap-limited-by-the-os
// FIXME: and for slab allocation look at this: https://hammertux.github.io/slab-allocator

pub static mut FRAME_ALLOCATOR: Mutex<BuddyFrameAllocator> = Mutex::new(BuddyFrameAllocator::invalid());
lazy_static! {
        pub static ref MAPPER: Mutex<OffsetPageTable<'static>> = Mutex::new(unsafe { init(PHYSICAL_MEMORY_OFFSET.load(Ordering::Acquire)) }); // FIXME: don't store the MAPPER but instead generate it new a couple of times as it's really cheap to recreate
}
pub static PHYSICAL_MEMORY_OFFSET: AtomicU64 = AtomicU64::new(0);

pub fn setup(
    memory_map: &'static MemoryMap,
    physical_memory_offset: u64,
)/* -> (OffsetPageTable/*, Arc<Mutex<BuddyFrameAllocator>>*/)*/ {
    // initialize a mapper
    PHYSICAL_MEMORY_OFFSET.store(physical_memory_offset, Ordering::SeqCst);
    let mut frame_allocator = unsafe { BuddyFrameAllocator::init(memory_map, &MAPPER) };
    println!("test0");
    let frame_allocator = Mutex::new(frame_allocator);

    unsafe { FRAME_ALLOCATOR = frame_allocator };

    // FIXME: How can we avoid using Arc here? should we define a global frame allocator?!
    /*crate::allocators::init_heap(&mut mapper)
        .expect("heap initialization failed");*/

    println!("inited heap");
    // (mapper/*, Arc::new(test)*/)
}

// FIXME: We need to add a syscall/(or any other way) allow the os to provide frames/pages to the userspace

pub fn setup_user_address_space() -> PhysFrame {
    let mut frame_alloc = unsafe { FRAME_ALLOCATOR.lock() };
    let frame = frame_alloc.allocate_frame().unwrap(); // FIXME: Properly handle errors!
    let pgt = unsafe { &mut *ptr::from_exposed_addr_mut(frame.start_address.as_u64() as usize) as &mut PageTable };

    // clear all entries
    pgt.zero();

    frame
}
