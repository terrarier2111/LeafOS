use crate::mem::addr::PhysAddr;

// FIXME: create heap in order to have a place where prepared frames(ones that are already inserted in the page table) can stay
// FIXME: or otherwise find a different solution to how to avoid inserting frames into the page table twice

// FIXME: see: https://wiki.syslinux.org/wiki/index.php?title=Heap_Management





// FIXME: the solution probably is to allocate a heap which has at least size N but if we have enough memory on the system we can allocate
// FIXME: up to some relative max value but we *PROBABLY* should cap this limit

// FIXME: If we run out of heap space, we will simply allocate more frames on demand and insert them into the page table
// FIXME: and once we're done with them, we will clear the relevant page table entries (for that we can simply check if
// FIXME: a frame is contained in the page table on deallocation) - we should probably also zero out all frames which are
// FIXME: released and were previously used by the kernel

// for now we are just using a 2MB heap as that's an order 9 entry which is the biggest we currently support
pub struct Heap {
    start: *mut u8,
    size: usize,
    // FIXME: unfortunately we probably have to use a buddy system in here as well (which is a lot of boilerplate code)
}

impl Heap {

    pub fn new(start: *mut u8) -> Self {
        // FIXME: insert every frame into the page table
        Self {
            start,
            size: 4096 * (1 << 9)
        }
    }

}
