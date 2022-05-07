use bootloader::bootinfo::MemoryMap;
use crate::mem::mapped_page_table::{FrameAllocator, OffsetPageTable};
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
    let mut frame_allocator = unsafe { DefaultFrameAllocator::init(memory_map, &mut mapper) };
    println!("inited frame allocator");
    let test_frame = frame_allocator.allocate_frame();
    println!("frame: {}", test_frame.is_some());
    if test_frame.is_some() {
        println!("frame_addr: {}", test_frame.unwrap().start_address.as_u64());
    }
    let test2_frame = frame_allocator.allocate_frame();
    println!("frame2: {}", test2_frame.is_some());
    frame_allocator.deallocate_frame(test2_frame.unwrap().start_address);
    frame_allocator.deallocate_frame(test_frame.unwrap().start_address);
    println!("cleaned up!");
    /*crate::allocators::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    println!("inited heap");*/
    (mapper, frame_allocator)
}