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
// Note that in this design if there is a request for 63 free pages (on a 64 bit system) and the occupation looks like this: [1100..[60*0]], [[62*0]..11], ..
// then no suitable page would be found although there is enough storage. This is because the metadata of things that are allocated may not overlap.
// (except if the size is suitably large - for example for a request of 100 pages, 2 pieces of metadata could be used).
// Also note that it's not possible to allocate metadata as needed (on-the-fly) as that would require us to store pointers to the regions.
// Even if the all_free metadata optimization would be a tremendous improvement for the runtime, the actual implementation is not easy/possible
// with the decribed metadata design because of various opportunities for data races. But the good thing is we may still be able to employ
// this technique for per-cpu chunks, as there won't be parallel allocations, only 1 allocation and N frees at a time.


use core::{sync::atomic::{AtomicUsize, Ordering}, ptr::{NonNull, null_mut}};

use crate::{sc_cell::SCCell, util::{build_bit_mask, SyncPtrMut, greater_zero_ret_one}};

pub struct FrameAllocator {
    used_pages: AtomicUsize,
    initial_layer: GenericLayer,
}

impl FrameAllocator {

    #[inline]
    pub const fn new() -> Self {
        Self {
            used_pages: AtomicUsize::new(0),
            initial_layer: todo!(),
        }
    }

    pub fn alloc_frames(&self, frame_cnt: usize) -> *mut () {

    }

    pub fn dealloc_frames(&self, ptr: *mut (), frame_cnt: usize) {

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
    // Emptied = 1,
    Last,
}

impl LayerTy {

    const SIZE: usize = Self::Last as usize;
    const MAPPING: [LayerTy; Self::SIZE] = [Self::Normal/*, Self::Emptied*/];

    #[inline]
    const fn from_raw(raw: usize) -> Self {
        Self::MAPPING[raw]
    }

}

static LAYER_START_ADDRS: [SCCell<SyncPtrMut<()>>; 4] = [SCCell::new(SyncPtrMut(null_mut())); 4];

struct GenericLayer {
    info: LayerInfo,
    // this looks up where in our metadata there is a free entry
    any_free_top_lookup: AtomicUsize, // FIXME: it will probably be very hard to maintain this without races from lower_lookup, we could detect when setting a bit raced with
                             // the bit being unset and somehow handle such a race
    any_free: [AtomicUsize; METADATA_WORDS],
    all_free_top_lookup: AtomicUsize,
    all_free: [AtomicUsize; METADATA_WORDS],
}

impl GenericLayer {

    fn new(info: LayerInfo) -> Self {
        Self {
            info,
            any_free_top_lookup: AtomicUsize::new(usize::MAX),
            any_free: [AtomicUsize::new(usize::MAX); METADATA_WORDS],
            all_free_top_lookup: AtomicUsize::new(usize::MAX),
            all_free: [AtomicUsize::new(usize::MAX); METADATA_WORDS],
        }
    }

    fn alloc(&self, size: usize, this_base: *mut ()) -> Option<NonNull<()>> {
        let own_pages = size.div_ceil(LAYER_MULTIPLIERS[self.info.id()]);
        let excess_pages = own_pages * LAYER_MULTIPLIERS[self.info.id()] - size;
        self.find_free_consecutive(this_base, own_pages, excess_pages)
    }

