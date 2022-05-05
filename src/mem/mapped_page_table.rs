use core::arch::asm;
use core::convert::Infallible;
use core::ops::FromResidual;
use crate::mem::addr::{PhysAddr, VirtAddr};
use crate::mem::frame::PhysFrame;
use crate::mem::page::{Page, PageRangeInclusive, PageSize, Size1GiB, Size2MiB, Size4KiB};
use crate::mem::page_table::{FrameError, PageTable, PageTableEntry, PageTableFlags, PageTableLevel};
use crate::mem::paging::level_5_paging;
use crate::println;

/// A Mapper implementation that relies on a PhysAddr to VirtAddr conversion function.
///
/// This type requires that the all physical page table frames are mapped to some virtual
/// address. Normally, this is done by mapping the complete physical address space into
/// the virtual address space at some offset. Other mappings between physical and virtual
/// memory are possible too, as long as they can be calculated as an `PhysAddr` to
/// `VirtAddr` closure.
#[derive(Debug)]
pub struct MappedPageTable<'a, P: PageTableFrameMapping> {
    page_table_walker: PageTableWalker<P>,
    top_level_table: &'a mut PageTable,
}

impl<'a, P: PageTableFrameMapping> MappedPageTable<'a, P> {
    /// Creates a new `MappedPageTable` that uses the passed closure for converting virtual
    /// to physical addresses.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because the caller must guarantee that the passed `page_table_frame_mapping`
    /// closure is correct. Also, the passed `level_4_table` must point to the level 4 page table
    /// of a valid page table hierarchy. Otherwise this function might break memory safety, e.g.
    /// by writing to an illegal memory location.
    #[inline]
    pub unsafe fn new(top_level_table: &'a mut PageTable, page_table_frame_mapping: P) -> Self {
        Self {
            top_level_table,
            page_table_walker: unsafe { PageTableWalker::new(page_table_frame_mapping) },
        }
    }

    /// Returns a mutable reference to the wrapped level 4 or level 5 `PageTable` instance.
    pub fn top_level_table(&mut self) -> &mut PageTable {
        &mut self.top_level_table
    }

    /// Helper function for implementing Mapper. Safe to limit the scope of unsafe, see
    /// https://github.com/rust-lang/rfcs/pull/2585.
    fn map_to_1gib<A>(
        &mut self,
        page: Page<Size1GiB>,
        frame: PhysFrame<Size1GiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size1GiB>, MapToError<Size1GiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self.page_table_walker.create_next_table(
            &mut p4[page.p4_index()],
            parent_table_flags,
            allocator,
        )?;

        if !p3[page.p3_index()].is_unused() {
            return Err(MapToError::PageAlreadyMapped(frame));
        }
        p3[page.p3_index()].set_addr(frame.start_address(), flags | PageTableFlags::HUGE_PAGE);

        Ok(MapperFlush::new(page))
    }

    /// Helper function for implementing Mapper. Safe to limit the scope of unsafe, see
    /// https://github.com/rust-lang/rfcs/pull/2585.
    fn map_to_2mib<A>(
        &mut self,
        page: Page<Size2MiB>,
        frame: PhysFrame<Size2MiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size2MiB>, MapToError<Size2MiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self.page_table_walker.create_next_table(
            &mut p4[page.p4_index()],
            parent_table_flags,
            allocator,
        )?;
        let p2 = self.page_table_walker.create_next_table(
            &mut p3[page.p3_index()],
            parent_table_flags,
            allocator,
        )?;

        if !p2[page.p2_index()].is_unused() {
            return Err(MapToError::PageAlreadyMapped(frame));
        }
        p2[page.p2_index()].set_addr(frame.start_address(), flags | PageTableFlags::HUGE_PAGE);

        Ok(MapperFlush::new(page))
    }

    /// Helper function for implementing Mapper. Safe to limit the scope of unsafe, see
    /// https://github.com/rust-lang/rfcs/pull/2585.
    fn map_to_4kib<A>(
        &mut self,
        page: Page<Size4KiB>,
        frame: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size4KiB>, MapToError<Size4KiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self.page_table_walker.create_next_table(
            &mut p4[page.p4_index()],
            parent_table_flags,
            allocator,
        )?;
        let p2 = self.page_table_walker.create_next_table(
            &mut p3[page.p3_index()],
            parent_table_flags,
            allocator,
        )?;
        let p1 = self.page_table_walker.create_next_table(
            &mut p2[page.p2_index()],
            parent_table_flags,
            allocator,
        )?;

        if !p1[page.p1_index()].is_unused() {
            return Err(MapToError::PageAlreadyMapped(frame));
        }
        p1[page.p1_index()].set_frame(frame, flags);

        Ok(MapperFlush::new(page))
    }
}

impl<'a, P: PageTableFrameMapping> Mapper<Size1GiB> for MappedPageTable<'a, P> {
    #[inline]
    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: Page<Size1GiB>,
        frame: PhysFrame<Size1GiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size1GiB>, MapToError<Size1GiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        self.map_to_1gib(page, frame, flags, parent_table_flags, allocator)
    }

    fn unmap(
        &mut self,
        page: Page<Size1GiB>,
    ) -> Result<(PhysFrame<Size1GiB>, MapperFlush<Size1GiB>), UnmapError> {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self
            .page_table_walker
            .next_table_mut(&mut p4[page.p4_index()])?;

        let p3_entry = &mut p3[page.p3_index()];
        let flags = p3_entry.flags();

        if !flags.contains(PageTableFlags::PRESENT) {
            return Err(UnmapError::PageNotMapped);
        }
        if !flags.contains(PageTableFlags::HUGE_PAGE) {
            return Err(UnmapError::ParentEntryHugePage);
        }

        let frame = PhysFrame::from_start_address(p3_entry.addr())
            .map_err(|address_not_aligned| UnmapError::InvalidFrameAddress(p3_entry.addr()))?;

        p3_entry.set_unused();
        Ok((frame, MapperFlush::new(page)))
    }

    unsafe fn update_flags(
        &mut self,
        page: Page<Size1GiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<Size1GiB>, FlagUpdateError> {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self
            .page_table_walker
            .next_table_mut(&mut p4[page.p4_index()])?;

        if p3[page.p3_index()].is_unused() {
            return Err(FlagUpdateError::PageNotMapped);
        }
        p3[page.p3_index()].set_flags(flags | PageTableFlags::HUGE_PAGE);

        Ok(MapperFlush::new(page))
    }

    fn translate_page(&self, page: Page<Size1GiB>) -> Result<PhysFrame<Size1GiB>, TranslateError> {
        let p4 = if level_5_paging() {
            let p5 = &self.top_level_table;
            self.page_table_walker.next_table(&p5[page.p5_index()])?
        } else {
            &self.top_level_table
        };
        let p3 = self.page_table_walker.next_table(&p4[page.p4_index()])?;

        let p3_entry = &p3[page.p3_index()];

        if p3_entry.is_unused() {
            return Err(TranslateError::PageNotMapped);
        }

        PhysFrame::from_start_address(p3_entry.addr())
            .map_err(|address_not_aligned| TranslateError::InvalidFrameAddress(p3_entry.addr()))
    }
}

