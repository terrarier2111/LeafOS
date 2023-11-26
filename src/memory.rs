use core::ptr::NonNull;

use limine::{MemmapEntry, NonNullPtr, MemoryMapEntryType};
use x86_64::{PhysAddr, structures::paging::PageTable, VirtAddr};
use x86_64::structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PhysFrame, Size4KiB};
use crate::memory;
use crate::sc_cell::SCCell;

static HHDM_OFFSET: SCCell<usize> = SCCell::new(0);

// The bigger the number of a page table, the larger the memory region (level 4 contains multiple level 3 etc.)
// Virtual memory blocks: pages
// Physical memory blocks: frames


/// Returns a mutable reference to the active level 4 table.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must be only called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
unsafe fn active_level_4_table(hhdm_offset: usize)
                                   -> &'static mut PageTable
{
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = hhdm_offset as u64 + phys.as_u64();

    &mut *(page_table_ptr as *mut PageTable)
}

/// Initialize a new OffsetPageTable.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must only be called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
pub unsafe fn init(hhdm_offset: usize) -> OffsetPageTable<'static> {
    HHDM_OFFSET.set(hhdm_offset);
    let level_4_table = active_level_4_table(hhdm_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

/// A FrameAllocator that always returns `None`.
pub struct EmptyFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        None
    }
}

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    entry_cnt: usize,
    ptr: NonNull<NonNullPtr<MemmapEntry>>,
    offset: usize,
    entry_offset: usize,
}

impl BootInfoFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(entry_cnt: usize, entries_ptr: NonNull<NonNullPtr<MemmapEntry>>) -> Self {
        BootInfoFrameAllocator {
            entry_cnt,
            ptr: entries_ptr,
            offset: 0,
            entry_offset: 0,
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        while self.entry_cnt > self.offset {
            let entry = unsafe { (*self.ptr.as_ptr()).as_ptr().add(self.offset) };
            if unsafe { (&*entry).typ != MemoryMapEntryType::Usable } {
                self.offset += 1;
                continue;
            }
            // divide by 4096 by shifting
            if unsafe { ((&*entry).len >> 12) > self.entry_offset }  {
                let entry_offset = self.entry_offset;
                self.entry_offset += 1;
                return Some(PhysFrame::from_start_address(PhysAddr::new(unsafe { (&*entry).base + entry_offset } as usize as u64)).unwrap());
            }
            self.offset += 1;
            self.entry_offset = 0;
        }
        None
    }
}

pub fn setup(entry_cnt: usize, entries_ptr: NonNull<NonNullPtr<MemmapEntry>>, physical_memory_offset: u64) -> (OffsetPageTable<'static>, BootInfoFrameAllocator) {
    let phys_mem_offset = VirtAddr::new(physical_memory_offset);
    // initialize a mapper
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(entry_cnt, entries_ptr)
    };
    crate::allocators::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    (mapper, frame_allocator)
}

#[inline]
pub fn get_hddm_offset() -> usize {
    HHDM_OFFSET.get()
}