    fn find_free_any(&self, this_base: *mut (), pages: usize) -> Option<NonNull<()>> {
        let multiplier = LAYER_MULTIPLIERS[self.info.id()];
        'outer: loop {            
            let bitset = self.any_free_top_lookup.load(Ordering::Acquire);

            if bitset == 0 {
                return None;
            }

            let set = 1 << bitset.trailing_zeros();
            let last_page = ret.trailing_zeros() as usize;
            let mut prev = self.all_free_top_lookup.load(Ordering::Acquire);
            loop {
                // set the entry to not fully free
                match self.all_free_top_lookup.compare_exchange_weak(prev, prev & !set, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => break 'outer,
                    Err(err) => {
                        if err & set != set {
                            continue 'outer;
                        }
                        prev = err;
                    },
                }
            }
            if self.any_free_top_lookup.load(Ordering::Acquire) & set == 0 {
                // the page we tried to allocate was just allocated by somebody else :(
                // retry allocating an entry
                continue;
            }
            // now we know nobody could claim pages from this list, so update it.
            let curr_pages_any = pages.div_ceil(multiplier);
            let curr_pages_all = pages.div_floor(multiplier);
            let mut free_bits_any = self.any_free[set * multiplier].load(Ordering::Acquire);
            while free_bits_any != 0 {
                let bitset_all = {
                    let mut bitset = free_bits_any;
                    for i in 1..curr_pages_all {
                        bitset &= free_bits_any >> 1;
                    }
                    bitset
                };
                let bitset = build_bit_mask(bitset.trailing_zeros(), curr_pages);
                match self.any_free[set * multiplier].compare_exchange(free_bits_any, free_bits_any & !bitset, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => {
                        let sub_pages = curr_pages * multiplier - pages;
                        // FIXME: handle cases in which we don't need a full page and just need a couple subpages
                        break 'outer;
                    },
                    Err(err) => {
                        free_bits = err;
                    },
                }
            }
        }

        Some(unsafe { NonNull::new_unchecked(base.cast::<u8>().add(multiplier * (1 << ret.trailing_zeros())).cast::<()>()) })
    }

    /// Finds consecutive bits and allocates them, lower_excess_pages denotes how many sub pages of the last page
    /// are not required and can be made available to other callers.
    fn find_free_consecutive(&self, base: *mut (), pages: usize, lower_excess_pages: usize) -> Option<NonNull<()>> {
        'outer: loop {
            let top_rows = pages.div_ceil(usize::BITS);
            let excess_pages = pages * usize::BITS as usize - pages;
            
            let mut bitset = self.all_free_top_lookup.load(Ordering::Acquire);
            let mut ret = bitset;
            for i in 1..pages {
                ret &= (bitset >> i);
            }

            if ret == 0 {
                return None;
            }

            let set = build_bit_mask(ret.trailing_zeros(), pages);
            let last_page = ret.trailing_zeros() as usize + pages - 1;
            loop {
                // try claiming the whole range for us in the most general list (the one with the weakest guarantees)
                match self.any_free_top_lookup.compare_exchange_weak(bitset, bitset & !set, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => break 'outer,
                    Err(err) => {
                        if err & set != set {
                            continue 'outer;
                        }
                        bitset = self.all_free_top_lookup.load(Ordering::Acquire);
                    },
                }
            }
            // now we know nobody could claim pages from this list, so update it.
            self.all_free_top_lookup.fetch_and(!set, Ordering::AcqRel);
            if excess_pages > 0 {
                let bit_set = build_bit_mask(usize::BITS - /*1 - */excess_pages, excess_pages);
                self.any_free[last_page].fetch_and(bit_set, Ordering::Relaxed);
            }
            // FIXME: could this be reordered before the fetch_and on all_free_top_lookup?
            self.any_free_top_lookup.fetch_or(1 << last_page, Ordering::AcqRel);
            // FIXME: use lower_excess_pages to free up more space
            break;
        }

        Some(unsafe { NonNull::new_unchecked(base.cast::<u8>().add(LAYER_MULTIPLIERS[self.info.id()] * (1 << ret.trailing_zeros())).cast::<()>()) })
    }

     /// Frees `pages` pages starting from `start`
     fn free_at<const FROM_LEFT: bool, const CLEAR_OTHER: bool>(&self, base: *mut (), offset: usize, pages: usize) {
       
    }

     /// Frees `pages` pages starting from the start
     fn free_from_start(&self, pages: usize) {
        self.free_at::<true>(0, 0, pages);
    }

    /// Frees `cnt` pages starting from the end
    fn free_from_end(&self, pages: usize) {
        self.free_at::<false>(0, 0, pages);
    }

    /// Frees `pages` pages starting from `start`
    fn free_at_clear_other<const FROM_LEFT: bool>(&self, offset: usize, pages: usize) {
        
    }

     /// Frees `pages` pages starting from the start
     fn free_from_start_clear_other(&self, pages: usize) {
        self.free_at_clear_other(0, pages);
    }

    /// Frees `cnt` pages starting from the end
    fn free_from_end_clear_other(&self, cnt: usize) {
        self.free_at_clear_other(0, cnt);
    }

}