impl<'a, P: PageTableFrameMapping> Mapper<Size2MiB> for MappedPageTable<'a, P> {
    #[inline]
    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: Page<Size2MiB>,
        frame: PhysFrame<Size2MiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size2MiB>, MapToError<Size2MiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        self.map_to_2mib(page, frame, flags, parent_table_flags, allocator)
    }

    fn unmap(
        &mut self,
        page: Page<Size2MiB>,
    ) -> Result<(PhysFrame<Size2MiB>, MapperFlush<Size2MiB>), UnmapError> {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self
            .page_table_walker
            .next_table_mut(&mut p4[page.p4_index()])?;
        let p2 = self
            .page_table_walker
            .next_table_mut(&mut p3[page.p3_index()])?;

        let p2_entry = &mut p2[page.p2_index()];
        let flags = p2_entry.flags();

        if !flags.contains(PageTableFlags::PRESENT) {
            return Err(UnmapError::PageNotMapped);
        }
        if !flags.contains(PageTableFlags::HUGE_PAGE) {
            return Err(UnmapError::ParentEntryHugePage);
        }

        let frame = PhysFrame::from_start_address(p2_entry.addr())
            .map_err(|address_not_aligned| UnmapError::InvalidFrameAddress(p2_entry.addr()))?;

        p2_entry.set_unused();
        Ok((frame, MapperFlush::new(page)))
    }

    unsafe fn update_flags(
        &mut self,
        page: Page<Size2MiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<Size2MiB>, FlagUpdateError> {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self
            .page_table_walker
            .next_table_mut(&mut p4[page.p4_index()])?;
        let p2 = self
            .page_table_walker
            .next_table_mut(&mut p3[page.p3_index()])?;

        if p2[page.p2_index()].is_unused() {
            return Err(FlagUpdateError::PageNotMapped);
        }

        p2[page.p2_index()].set_flags(flags | PageTableFlags::HUGE_PAGE);

        Ok(MapperFlush::new(page))
    }

    fn translate_page(&self, page: Page<Size2MiB>) -> Result<PhysFrame<Size2MiB>, TranslateError> {
        let p4 = if level_5_paging() {
            let p5 = &self.top_level_table;
            self.page_table_walker
                .next_table(&p5[page.p5_index()])?
        } else {
            &self.top_level_table
        };
        let p3 = self.page_table_walker.next_table(&p4[page.p4_index()])?;
        let p2 = self.page_table_walker.next_table(&p3[page.p3_index()])?;

        let p2_entry = &p2[page.p2_index()];

        if p2_entry.is_unused() {
            return Err(TranslateError::PageNotMapped);
        }

        PhysFrame::from_start_address(p2_entry.addr())
            .map_err(|address_not_aligned| TranslateError::InvalidFrameAddress(p2_entry.addr()))
    }
}

impl<'a, P: PageTableFrameMapping> Mapper<Size4KiB> for MappedPageTable<'a, P> {
    #[inline]
    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: Page<Size4KiB>,
        frame: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size4KiB>, MapToError<Size4KiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        self.map_to_4kib(page, frame, flags, parent_table_flags, allocator)
    }

    fn unmap(
        &mut self,
        page: Page<Size4KiB>,
    ) -> Result<(PhysFrame<Size4KiB>, MapperFlush<Size4KiB>), UnmapError> {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self
            .page_table_walker
            .next_table_mut(&mut p4[page.p4_index()])?;
        let p2 = self
            .page_table_walker
            .next_table_mut(&mut p3[page.p3_index()])?;
        let p1 = self
            .page_table_walker
            .next_table_mut(&mut p2[page.p2_index()])?;

        let p1_entry = &mut p1[page.p1_index()];

        let frame = p1_entry.frame().map_err(|err| match err {
            FrameError::FrameNotPresent => UnmapError::PageNotMapped,
            FrameError::HugeFrame => UnmapError::ParentEntryHugePage,
        })?;

        p1_entry.set_unused();
        Ok((frame, MapperFlush::new(page)))
    }

    unsafe fn update_flags(
        &mut self,
        page: Page<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<Size4KiB>, FlagUpdateError> {
        let p4 = if level_5_paging() {
            let p5 = &mut self.top_level_table;
            self.page_table_walker
                .next_table_mut(&mut p5[page.p5_index()])?
        } else {
            &mut self.top_level_table
        };
        let p3 = self
            .page_table_walker
            .next_table_mut(&mut p4[page.p4_index()])?;
        let p2 = self
            .page_table_walker
            .next_table_mut(&mut p3[page.p3_index()])?;
        let p1 = self
            .page_table_walker
            .next_table_mut(&mut p2[page.p2_index()])?;

        if p1[page.p1_index()].is_unused() {
            return Err(FlagUpdateError::PageNotMapped);
        }

        p1[page.p1_index()].set_flags(flags);

        Ok(MapperFlush::new(page))
    }

    fn translate_page(&self, page: Page<Size4KiB>) -> Result<PhysFrame<Size4KiB>, TranslateError> {
        let p4 = if level_5_paging() {
            let p5 = &self.top_level_table;
            self.page_table_walker.next_table(&p5[page.p5_index()])?
        } else {
            &self.top_level_table
        };
        let p3 = self.page_table_walker.next_table(&p4[page.p4_index()])?;
        let p2 = self.page_table_walker.next_table(&p3[page.p3_index()])?;
        let p1 = self.page_table_walker.next_table(&p2[page.p2_index()])?;

        let p1_entry = &p1[page.p1_index()];

        if p1_entry.is_unused() {
            return Err(TranslateError::PageNotMapped);
        }

        PhysFrame::from_start_address(p1_entry.addr())
            .map_err(|address_not_aligned| TranslateError::InvalidFrameAddress(p1_entry.addr()))
    }
}

