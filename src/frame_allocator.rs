// there are 2 possible design approaches:

// 1: full tree structure:
// replicate the page table's layout (somewhat) and store 1 bit per page in order to determine whether it's allocated or not
// and have a 4/5 level tree structure in order to find a free page (also have counters for how many sub pages are free in a top page)
// this might allow for opportunistic allocations of metadata

// 2: partial tree structure:
// have a 1/2 level tree structure for pages of multiple orders (also store the amount of free sub pages) this might reduce indirections
// on allocation/deallocation

// 3: varyingly levelled tree structure:
// have a 1/2 level tree structure for pages of multiple orders (but increase the amount of levels when going toward smaller page sizes)
// (also store the amount of free sub pages this might reduce indirections on allocation/deallocation

// 4: regional tree:
// just have a partial tree structure and make it per-cpu (similar to how LLFree does it) but have top level pages for these per-cpu
// memory regions as well which can be used to claim a large chunk of memory immediately as on allocation of lower order regions
// (such as these per-cpu regions) they have to traverse the binary tree bottom up and announce their desire to claim a page
// but if they detect that a top region is already being claimed, just give up and let the page be claimed. Now it has to try to
// claim another page in another chunk which can be found by traversing the top level structure. This is basically like the full tree structure
// with little indirection in case of smaller allocations and less contention on average on the local counters (as all allocations happen from the same cpu)
// but sacrificing performance when larger allocations happen. Also we allocate from the left of the trees to the right in order to reduce fragmentation
// this can be done easily in the tree structure (but we may sacrifice a bit of performance for that).
// we could also avoid false sharing by doubling the amount of metadata (having 1 set for the local thread and 1 set for the other threads).
// But this is likely not worth the little performance boost. Maybe we could alternatively just keep the duplicated metadata per-cpu and not
// duplicate the whole metadata even for chunks that aren't currently claimed by a certain cpu. So the metadata would be per-cpu and not
// per-chunk. This means the additional metadata would only be `metadata_per_chunk * cpu_cores` and not `metadata_per_chunk * chunks`.
// This may actually be worth it. To merge local metadata with the persistent metadata we could just subtract the reference count of the local metadata
// from the reference count of the persistent metadata and then check when the persistent counter reaches 0, we have to ensure tho that the local counter is disconnected
// for example through a bitflag set in the persistent counter.
// There would still be false-sharing for the individual bit flags tho (on dealloc).
// We don't have to store any pointers as we simply set used bits for all frames that are unusable.
// So we can just use some base and add offsets to it to determine the address of a specific entry in the table.
// Also claim 2 different pages per core, one for completely free chunks and one for maybe partially free chunks.
// The completely free chunks will have additional metadata indicating whether the whole chunk is free or not.
// In order to do this efficiently we have to store this additional metadata for all layers except for the final layer.
// We do not however have to maintain this metadata for all chunks.
// FIXME: Note, that there is an alternative approach requiring less additional metadata, we simply
// store the number of used pages. This will however introduce 1 additional atomic rmw operation
// on every alloc/free call.
// To maintain this additional metadata a chunk type indicator has to be stored in every chunk.


use core::{sync::atomic::AtomicUsize, ptr::NonNull};

use crate::{sc_cell::SCCell, util::build_bit_mask};

pub struct FrameAllocator {
    used_pages: AtomicUsize,

}

impl FrameAllocator {

    #[inline]
    pub const fn new() -> Self {
        Self {
            used_pages: AtomicUsize::new(0),
        }
    }

    pub fn alloc_frames(frame_cnt: usize) -> *mut () {

    }

}

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
struct LayerInfo(usize);

impl LayerInfo {

    #[inline]
    pub const fn new(id: usize, ty: LayerTy) -> Self {
        Self(id | ((ty as usize) << LAYER_TY_INFO_OFFSET))
    }

    #[inline]
    pub const fn ty(self) -> LayerTy {
        LayerTy::from_raw(self.0 >> LAYER_TY_INFO_OFFSET)
    }

    #[inline]
    pub const fn id(self) -> usize {
        self.0 & ID_MASK
    }

}

const ID_MASK: usize = build_bit_mask(0, LAYER_TY_INFO_OFFSET);
const LAYER_TY_INFO_SIZE: usize = usize::BITS - LAYER_TY_INFO_OFFSET;
const LAYER_TY_INFO_OFFSET: usize = (LayerTy::Last as usize).trailing_zeros();

#[repr(usize)]
enum LayerTy {
    Normal = 0,
    Emptied = 1,
    Last,
}

impl LayerTy {

    const SIZE: usize = Self::Last as usize;
    const MAPPING: [LayerTy; Self::SIZE] = [Self::Normal, Self::Emptied];

    #[inline]
    const fn from_raw(raw: usize) -> Self {
        Self::MAPPING[raw]
    }

}

struct Layer {
    info: LayerInfo,
    // this looks up where in our metadata there is a free entry
    top_lookup: AtomicUsize, // FIXME: it will probably be very hard to maintain this without races from lower_lookup, we could detect when setting a bit raced with
                             // the bit being unset and somehow handle such a race
    any_free: [AtomicUsize; METADATA_WORDS],
    all_free: [AtomicUsize; METADATA_WORDS],
}

impl Layer {

    fn alloc(&self, size: usize) -> Option<NonNull<()>> {
        
    }

    fn find_free_consecutive(&self, pages: usize) -> Option<NonNull<()>> {
        let top_rows = pages.div_ceil(usize::BITS);
        
        let bitset = self.top_lookup.load(core::sync::atomic::Ordering::Acquire);
        let mut ret = bitset;
        for i in 1..pages {
            ret &= (bitset >> i);
        }

    }

}

struct FinalLayer {
    // this looks up where in our metadata there is a free entry
    top_lookup: AtomicUsize, // FIXME: it will probably be very hard to maintain this without races from lower_lookup, we could detect when setting a bit raced with
                             // the bit being unset and somehow handle such a race
    lower_lookup: [AtomicUsize; METADATA_WORDS],
}

impl FinalLayer {



}

const LAYER_MULTIPLIERS: [usize; 4] = [0, 0, 0, 0]; // FIXME: implement this!

const ENTRIES_PER_INDIRECTION: usize = 4096;
const METADATA_WORDS: usize = ENTRIES_PER_INDIRECTION / usize::BITS;


