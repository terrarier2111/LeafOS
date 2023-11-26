pub(crate) const fn build_bit_mask(offset: usize, ones_cnt: usize) -> usize {
    let mut mask = 0;
    let mut bit = 0;
    while bit < ones_cnt {
        mask |= 1 << bit;
        bit += 1;
    }
    mask << offset
}