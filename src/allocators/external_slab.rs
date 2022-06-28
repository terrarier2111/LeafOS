use crate::mem::addr::{PhysAddr, VirtAddr};
use crate::mem::frame::PhysFrame;
use crate::mem::mapped_page_table::{Mapper, Translate};
use crate::mem::page::{Page, Size4KiB};
use crate::mem::page_table::PageTableFlags;
use crate::mem::paging::BuddyFrameAllocator;
use crate::mem::{FRAME_ALLOCATOR, MAPPER, PHYSICAL_MEMORY_OFFSET};
use crate::println;
use alloc::sync::Arc;
use core::alloc::{GlobalAlloc, Layout};
use core::mem::{transmute, MaybeUninit};
use core::ops::DerefMut;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use slabmalloc::{AllocationError, Allocator, LargeObjectPage, ObjectPage, ZoneAllocator};
use spin::Mutex;

// FIXME: currently it looks like the underlying slab allocation library is buggy because it returns unaligned, seemingly random addresses
// FIXME: see: https://github.com/gz/rust-slabmalloc/issues/9

/// To use a ZoneAlloactor we require a lower-level allocator
/// (not provided by this crate) that can supply the allocator
/// with backing memory for `LargeObjectPage` and `ObjectPage` structs.
///
// FIXME: Make the buddy allocator internally mutable (preferably using atomics - this is okay as we don't care about hardware which doesn't support atomics)
struct Pager;

impl Pager {
    const BASE_PAGE_SIZE: usize = 4096;
    const LARGE_PAGE_SIZE: usize = 2 * 1024 * 1024;

    /// Allocates a given `page_size`.
    fn alloc_page(&self, page_size: usize) -> Option<*mut u8> {
        // FIXME: map multiple pages
        let order = BuddyFrameAllocator::order_from_size(page_size);
        let frame = unsafe { FRAME_ALLOCATOR.lock().allocate_frames(order) };

        // FIXME: use a heap instead of inserting pages into the TLB every time we allocate them
        if let Some(frame) = &frame {
            for fc in 0..(1 << order) {
                let mut frame_allocator = unsafe { FRAME_ALLOCATOR.lock() };
                let mut frame_allocator = frame_allocator.deref_mut();

                let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
                let mut mapper = MAPPER.lock();
                let page: Page<Size4KiB> =
                    Page::from_start_address(VirtAddr::new(frame.as_u64() + (fc as u64 * 4096))).unwrap();
                println!("mapped: {:?}", VirtAddr::new(frame.as_u64() + (fc as u64 * 4096)));
                let phys_frame: PhysFrame<Size4KiB> =
                    PhysFrame::from_start_address(frame.clone() + (fc as u64 * 4096)).unwrap();
                unsafe {
                    mapper
                        .map_to::<BuddyFrameAllocator>(page, phys_frame, flags, frame_allocator)
                        .unwrap()
                        .flush()
                };
            }
        }

        println!("allocated: {} align: {}", frame.unwrap().as_u64(), frame.unwrap().as_u64() % 4096);

        frame.map(|x| {
            ptr::from_exposed_addr_mut(
                (x.as_u64()/* + PHYSICAL_MEMORY_OFFSET.load(Ordering::Relaxed)*/) as usize,
            )
        })
    }

    /// De-allocates a given `page_size`.
    fn dealloc_page(&self, ptr: *mut u8, page_size: usize) {
        // FIXME: do we translate smth?
        println!("deallocing {} align: {}", ptr.expose_addr(), ptr.expose_addr() % 4096);
        let order = BuddyFrameAllocator::order_from_size(page_size);
        let frame = PhysAddr::new(ptr.expose_addr() as u64);
        for fc in 0..(1 << order) {
            let mut mapper = MAPPER.lock();
            println!("frame: {} align: {}", frame.as_u64() + (fc as u64 * 4096), (frame.as_u64() + (fc as u64 * 4096)) % 4096);
            let page: Page<Size4KiB> =
                Page::from_start_address(VirtAddr::new(frame.as_u64() + (fc as u64 * 4096))).unwrap();
            println!("unmapped: {:?}", VirtAddr::new(frame.as_u64() + (fc as u64 * 4096)));
            unsafe {
                mapper
                    .unmap(page).unwrap().1.flush();
            };
        }
        unsafe {
            FRAME_ALLOCATOR.lock().deallocate_frames(
                PhysAddr::new(ptr.expose_addr() as u64),
                order,
            )
        };
    }