impl<'a, P: PageTableFrameMapping> Translate for MappedPageTable<'a, P> {
    #[allow(clippy::inconsistent_digit_grouping)]
    fn translate(&self, addr: VirtAddr) -> TranslateResult {
        let p4 = if level_5_paging() {
            let p5 = &self.top_level_table;
            self.page_table_walker.next_table(&p5[addr.p5_index()])?
        } else {
            &self.top_level_table
        };
        let p3 = match self.page_table_walker.next_table(&p4[addr.p4_index()]) {
            Ok(page_table) => page_table,
            Err(PageTableWalkError::NotMapped) => return TranslateResult::NotMapped,
            Err(PageTableWalkError::MappedToHugePage) => {
                panic!("level 4 entry has huge page bit set")
            }
        };
        let p2 = match self.page_table_walker.next_table(&p3[addr.p3_index()]) {
            Ok(page_table) => page_table,
            Err(PageTableWalkError::NotMapped) => return TranslateResult::NotMapped,
            Err(PageTableWalkError::MappedToHugePage) => {
                let entry = &p3[addr.p3_index()];
                let frame = PhysFrame::containing_address(entry.addr());
                let offset = addr.as_u64() & 0o_777_777_7777;
                let flags = entry.flags();
                return TranslateResult::Mapped {
                    frame: MappedFrame::Size1GiB(frame),
                    offset,
                    flags,
                };
            }
        };
        let p1 = match self.page_table_walker.next_table(&p2[addr.p2_index()]) {
            Ok(page_table) => page_table,
            Err(PageTableWalkError::NotMapped) => return TranslateResult::NotMapped,
            Err(PageTableWalkError::MappedToHugePage) => {
                let entry = &p2[addr.p2_index()];
                let frame = PhysFrame::containing_address(entry.addr());
                let offset = addr.as_u64() & 0o_777_7777;
                let flags = entry.flags();
                return TranslateResult::Mapped {
                    frame: MappedFrame::Size2MiB(frame),
                    offset,
                    flags,
                };
            }
        };

        let p1_entry = &p1[addr.p1_index()];

        if p1_entry.is_unused() {
            return TranslateResult::NotMapped;
        }

        let frame = match PhysFrame::from_start_address(p1_entry.addr()) {
            Ok(frame) => frame,
            Err(_address_not_aligned) => return TranslateResult::InvalidFrameAddress(p1_entry.addr()),
        };
        let offset = u64::from(addr.page_offset());
        let flags = p1_entry.flags();
        TranslateResult::Mapped {
            frame: MappedFrame::Size4KiB(frame),
            offset,
            flags,
        }
    }
}

impl<'a, P: PageTableFrameMapping> CleanUp for MappedPageTable<'a, P> {
    #[inline]
    unsafe fn clean_up<D>(&mut self, frame_deallocator: &mut D)
        where
            D: FrameDeallocator<Size4KiB>,
    {
        unsafe {
            self.clean_up_addr_range(
                PageRangeInclusive {
                    start: Page::from_start_address(VirtAddr::new(0)).unwrap(),
                    end: Page::from_start_address(VirtAddr::new(0xffff_ffff_ffff_f000)).unwrap(),
                },
                frame_deallocator,
            )
        }
    }

    unsafe fn clean_up_addr_range<D>(
        &mut self,
        range: PageRangeInclusive,
        frame_deallocator: &mut D,
    ) where
        D: FrameDeallocator<Size4KiB>,
    {
        unsafe fn clean_up<P: PageTableFrameMapping>(
            page_table: &mut PageTable,
            page_table_walker: &PageTableWalker<P>,
            level: PageTableLevel,
            range: PageRangeInclusive,
            frame_deallocator: &mut impl FrameDeallocator<Size4KiB>,
        ) -> bool {
            if range.is_empty() {
                return false;
            }

            let table_addr = range
                .start
                .start_address()
                .align_down(level.table_address_space_alignment());

            let start = range.start.page_table_index(level);
            let end = range.end.page_table_index(level);

            if let Some(next_level) = level.next_lower_level() {
                let offset_per_entry = level.entry_address_space_alignment();
                for (i, entry) in page_table
                    .iter_mut()
                    .enumerate()
                    .take(usize::from(end) + 1)
                    .skip(usize::from(start))
                {
                    if let Ok(page_table) = page_table_walker.next_table_mut(entry) {
                        let start = table_addr + (offset_per_entry * (i as u64));
                        let end = start + (offset_per_entry - 1);
                        let start = Page::<Size4KiB>::containing_address(start);
                        let start = start.max(range.start);
                        let end = Page::<Size4KiB>::containing_address(end);
                        let end = end.min(range.end);
                        unsafe {
                            if clean_up(
                                page_table,
                                page_table_walker,
                                next_level,
                                Page::range_inclusive(start, end),
                                frame_deallocator,
                            ) {
                                let frame = entry.frame().unwrap();
                                entry.set_unused();
                                frame_deallocator.deallocate_frame(frame);
                            }
                        }
                    }
                }
            }

            page_table.iter().all(PageTableEntry::is_unused)
        }

        unsafe {
            clean_up(
                self.top_level_table,
                &self.page_table_walker,
                PageTableLevel::Four,
                range,
                frame_deallocator,
            );
        }
    }
}

#[derive(Debug)]
struct PageTableWalker<P: PageTableFrameMapping> {
    page_table_frame_mapping: P,
}

impl<P: PageTableFrameMapping> PageTableWalker<P> {
    #[inline]
    pub unsafe fn new(page_table_frame_mapping: P) -> Self {
        Self {
            page_table_frame_mapping,
        }
    }

