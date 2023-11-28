pub(crate) const fn build_bit_mask(offset: usize, ones_cnt: usize) -> usize {
    // FIXME: what if ones_cnt = 0?
    ((1 << ones_cnt) - 1 | ((1 - greater_zero_ret_zero(ones_cnt)) << ones_cnt)) << offset
}

/// a short, branchless algorithm that is eqivalent to
/// if num > 0:
///    ret 1
/// else:
///    ret 0
#[inline]
pub(crate) const fn greater_zero_ret_one(num: usize) -> usize {
    const MSB_OFF: usize = (usize::BITS - 1) as usize;

    // if num is 0, identity will have a value of 0 as all bits are 0, for other values, this will overflow.
    let identity = 0_usize - num;
    identity >> MSB_OFF
}

/// a short, branchless algorithm that is eqivalent to
/// if num > 0:
///    ret 0
/// else:
///    ret 1
#[inline]
pub(crate) const fn greater_zero_ret_zero(num: usize) -> usize {
    1 - greater_zero_ret_one(num)
}

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SyncPtr<T>(pub *const T);

unsafe impl<T> Send for SyncPtr<T> {}
unsafe impl<T> Sync for SyncPtr<T> {}

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SyncPtrMut<T>(pub *mut T);

unsafe impl<T> Send for SyncPtrMut<T> {}
unsafe impl<T> Sync for SyncPtrMut<T> {}

mod test {
    use crate::util::build_bit_mask;


    const fn simple_build_bit_mask(offset: usize, ones_cnt: usize) -> usize {
        let mut mask = 0;
        let mut bit = 0;
        while bit < ones_cnt {
            mask |= 1 << bit;
            bit += 1;
        }
        mask << offset
    }

    #[test]
    fn test_build_bit_mask_valid() {
        for off in 0..(usize::BITS as usize) {
            for cnt in 0..(usize::BITS as usize - off) {
                assert_eq!(simple_build_bit_mask(off, cnt), build_bit_mask(off, cnt));
            }
        }
    }

    #[test]
    fn test_build_bit_mask_overflowing() {
        for off in 0..(usize::BITS as usize) {
            for cnt in 0..(usize::BITS as usize) {
                assert_eq!(simple_build_bit_mask(off, cnt), build_bit_mask(off, cnt));
            }
        }
    }

}
