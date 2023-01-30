/// A collection of contiguous pages
struct Slab {

}

/// A collection of slabs
struct Cache {

}
// FIXME: How do we store multiple slabs (as we don't know how many will be there) - should we use linked lists?

// references:
// https://www.geeksforgeeks.org/operating-system-allocating-kernel-memory-buddy-system-slab-system/
// https://github.com/Andy-Python-Programmer/aero/blob/master/src/aero_kernel/src/mem/alloc.rs
// https://medium.com/howsofcoding/custom-memory-allocator-malloc-62d28e10bfb8
// https://www.youtube.com/watch?v=rsp0rBP61As