    /// Internal helper function to get a reference to the page table of the next level.
    ///
    /// Returns `PageTableWalkError::NotMapped` if the entry is unused. Returns
    /// `PageTableWalkError::MappedToHugePage` if the `HUGE_PAGE` flag is set
    /// in the passed entry.
    #[inline]
    fn next_table<'b>(
        &self,
        entry: &'b PageTableEntry,
    ) -> Result<&'b PageTable, PageTableWalkError> {
        let page_table_ptr = self
            .page_table_frame_mapping
            .frame_to_pointer(entry.frame()?);
        let page_table: &PageTable = unsafe { &*page_table_ptr };

        Ok(page_table)
    }

    /// Internal helper function to get a mutable reference to the page table of the next level.
    ///
    /// Returns `PageTableWalkError::NotMapped` if the entry is unused. Returns
    /// `PageTableWalkError::MappedToHugePage` if the `HUGE_PAGE` flag is set
    /// in the passed entry.
    #[inline]
    fn next_table_mut<'b>(
        &self,
        entry: &'b mut PageTableEntry,
    ) -> Result<&'b mut PageTable, PageTableWalkError> {
        let page_table_ptr = self
            .page_table_frame_mapping
            .frame_to_pointer(entry.frame()?);
        let page_table: &mut PageTable = unsafe { &mut *page_table_ptr };

        Ok(page_table)
    }

    /// Internal helper function to create the page table of the next level if needed.
    ///
    /// If the passed entry is unused, a new frame is allocated from the given allocator, zeroed,
    /// and the entry is updated to that address. If the passed entry is already mapped, the next
    /// table is returned directly.
    ///
    /// Returns `MapToError::FrameAllocationFailed` if the entry is unused and the allocator
    /// returned `None`. Returns `MapToError::ParentEntryHugePage` if the `HUGE_PAGE` flag is set
    /// in the passed entry.
    fn create_next_table<'b, A>(
        &self,
        entry: &'b mut PageTableEntry,
        insert_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<&'b mut PageTable, PageTableCreateError>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        let created;

        if entry.is_unused() {
            if let Some(frame) = allocator.allocate_frame() {
                entry.set_frame(frame, insert_flags);
                created = true;
            } else {
                return Err(PageTableCreateError::FrameAllocationFailed);
            }
        } else {
            if !insert_flags.is_empty() && !entry.flags().contains(insert_flags) {
                entry.set_flags(entry.flags() | insert_flags);
            }
            created = false;
        }

        let page_table = match self.next_table_mut(entry) {
            Err(PageTableWalkError::MappedToHugePage) => {
                return Err(PageTableCreateError::MappedToHugePage);
            }
            Err(PageTableWalkError::NotMapped) => panic!("entry should be mapped at this point"),
            Ok(page_table) => page_table,
        };

        if created {
            page_table.zero();
        }
        Ok(page_table)
    }
}

#[derive(Debug)]
enum PageTableWalkError {
    NotMapped,
    MappedToHugePage,
}

#[derive(Debug)]
enum PageTableCreateError {
    MappedToHugePage,
    FrameAllocationFailed,
}

impl From<PageTableCreateError> for MapToError<Size4KiB> {
    #[inline]
    fn from(err: PageTableCreateError) -> Self {
        match err {
            PageTableCreateError::MappedToHugePage => MapToError::ParentEntryHugePage,
            PageTableCreateError::FrameAllocationFailed => MapToError::FrameAllocationFailed,
        }
    }
}

impl From<PageTableCreateError> for MapToError<Size2MiB> {
    #[inline]
    fn from(err: PageTableCreateError) -> Self {
        match err {
            PageTableCreateError::MappedToHugePage => MapToError::ParentEntryHugePage,
            PageTableCreateError::FrameAllocationFailed => MapToError::FrameAllocationFailed,
        }
    }
}

impl From<PageTableCreateError> for MapToError<Size1GiB> {
    #[inline]
    fn from(err: PageTableCreateError) -> Self {
        match err {
            PageTableCreateError::MappedToHugePage => MapToError::ParentEntryHugePage,
            PageTableCreateError::FrameAllocationFailed => MapToError::FrameAllocationFailed,
        }
    }
}

impl From<FrameError> for PageTableWalkError {
    #[inline]
    fn from(err: FrameError) -> Self {
        match err {
            FrameError::HugeFrame => PageTableWalkError::MappedToHugePage,
            FrameError::FrameNotPresent => PageTableWalkError::NotMapped,
        }
    }
}

impl From<PageTableWalkError> for MapToError<Size4KiB> {
    #[inline]
    fn from(err: PageTableWalkError) -> Self {
        match err {
            PageTableWalkError::NotMapped => MapToError::FrameAllocationFailed,
            PageTableWalkError::MappedToHugePage => MapToError::ParentEntryHugePage,
        }
    }
}

impl From<PageTableWalkError> for MapToError<Size2MiB> {
    #[inline]
    fn from(err: PageTableWalkError) -> Self {
        match err {
            PageTableWalkError::NotMapped => MapToError::FrameAllocationFailed,
            PageTableWalkError::MappedToHugePage => MapToError::ParentEntryHugePage,
        }
    }
}

impl From<PageTableWalkError> for MapToError<Size1GiB> {
    #[inline]
    fn from(err: PageTableWalkError) -> Self {
        match err {
            PageTableWalkError::NotMapped => MapToError::FrameAllocationFailed,
            PageTableWalkError::MappedToHugePage => MapToError::ParentEntryHugePage,
        }
    }
}

impl FromResidual<Result<Infallible, PageTableWalkError>> for TranslateResult {
    fn from_residual(residual: Result<Infallible, PageTableWalkError>) -> Self {
        match residual {
            Ok(_) => {
                println!("FROM RISIDUAL ERROR THINGY!");
                loop {}
            },
            Err(err) => {
                match err {
                    PageTableWalkError::NotMapped => TranslateResult::NotMapped,
                    PageTableWalkError::MappedToHugePage => TranslateResult::InvalidFrameAddress(PhysAddr::new(0)), // FIXME: Is this correct?
                }
            },
        }
    }
}

impl From<PageTableWalkError> for UnmapError {
    #[inline]
    fn from(err: PageTableWalkError) -> Self {
        match err {
            PageTableWalkError::MappedToHugePage => UnmapError::ParentEntryHugePage,
            PageTableWalkError::NotMapped => UnmapError::PageNotMapped,
        }
    }
}

impl From<PageTableWalkError> for FlagUpdateError {
    #[inline]
    fn from(err: PageTableWalkError) -> Self {
        match err {
            PageTableWalkError::MappedToHugePage => FlagUpdateError::ParentEntryHugePage,
            PageTableWalkError::NotMapped => FlagUpdateError::PageNotMapped,
        }
    }
}

impl From<PageTableWalkError> for TranslateError {
    #[inline]
    fn from(err: PageTableWalkError) -> Self {
        match err {
            PageTableWalkError::MappedToHugePage => TranslateError::ParentEntryHugePage,
            PageTableWalkError::NotMapped => TranslateError::PageNotMapped,
        }
    }
}

/// Provides a virtual address mapping for physical page table frames.
///
/// This only works if the physical address space is somehow mapped to the virtual
/// address space, e.g. at an offset.
///
/// ## Safety
///
/// This trait is unsafe to implement because the implementer must ensure that
/// `frame_to_pointer` returns a valid page table pointer for any given physical frame.
pub unsafe trait PageTableFrameMapping {
    /// Translate the given physical frame to a virtual page table pointer.
    fn frame_to_pointer(&self, frame: PhysFrame) -> *mut PageTable;
}

