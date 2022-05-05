use bootloader::bootinfo::MemoryMap;
use crate::mem::mapped_page_table::OffsetPageTable;
use crate::mem::paging::{DefaultFrameAllocator, init};
use crate::println;

pub mod paging;
pub mod addr;
pub mod page_table;
pub mod frame;
pub mod page;
pub mod mapped_page_table;

pub fn setup(
    memory_map: &'static MemoryMap,
    physical_memory_offset: u64,
) -> (OffsetPageTable, DefaultFrameAllocator) {
    // initialize a mapper
    let mut mapper = unsafe { init(physical_memory_offset) };
    println!("inited mapper");
    let mut frame_allocator = unsafe { DefaultFrameAllocator::init(memory_map) };
    println!("inited frame allocator");
    crate::allocators::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    println!("inited heap");
    (mapper, frame_allocator)
}