    /// Allocates a new ObjectPage from the System.
    fn allocate_page(&self) -> Option<&'static mut ObjectPage<'static>> {
        self.alloc_page(Pager::BASE_PAGE_SIZE)
            .map(|r| unsafe { transmute(r as usize) })
    }

    /// Release a ObjectPage back to the System.
    #[allow(unused)]
    fn release_page(&self, p: &'static mut ObjectPage<'static>) {
        self.dealloc_page(p as *const ObjectPage as *mut u8, Pager::BASE_PAGE_SIZE);
    }

    /// Allocates a new LargeObjectPage from the system.
    fn allocate_large_page(&self) -> Option<&'static mut LargeObjectPage<'static>> {
        self.alloc_page(Pager::LARGE_PAGE_SIZE)
            .map(|r| unsafe { transmute(r as usize) })
    }

    /// Release a LargeObjectPage back to the System.
    #[allow(unused)]
    fn release_large_page(&self, p: &'static mut LargeObjectPage<'static>) {
        self.dealloc_page(
            p as *const LargeObjectPage as *mut u8,
            Pager::LARGE_PAGE_SIZE,
        );
    }
}

/// A pager for GlobalAlloc.
static PAGER: Pager = Pager;

/// A SafeZoneAllocator that wraps the ZoneAllocator in a Mutex.
///
/// Note: This is not very scalable since we use a single big lock
/// around the allocator. There are better ways make the ZoneAllocator
/// thread-safe directly, but they are not implemented yet.
pub struct SafeZoneAllocator(Mutex<ZoneAllocator<'static>>);

unsafe impl GlobalAlloc for SafeZoneAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match layout.size() {
            Pager::BASE_PAGE_SIZE => {
                // Best to use the underlying backend directly to allocate pages
                // to avoid fragmentation
                PAGER.allocate_page().expect("Can't allocate page?") as *mut _ as *mut u8
            }
            Pager::LARGE_PAGE_SIZE => {
                // Best to use the underlying backend directly to allocate large
                // to avoid fragmentation
                PAGER.allocate_large_page().expect("Can't allocate page?") as *mut _ as *mut u8
            }
            0..=ZoneAllocator::MAX_ALLOC_SIZE => {
                let mut zone_allocator = self.0.lock();
                match zone_allocator.allocate(layout) {
                    Ok(nptr) => nptr.as_ptr(),
                    Err(AllocationError::OutOfMemory) => {
                        if layout.size() <= ZoneAllocator::MAX_BASE_ALLOC_SIZE {
                            PAGER.allocate_page().map_or(ptr::null_mut(), |page| {
                                zone_allocator
                                    .refill(layout, page)
                                    .expect("Could not refill?");
                                zone_allocator
                                    .allocate(layout)
                                    .expect("Should succeed after refill")
                                    .as_ptr()
                            })
                        } else {
                            // layout.size() <= ZoneAllocator::MAX_ALLOC_SIZE
                            PAGER
                                .allocate_large_page()
                                .map_or(ptr::null_mut(), |large_page| {
                                    zone_allocator
                                        .refill_large(layout, large_page)
                                        .expect("Could not refill?");
                                    zone_allocator
                                        .allocate(layout)
                                        .expect("Should succeed after refill")
                                        .as_ptr()
                                })
                        }
                    }
                    Err(AllocationError::InvalidLayout) => panic!("Can't allocate this size"),
                }
            }
            _ => unimplemented!("Can't handle it, probably needs another allocator."),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match layout.size() {
            Pager::BASE_PAGE_SIZE => PAGER.dealloc_page(ptr, Pager::BASE_PAGE_SIZE),
            Pager::LARGE_PAGE_SIZE => PAGER.dealloc_page(ptr, Pager::LARGE_PAGE_SIZE),
            0..=ZoneAllocator::MAX_ALLOC_SIZE => {
                if let Some(nptr) = NonNull::new(ptr) {
                    self.0
                        .lock()
                        .deallocate(nptr, layout)
                        .expect("Couldn't deallocate");
                } else {
                    // Nothing to do (don't dealloc null pointers).
                }

                // A proper reclamation strategy could be implemented here
                // to release empty pages back from the ZoneAllocator to the PAGER
                PAGER.dealloc_page(ptr, layout.size());
            }
            _ => unimplemented!("Can't handle it, probably needs another allocator."),
        }
    }
}

impl SafeZoneAllocator {
    pub const fn new() -> Self {
        Self(Mutex::new(ZoneAllocator::new()))
    }
}