/// A trait for types that can allocate a frame of memory.
///
/// This trait is unsafe to implement because the implementer must guarantee that
/// the `allocate_frame` method returns only unique unused frames.
pub unsafe trait FrameAllocator<S: PageSize> {
    /// Allocate a frame of the appropriate size and return it if possible.
    fn allocate_frame(&mut self) -> Option<PhysFrame<S>>;
}

/// A trait for types that can deallocate a frame of memory.
pub trait FrameDeallocator<S: PageSize> {
    /// Deallocate the given unused frame.
    ///
    /// ## Safety
    ///
    /// The caller must ensure that the passed frame is unused.
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<S>);
}


/// An empty convencience trait that requires the `Mapper` trait for all page sizes.
pub trait MapperAllSizes: Mapper<Size4KiB> + Mapper<Size2MiB> + Mapper<Size1GiB> {}

impl<T> MapperAllSizes for T where T: Mapper<Size4KiB> + Mapper<Size2MiB> + Mapper<Size1GiB> {}

/// Provides methods for translating virtual addresses.
pub trait Translate {
    /// Return the frame that the given virtual address is mapped to and the offset within that
    /// frame.
    ///
    /// If the given address has a valid mapping, the mapped frame and the offset within that
    /// frame is returned. Otherwise an error value is returned.
    ///
    /// This function works with huge pages of all sizes.
    fn translate(&self, addr: VirtAddr) -> TranslateResult;

    /// Translates the given virtual address to the physical address that it maps to.
    ///
    /// Returns `None` if there is no valid mapping for the given address.
    ///
    /// This is a convenience method. For more information about a mapping see the
    /// [`translate`](Translate::translate) method.
    #[inline]
    fn translate_addr(&self, addr: VirtAddr) -> Option<PhysAddr> {
        match self.translate(addr) {
            TranslateResult::NotMapped | TranslateResult::InvalidFrameAddress(_) => None,
            TranslateResult::Mapped { frame, offset, .. } => Some(frame.start_address() + offset),
        }
    }
}

/// The return value of the [`Translate::translate`] function.
///
/// If the given address has a valid mapping, a `Frame4KiB`, `Frame2MiB`, or `Frame1GiB` variant
/// is returned, depending on the size of the mapped page. The remaining variants indicate errors.
#[derive(Debug)]
pub enum TranslateResult {
    /// The virtual address is mapped to a physical frame.
    Mapped {
        /// The mapped frame.
        frame: MappedFrame,
        /// The offset whithin the mapped frame.
        offset: u64,
        /// The entry flags in the lowest-level page table.
        ///
        /// Flags of higher-level page table entries are not included here, but they can still
        /// affect the effective flags for an address, for example when the WRITABLE flag is not
        /// set for a level 3 entry.
        flags: PageTableFlags,
    },
    /// The given virtual address is not mapped to a physical frame.
    NotMapped,
    /// The page table entry for the given virtual address points to an invalid physical address.
    InvalidFrameAddress(PhysAddr),
}

/// Represents a physical frame mapped in a page table.
#[derive(Debug)]
pub enum MappedFrame {
    /// The virtual address is mapped to a 4KiB frame.
    Size4KiB(PhysFrame<Size4KiB>),
    /// The virtual address is mapped to a "large" 2MiB frame.
    Size2MiB(PhysFrame<Size2MiB>),
    /// The virtual address is mapped to a "huge" 1GiB frame.
    Size1GiB(PhysFrame<Size1GiB>),
}

impl MappedFrame {
    /// Returns the start address of the frame.
    pub const fn start_address(&self) -> PhysAddr {
        match self {
            MappedFrame::Size4KiB(frame) => frame.start_address,
            MappedFrame::Size2MiB(frame) => frame.start_address,
            MappedFrame::Size1GiB(frame) => frame.start_address,
        }
    }

    /// Returns the size the frame (4KB, 2MB or 1GB).
    pub const fn size(&self) -> u64 {
        match self {
            MappedFrame::Size4KiB(_) => Size4KiB::SIZE,
            MappedFrame::Size2MiB(_) => Size2MiB::SIZE,
            MappedFrame::Size1GiB(_) => Size1GiB::SIZE,
        }
    }
}

/// A trait for common page table operations on pages of size `S`.
pub trait Mapper<S: PageSize> {
    /// Creates a new mapping in the page table.
    ///
    /// This function might need additional physical frames to create new page tables. These
    /// frames are allocated from the `allocator` argument. At most three frames are required.
    ///
    /// Parent page table entries are automatically updated with `PRESENT | WRITABLE | USER_ACCESSIBLE`
    /// if present in the `PageTableFlags`. Depending on the used mapper implementation
    /// the `PRESENT` and `WRITABLE` flags might be set for parent tables,
    /// even if they are not set in `PageTableFlags`.
    ///
    /// The `map_to_with_table_flags` method gives explicit control over the parent page table flags.
    ///
    /// ## Safety
    ///
    /// Creating page table mappings is a fundamentally unsafe operation because
    /// there are various ways to break memory safety through it. For example,
    /// re-mapping an in-use page to a different frame changes and invalidates
    /// all values stored in that page, resulting in undefined behavior on the
    /// next use.
    ///
    /// The caller must ensure that no undefined behavior or memory safety
    /// violations can occur through the new mapping. Among other things, the
    /// caller must prevent the following:
    ///
    /// - Aliasing of `&mut` references, i.e. two `&mut` references that point to
    ///   the same physical address. This is undefined behavior in Rust.
    ///     - This can be ensured by mapping each page to an individual physical
    ///       frame that is not mapped anywhere else.
    /// - Creating uninitalized or invalid values: Rust requires that all values
    ///   have a correct memory layout. For example, a `bool` must be either a 0
    ///   or a 1 in memory, but not a 3 or 4. An exception is the `MaybeUninit`
    ///   wrapper type, which abstracts over possibly uninitialized memory.
    ///     - This is only a problem when re-mapping pages to different physical
    ///       frames. Mapping a page that is not in use yet is fine.
    ///
    /// Special care must be taken when sharing pages with other address spaces,
    /// e.g. by setting the `GLOBAL` flag. For example, a global mapping must be
    /// the same in all address spaces, otherwise undefined behavior can occur
    /// because of TLB races. It's worth noting that all the above requirements
    /// also apply to shared mappings, including the aliasing requirements.
    ///
    /// # Examples
    ///
    /// Create a USER_ACCESSIBLE mapping:
    ///
    /// ```
    /// # #[cfg(feature = "instructions")]
    /// # use x86_64::structures::paging::{
    /// #    Mapper, Page, PhysFrame, FrameAllocator,
    /// #    Size4KiB, OffsetPageTable, page_table::PageTableFlags
    /// # };
    /// # #[cfg(feature = "instructions")]
    /// # unsafe fn test(mapper: &mut OffsetPageTable, frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    /// #         page: Page<Size4KiB>, frame: PhysFrame) {
    ///         mapper
    ///           .map_to(
    ///               page,
    ///               frame,
    ///              PageTableFlags::PRESENT
    ///                   | PageTableFlags::WRITABLE
    ///                   | PageTableFlags::USER_ACCESSIBLE,
    ///               frame_allocator,
    ///           )
    ///           .unwrap()
    ///           .flush();
    /// # }
    /// ```
    #[inline]
    unsafe fn map_to<A>(
        &mut self,
        page: Page<S>,
        frame: PhysFrame<S>,
        flags: PageTableFlags,
        frame_allocator: &mut A,
    ) -> Result<MapperFlush<S>, MapToError<S>>
        where
            Self: Sized,
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        let parent_table_flags = flags
            & (PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::USER_ACCESSIBLE);

