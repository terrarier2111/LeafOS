use alloc::sync::Arc;
use core::ops::DerefMut;
use spin::Mutex;
use crate::allocators::external_slab::SafeZoneAllocator;
use crate::mem::addr::VirtAddr;
use crate::mem::FRAME_ALLOCATOR;
use crate::mem::mapped_page_table::{FrameAllocator, Mapper, MapToError};
use crate::mem::page::{Page, Size4KiB};
use crate::mem::page_table::PageTableFlags;
use crate::mem::paging::BuddyFrameAllocator;
use crate::println;

mod fixed_size_block;
// mod slab;
mod external_slab;

/*
#[global_allocator]
static ALLOCATOR: Locked<FixedSizeBlockAllocator> = Locked::new(
    FixedSizeBlockAllocator::new());
*/

/// A wrapper around spin::Mutex to permit trait implementations.
pub struct Locked<A> {
    inner: Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}

#[global_allocator]
static ALLOCATOR: SafeZoneAllocator = SafeZoneAllocator::new(); // FIXME: Replace the current allocator impl with a better one

// FIXME: Pick these constants based on arguable ideas or even better don't pick constants at all and choose suitable
// FIXME: values at runtime in respect to resource availability
pub const HEAP_START: usize = 0x_4444_4444_0000;
// pub const HEAP_SIZE: usize = 100 * 1024; // 100 KiB
pub const HEAP_SIZE: usize = 1000 * 1024; // 1000 KiB

/*
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    println!("pre init!");

    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let mut frame_allocator = unsafe { FRAME_ALLOCATOR.lock() };
        let mut frame_allocator = frame_allocator.deref_mut();

        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush()
        };
    }

    Ok(())
}*/