struct FinalLayer {
    locked: AtomicUsize,
    // this looks up where in our metadata there is a free entry
    all_free_top_lookup: AtomicUsize,
    any_free_top_lookup: AtomicUsize, // FIXME: it will probably be very hard to maintain this without races from lower_lookup, we could detect when setting a bit raced with
                             // the bit being unset and somehow handle such a race
    lower_lookup: [AtomicUsize; METADATA_WORDS],
}

impl FinalLayer {

    fn find_free_any(&self, this_base: *mut (), pages: usize) -> Option<NonNull<()>> {
        
    }

    /// Finds consecutive bits and allocates them, lower_excess_pages denotes how many sub pages of the last page
    /// are not required and can be made available to other callers.
    fn find_free_consecutive(&self, base: *mut (), pages: usize, lower_excess_pages: usize) -> Option<NonNull<()>> {
        
    }

}

pub trait Layer {

    fn free_layer<const FROM_LEFT: bool, const CLEAR_OTHER: bool>(&self, base: *mut (), offset: usize, pages: usize);

    fn is_final(&self) -> bool;

    fn get_multiplier(&self) -> usize;

}

impl Layer for GenericLayer {
    fn free_layer<const FROM_LEFT: bool, const CLEAR_OTHER: bool>(&self, base: *mut (), offset: usize, pages: usize) {
        let multiplier = self.get_multiplier();
        let top_multiplier = multiplier * usize::BITS as usize;

        let (entry_all, bitset_entry_front_all, bitset_entry_back_all, remaining_front, remaining_back, bitset_top_all) = {
            // say we have chunks of size 4 and top chunks of size 8 and all the 0 should be replaced with 1
            // [1100000000000000]
            // [..00..] first calculate the remaining front part (2 would be the amount of that in this case)
            let remaining_front = (multiplier - offset % multiplier) % multiplier;
            // [....0000..] calculate the number of chunks until a full top level entry is reached (1 would be the amount in this case)
            let remaining_entries_front = top_multiplier - (offset + remaining_front) % top_multiplier;

            // calculate how many full entries (that should be freed) will follow the front part
            let entry_cnt_base = (pages - remaining_front).div_floor(multiplier);
            // calculate how many top entries there actually are (in between the front and back parts)
            let top_entry_cnt = (entry_cnt_base - remaining_entries_front).div_floor(usize::BITS);

            // [...00000001..] calculate the whole remaining back part (12 would be the amount in this case)
            let remaining = pages - remaining_front - remaining_entries_front * multiplier - top_entry_cnt * top_multiplier;
            // [...........00001] calculate the remaining back part (4 would be the amount in this case)
            let remaining_back = remaining % multiplier;
            // [...0000.....] calculate the remaining back entries (1 would be the amount in this case)
            let remaining_entries_back = remaining / multiplier;
            
            let entry = offset.div_floor(multiplier);
            let off = if FROM_LEFT {
                entry
            } else {
                ENTRIES_PER_INDIRECTION - 1 - entry
            };
            // we have to offset this as the entry is not the actual offset we want becasue of directionality
            let top_off = if FROM_LEFT {
                entry
            } else {
                usize::BITS - top_entry_cnt
            };
            let top_bitset = build_bit_mask(top_off, top_entry_cnt);

            (off, remaining_entries_front, remaining_entries_back, remaining_front, remaining_back, top_bitset)
        };

        let (entry_any, bitset_entry_front_any, bitset_entry_back_any, bitset_top_any) = {
            // FIXME: finish this up!
            let remaining_front_all = (top_multiplier - offset % top_multiplier) % top_multiplier;
            let remaining_front_pages = remaining_front_all.div_ceil(multiplier);
            let remaining_front_rest = remaining_front_all - (remaining_front_all.div_floor(multiplier) * multiplier);
            // calculate how many partial(or full) entries (that should be freed) will follow the front part
            let entry_cnt_base = (pages - remaining_front_rest).div_floor(multiplier);
            let top_entry_cnt = (entry_cnt_base - entries_remaining_front).div_floor(usize::BITS);
            let remaining_back_all = pages - remaining_front_all - top_entry_cnt * top_multiplier;
            let entries_remaining_back = remaining_back_all.div_ceil(multiplier);
            let entry = offset.div_floor(multiplier);
            let off = if FROM_LEFT {
                entry
            } else {
                ENTRIES_PER_INDIRECTION - 1 - entry
            };
            let top_entry_cnt = top_entry_cnt + greater_zero_ret_one(remaining_front_all) + greater_zero_ret_one(remaining_back_all);
            // we have to offset this as the entry is not the actual offset we want becasue of directionality
            let top_off = if FROM_LEFT {
                entry
            } else {
                usize::BITS - top_entry_cnt
            };
            let top_bitset = build_bit_mask(top_off, top_entry_cnt);

            (off, entries_remaining_front, entries_remaining_back, top_bitset)
        };
        
        // FIXME: free up remaining entries!

        if CLEAR_OTHER {
            self.any_free[entry_any].store(bitset_entry_front_any, Ordering::Release);
            self.all_free[entry_all].store(bitset_entry_front_all, Ordering::Release);
            self.any_free[entry_any + bitset_entry_front_any.count_ones() as usize + usize::BITS as usize * bitset_top_any.count_ones() as usize].store(bitset_entry_back_any, Ordering::Release);
            self.all_free[entry_all + bitset_entry_front_all.count_ones() as usize + usize::BITS as usize * bitset_top_all.count_ones() as usize].store(bitset_entry_back_all, Ordering::Release);
            self.any_free_top_lookup.store(bitset_top_any, Ordering::Release);
            self.all_free_top_lookup.store(bitset_top_all, Ordering::Release);
        } else {
            self.any_free[entry_any].fetch_or(bitset_entry_front_any, Ordering::AcqRel);
            self.all_free[entry_all].fetch_or(bitset_entry_front_all, Ordering::AcqRel);
            self.any_free[entry_any + bitset_entry_front_any.count_ones() as usize + usize::BITS as usize * bitset_top_any.count_ones() as usize].fetch_or(bitset_entry_back_any, Ordering::AcqRel);
            self.all_free[entry_all + bitset_entry_front_all.count_ones() as usize + usize::BITS as usize * bitset_top_all.count_ones() as usize].fetch_or(bitset_entry_back_all, Ordering::AcqRel);
            self.any_free_top_lookup.fetch_or(bitset_top_any, Ordering::AcqRel);
            self.all_free_top_lookup.fetch_or(bitset_top_all, Ordering::AcqRel);
        }
        if remaining_back != 0 {
            let sub_ptr = unsafe { base.byte_add(entry_cnt_base_all * multiplier) };
            let addr = LAYER_START_ADDRS[self.info.id() + 1].get().0;
            if self.info.id() < 2 {
                let lower = unsafe { &*addr.cast::<Layer> };
                lower.free_at::<FROM_LEFT, CLEAR_OTHER>(sub_ptr, 0, remaining);
            } else {
                let final = unsafe { &*addr.cast::<FinalLayer>() };
                // FIXME: free
            }
        }
    }

    #[inline]
    fn is_final(&self) -> bool {
        false
    }

    #[inline]
    fn get_multiplier(&self) -> usize {
        LAYER_MULTIPLIERS[self.info.id()]
    }
}

fn free_layer<const FROM_LEFT: bool, const CLEAR_OTHER: bool, const FINAL_LAYER: bool>(layer: *mut (), base: *mut (), offset: usize, pages: usize) {
   
}

const LAYER_MULTIPLIERS: [usize; 3] = [
    ENTRIES_PER_INDIRECTION * ENTRIES_PER_INDIRECTION * ENTRIES_PER_INDIRECTION,
    ENTRIES_PER_INDIRECTION * ENTRIES_PER_INDIRECTION,
    ENTRIES_PER_INDIRECTION
];

const ENTRIES_PER_INDIRECTION: usize = usize::BITS as usize * usize::BITS as usize;
const METADATA_WORDS: usize = ENTRIES_PER_INDIRECTION / usize::BITS as usize;