        unsafe {
            self.map_to_with_table_flags(page, frame, flags, parent_table_flags, frame_allocator)
        }
    }

    /// Creates a new mapping in the page table.
    ///
    /// This function might need additional physical frames to create new page tables. These
    /// frames are allocated from the `allocator` argument. At most three frames are required.
    ///
    /// The flags of the parent table(s) can be explicitly specified. Those flags are used for
    /// newly created table entries, and for existing entries the flags are added.
    ///
    /// Depending on the used mapper implementation, the `PRESENT` and `WRITABLE` flags might
    /// be set for parent tables, even if they are not specified in `parent_table_flags`.
    ///
    /// ## Safety
    ///
    /// Creating page table mappings is a fundamentally unsafe operation because
    /// there are various ways to break memory safety through it. For example,
    /// re-mapping an in-use page to a different frame changes and invalidates
    /// all values stored in that page, resulting in undefined behavior on the
    /// next use.
    ///
    /// The caller must ensure that no undefined behavior or memory safety
    /// violations can occur through the new mapping. Among other things, the
    /// caller must prevent the following:
    ///
    /// - Aliasing of `&mut` references, i.e. two `&mut` references that point to
    ///   the same physical address. This is undefined behavior in Rust.
    ///     - This can be ensured by mapping each page to an individual physical
    ///       frame that is not mapped anywhere else.
    /// - Creating uninitalized or invalid values: Rust requires that all values
    ///   have a correct memory layout. For example, a `bool` must be either a 0
    ///   or a 1 in memory, but not a 3 or 4. An exception is the `MaybeUninit`
    ///   wrapper type, which abstracts over possibly uninitialized memory.
    ///     - This is only a problem when re-mapping pages to different physical
    ///       frames. Mapping a page that is not in use yet is fine.
    ///
    /// Special care must be taken when sharing pages with other address spaces,
    /// e.g. by setting the `GLOBAL` flag. For example, a global mapping must be
    /// the same in all address spaces, otherwise undefined behavior can occur
    /// because of TLB races. It's worth noting that all the above requirements
    /// also apply to shared mappings, including the aliasing requirements.
    ///
    /// # Examples
    ///
    /// Create USER_ACCESSIBLE | NO_EXECUTE | NO_CACHE mapping and update
    /// the top hierarchy only with USER_ACCESSIBLE:
    ///
    /// ```
    /// # #[cfg(feature = "instructions")]
    /// # use x86_64::structures::paging::{
    /// #    Mapper, PhysFrame, Page, FrameAllocator,
    /// #    Size4KiB, OffsetPageTable, page_table::PageTableFlags
    /// # };
    /// # #[cfg(feature = "instructions")]
    /// # unsafe fn test(mapper: &mut OffsetPageTable, frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    /// #         page: Page<Size4KiB>, frame: PhysFrame) {
    ///         mapper
    ///           .map_to_with_table_flags(
    ///               page,
    ///               frame,
    ///              PageTableFlags::PRESENT
    ///                   | PageTableFlags::WRITABLE
    ///                   | PageTableFlags::USER_ACCESSIBLE
    ///                   | PageTableFlags::NO_EXECUTE
    ///                   | PageTableFlags::NO_CACHE,
    ///              PageTableFlags::USER_ACCESSIBLE,
    ///               frame_allocator,
    ///           )
    ///           .unwrap()
    ///           .flush();
    /// # }
    /// ```
    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: Page<S>,
        frame: PhysFrame<S>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        frame_allocator: &mut A,
    ) -> Result<MapperFlush<S>, MapToError<S>>
        where
            Self: Sized,
            A: FrameAllocator<Size4KiB> + ?Sized;

    /// Removes a mapping from the page table and returns the frame that used to be mapped.
    ///
    /// Note that no page tables or pages are deallocated.
    fn unmap(&mut self, page: Page<S>) -> Result<(PhysFrame<S>, MapperFlush<S>), UnmapError>;

    /// Updates the flags of an existing mapping.
    ///
    /// ## Safety
    ///
    /// This method is unsafe because changing the flags of a mapping
    /// might result in undefined behavior. For example, setting the
    /// `GLOBAL` and `MUTABLE` flags for a page might result in the corruption
    /// of values stored in that page from processes running in other address
    /// spaces.
    unsafe fn update_flags(
        &mut self,
        page: Page<S>,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<S>, FlagUpdateError>;

    /// Return the frame that the specified page is mapped to.
    ///
    /// This function assumes that the page is mapped to a frame of size `S` and returns an
    /// error otherwise.
    fn translate_page(&self, page: Page<S>) -> Result<PhysFrame<S>, TranslateError>;

    /// Maps the given frame to the virtual page with the same address.
    ///
    /// ## Safety
    ///
    /// This is a convencience function that invokes [`Mapper::map_to`] internally, so
    /// all safety requirements of it also apply for this function.
    #[inline]
    unsafe fn identity_map<A>(
        &mut self,
        frame: PhysFrame<S>,
        flags: PageTableFlags,
        frame_allocator: &mut A,
    ) -> Result<MapperFlush<S>, MapToError<S>>
        where
            Self: Sized,
            A: FrameAllocator<Size4KiB> + ?Sized,
            S: PageSize,
            Self: Mapper<S>,
    {
        let page = Page::containing_address(VirtAddr::new(frame.start_address().as_u64()));
        unsafe { self.map_to(page, frame, flags, frame_allocator) }
    }
}

/// This type represents a page whose mapping has changed in the page table.
///
/// The old mapping might be still cached in the translation lookaside buffer (TLB), so it needs
/// to be flushed from the TLB before it's accessed. This type is returned from function that
/// change the mapping of a page to ensure that the TLB flush is not forgotten.
#[derive(Debug)]
#[must_use = "Page Table changes must be flushed or ignored."]
pub struct MapperFlush<S: PageSize>(Page<S>);

impl<S: PageSize> MapperFlush<S> {
    /// Create a new flush promise
    ///
    /// Note that this method is intended for implementing the [`Mapper`] trait and no other uses
    /// are expected.
    #[inline]
    pub fn new(page: Page<S>) -> Self {
        MapperFlush(page)
    }

    /// Flush the page from the TLB to ensure that the newest mapping is used.
    #[inline]
    pub fn flush(self) {
        flush(self.0.start_address());
    }

    /// Don't flush the TLB and silence the “must be used” warning.
    #[inline]
    pub fn ignore(self) {}
}

#[inline]
fn flush(addr: VirtAddr) {
    unsafe {
        asm!("invlpg [{}]", in(reg) addr.as_u64(), options(nostack, preserves_flags));
    }
}

/// This type represents a change of a page table requiring a complete TLB flush
///
/// The old mapping might be still cached in the translation lookaside buffer (TLB), so it needs
/// to be flushed from the TLB before it's accessed. This type is returned from a function that
/// made the change to ensure that the TLB flush is not forgotten.
#[derive(Debug, Default)]
#[must_use = "Page Table changes must be flushed or ignored."]
pub struct MapperFlushAll(());

impl MapperFlushAll {
    /// Create a new flush promise
    ///
    /// Note that this method is intended for implementing the [`Mapper`] trait and no other uses
    /// are expected.
    #[inline]
    pub fn new() -> Self {
        MapperFlushAll(())
    }

    /// Flush all pages from the TLB to ensure that the newest mapping is used.
    #[cfg(feature = "instructions")]
    #[inline]
    pub fn flush_all(self) {
        crate::instructions::tlb::flush_all()
    }

    /// Don't flush the TLB and silence the “must be used” warning.
    #[inline]
    pub fn ignore(self) {}
}

/// This error is returned from `map_to` and similar methods.
#[derive(Debug)]
pub enum MapToError<S: PageSize> {
    /// An additional frame was needed for the mapping process, but the frame allocator
    /// returned `None`.
    FrameAllocationFailed,
    /// An upper level page table entry has the `HUGE_PAGE` flag set, which means that the
    /// given page is part of an already mapped huge page.
    ParentEntryHugePage,
    /// The given page is already mapped to a physical frame.
    PageAlreadyMapped(PhysFrame<S>),
}

/// An error indicating that an `unmap` call failed.
#[derive(Debug)]
pub enum UnmapError {
    /// An upper level page table entry has the `HUGE_PAGE` flag set, which means that the
    /// given page is part of a huge page and can't be freed individually.
    ParentEntryHugePage,
    /// The given page is not mapped to a physical frame.
    PageNotMapped,
    /// The page table entry for the given page points to an invalid physical address.
    InvalidFrameAddress(PhysAddr),
}

/// An error indicating that an `update_flags` call failed.
#[derive(Debug)]
pub enum FlagUpdateError {
    /// The given page is not mapped to a physical frame.
    PageNotMapped,
    /// An upper level page table entry has the `HUGE_PAGE` flag set, which means that the
    /// given page is part of a huge page and can't be freed individually.
    ParentEntryHugePage,
}

/// An error indicating that an `translate` call failed.
#[derive(Debug)]
pub enum TranslateError {
    /// The given page is not mapped to a physical frame.
    PageNotMapped,
    /// An upper level page table entry has the `HUGE_PAGE` flag set, which means that the
    /// given page is part of a huge page and can't be freed individually.
    ParentEntryHugePage,
    /// The page table entry for the given page points to an invalid physical address.
    InvalidFrameAddress(PhysAddr),
}

static _ASSERT_OBJECT_SAFE: Option<&(dyn Translate + Sync)> = None;

/// Provides methods for cleaning up unused entries.
pub trait CleanUp {
    /// Remove all empty P1-P3 tables
    ///
    /// ## Safety
    ///
    /// The caller has to guarantee that it's safe to free page table frames:
    /// All page table frames must only be used once and only in this page table
    /// (e.g. no reference counted page tables or reusing the same page tables for different virtual addresses ranges in the same page table).
    unsafe fn clean_up<D>(&mut self, frame_deallocator: &mut D)
        where
            D: FrameDeallocator<Size4KiB>;

    /// Remove all empty P1-P3 tables in a certain range
    /// ```
    /// # use core::ops::RangeInclusive;
    /// # use x86_64::{VirtAddr, structures::paging::{
    /// #    FrameDeallocator, Size4KiB, mapper::CleanUp, page::Page,
    /// # }};
    /// # unsafe fn test(page_table: &mut impl CleanUp, frame_deallocator: &mut impl FrameDeallocator<Size4KiB>) {
    /// // clean up all page tables in the lower half of the address space
    /// let lower_half = Page::range_inclusive(
    ///     Page::containing_address(VirtAddr::new(0)),
    ///     Page::containing_address(VirtAddr::new(0x0000_7fff_ffff_ffff)),
    /// );
    /// page_table.clean_up_addr_range(lower_half, frame_deallocator);
    /// # }
    /// ```
    ///
    /// ## Safety
    ///
    /// The caller has to guarantee that it's safe to free page table frames:
    /// All page table frames must only be used once and only in this page table
    /// (e.g. no reference counted page tables or reusing the same page tables for different virtual addresses ranges in the same page table).
    unsafe fn clean_up_addr_range<D>(
        &mut self,
        range: PageRangeInclusive,
        frame_deallocator: &mut D,
    ) where
        D: FrameDeallocator<Size4KiB>;
}

/// A Mapper implementation that requires that the complete physically memory is mapped at some
/// offset in the virtual address space.
#[derive(Debug)]
pub struct OffsetPageTable<'a> {
    inner: MappedPageTable<'a, PhysOffset>,
}

impl<'a> OffsetPageTable<'a> {
    /// Creates a new `OffsetPageTable` that uses the given offset for converting virtual
    /// to physical addresses.
    ///
    /// The complete physical memory must be mapped in the virtual address space starting at
    /// address `phys_offset`. This means that for example physical address `0x5000` can be
    /// accessed through virtual address `phys_offset + 0x5000`. This mapping is required because
    /// the mapper needs to access page tables, which are not mapped into the virtual address
    /// space by default.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because the caller must guarantee that the passed `phys_offset`
    /// is correct. Also, the passed `level_4_table` must point to the level 4 page table
    /// of a valid page table hierarchy. Otherwise this function might break memory safety, e.g.
    /// by writing to an illegal memory location.
    #[inline]
    pub unsafe fn new(top_level_table: &'a mut PageTable, phys_offset: VirtAddr) -> Self {
        let phys_offset = PhysOffset {
            offset: phys_offset,
        };
        Self {
            inner: unsafe { MappedPageTable::new(top_level_table, phys_offset) },
        }
    }

    /// Returns a mutable reference to the wrapped top level `PageTable` instance.
    #[inline]
    pub fn top_level_table(&mut self) -> &mut PageTable {
        self.inner.top_level_table()
    }
}

#[derive(Debug)]
struct PhysOffset {
    offset: VirtAddr,
}

unsafe impl PageTableFrameMapping for PhysOffset {
    fn frame_to_pointer(&self, frame: PhysFrame) -> *mut PageTable {
        let virt = self.offset + frame.start_address().as_u64();
        virt.as_mut_ptr()
    }
}

// delegate all trait implementations to inner

impl<'a> Mapper<Size1GiB> for OffsetPageTable<'a> {
    #[inline]
    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: Page<Size1GiB>,
        frame: PhysFrame<Size1GiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size1GiB>, MapToError<Size1GiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        unsafe {
            self.inner
                .map_to_with_table_flags(page, frame, flags, parent_table_flags, allocator)
        }
    }

    #[inline]
    fn unmap(
        &mut self,
        page: Page<Size1GiB>,
    ) -> Result<(PhysFrame<Size1GiB>, MapperFlush<Size1GiB>), UnmapError> {
        self.inner.unmap(page)
    }

    #[inline]
    unsafe fn update_flags(
        &mut self,
        page: Page<Size1GiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<Size1GiB>, FlagUpdateError> {
        unsafe { self.inner.update_flags(page, flags) }
    }

    #[inline]
    fn translate_page(&self, page: Page<Size1GiB>) -> Result<PhysFrame<Size1GiB>, TranslateError> {
        self.inner.translate_page(page)
    }
}

impl<'a> Mapper<Size2MiB> for OffsetPageTable<'a> {
    #[inline]
    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: Page<Size2MiB>,
        frame: PhysFrame<Size2MiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size2MiB>, MapToError<Size2MiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        unsafe {
            self.inner
                .map_to_with_table_flags(page, frame, flags, parent_table_flags, allocator)
        }
    }

    #[inline]
    fn unmap(
        &mut self,
        page: Page<Size2MiB>,
    ) -> Result<(PhysFrame<Size2MiB>, MapperFlush<Size2MiB>), UnmapError> {
        self.inner.unmap(page)
    }

    #[inline]
    unsafe fn update_flags(
        &mut self,
        page: Page<Size2MiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<Size2MiB>, FlagUpdateError> {
        unsafe { self.inner.update_flags(page, flags) }
    }

    /*
    #[inline]
    unsafe fn set_flags_p4_entry(
        &mut self,
        page: Page<Size2MiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlushAll, FlagUpdateError> {
        unsafe { self.inner.set_flags_p4_entry(page, flags) }
    }

    #[inline]
    unsafe fn set_flags_p3_entry(
        &mut self,
        page: Page<Size2MiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlushAll, FlagUpdateError> {
        unsafe { self.inner.set_flags_p3_entry(page, flags) }
    }

    #[inline]
    unsafe fn set_flags_p2_entry(
        &mut self,
        page: Page<Size2MiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlushAll, FlagUpdateError> {
        unsafe { self.inner.set_flags_p2_entry(page, flags) }
    }*/

    #[inline]
    fn translate_page(&self, page: Page<Size2MiB>) -> Result<PhysFrame<Size2MiB>, TranslateError> {
        self.inner.translate_page(page)
    }
}

impl<'a> Mapper<Size4KiB> for OffsetPageTable<'a> {
    #[inline]
    unsafe fn map_to_with_table_flags<A>(
        &mut self,
        page: Page<Size4KiB>,
        frame: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
        parent_table_flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size4KiB>, MapToError<Size4KiB>>
        where
            A: FrameAllocator<Size4KiB> + ?Sized,
    {
        unsafe {
            self.inner
                .map_to_with_table_flags(page, frame, flags, parent_table_flags, allocator)
        }
    }

    #[inline]
    fn unmap(
        &mut self,
        page: Page<Size4KiB>,
    ) -> Result<(PhysFrame<Size4KiB>, MapperFlush<Size4KiB>), UnmapError> {
        self.inner.unmap(page)
    }

    #[inline]
    unsafe fn update_flags(
        &mut self,
        page: Page<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<Size4KiB>, FlagUpdateError> {
        unsafe { self.inner.update_flags(page, flags) }
    }

    /*
    #[inline]
    unsafe fn set_flags_p4_entry(
        &mut self,
        page: Page<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlushAll, FlagUpdateError> {
        unsafe { self.inner.set_flags_p4_entry(page, flags) }
    }

    #[inline]
    unsafe fn set_flags_p3_entry(
        &mut self,
        page: Page<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlushAll, FlagUpdateError> {
        unsafe { self.inner.set_flags_p3_entry(page, flags) }
    }

    #[inline]
    unsafe fn set_flags_p2_entry(
        &mut self,
        page: Page<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<MapperFlushAll, FlagUpdateError> {
        unsafe { self.inner.set_flags_p2_entry(page, flags) }
    }*/

    #[inline]
    fn translate_page(&self, page: Page<Size4KiB>) -> Result<PhysFrame<Size4KiB>, TranslateError> {
        self.inner.translate_page(page)
    }
}

impl<'a> Translate for OffsetPageTable<'a> {
    #[inline]
    fn translate(&self, addr: VirtAddr) -> TranslateResult {
        self.inner.translate(addr)
    }
}

impl<'a> CleanUp for OffsetPageTable<'a> {
    #[inline]
    unsafe fn clean_up<D>(&mut self, frame_deallocator: &mut D)
        where
            D: FrameDeallocator<Size4KiB>,
    {
        unsafe { self.inner.clean_up(frame_deallocator) }
    }

    #[inline]
    unsafe fn clean_up_addr_range<D>(
        &mut self,
        range: PageRangeInclusive,
        frame_deallocator: &mut D,
    ) where
        D: FrameDeallocator<Size4KiB>,
    {
        unsafe { self.inner.clean_up_addr_range(range, frame_deallocator) }
    }
}
