//! Physical and virtual addresses manipulation

#[cfg(feature = "step_trait")]
use core::convert::TryFrom;
use core::fmt;
#[cfg(feature = "step_trait")]
use core::iter::Step;
use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::structures::paging::page_table::PageTableLevel;
use crate::structures::paging::{PageOffset, PageTableIndex};
use bit_field::BitField;
use crate::mem::page_table::{PageOffset, PageTableIndex, PageTableLevel};

#[cfg(feature = "step_trait")]
const ADDRESS_SPACE_SIZE: u64 = 0x1_0000_0000_0000;

/// A 64-bit virtual memory address.
///
/// This is a wrapper type around an `u64`, so it is always 8 bytes, even when compiled
/// on non 64-bit systems. The
/// [`TryFrom`](https://doc.rust-lang.org/std/convert/trait.TryFrom.html) trait can be used for performing conversions
/// between `u64` and `usize`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct VirtAddr(u64);

/// A 64-bit physical memory address.
///
/// This is a wrapper type around an `u64`, so it is always 8 bytes, even when compiled
/// on non 64-bit systems. The
/// [`TryFrom`](https://doc.rust-lang.org/std/convert/trait.TryFrom.html) trait can be used for performing conversions
/// between `u64` and `usize`.
///
/// On `x86_64`, only the 52 lower bits of a physical address can be used. The top 12 bits need
/// to be zero. This type guarantees that it always represents a valid physical address.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PhysAddr(u64);

/// A passed `u64` was not a valid virtual address.
///
/// This means that bits 48 to 64 are not
/// a valid sign extension and are not null either. So automatic sign extension would have
/// overwritten possibly meaningful bits. This likely indicates a bug, for example an invalid
/// address calculation.
///
/// Contains the invalid address.
pub struct VirtAddrNotValid(pub u64);

impl core::fmt::Debug for VirtAddrNotValid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VirtAddrNotValid")
            .field(&format_args!("{:#x}", self.0))
            .finish()
    }
}

impl VirtAddr {
    /// Creates a new virtual address.
    #[inline]
    pub fn new(addr: u64) -> VirtAddr {
        Self(addr)
    }

    /// Creates a virtual address that points to `0`.
    #[inline]
    pub const fn zero() -> VirtAddr {
        VirtAddr(0)
    }

    /// Converts the address to an `u64`.
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Creates a virtual address from the given pointer
    // cfg(target_pointer_width = "32") is only here for backwards
    // compatibility: Earlier versions of this crate did not have any `cfg()`
    // on this function. At least for 32- and 64-bit we know the `as u64` cast
    // doesn't truncate.
    #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
    #[inline]
    pub fn from_ptr<T>(ptr: *const T) -> Self {
        Self::new(ptr as u64)
    }

    /// Converts the address to a raw pointer.
    #[cfg(target_pointer_width = "64")]
    #[inline]
    pub const fn as_ptr<T>(self) -> *const T {
        self.as_u64() as *const T
    }

    /// Converts the address to a mutable raw pointer.
    #[cfg(target_pointer_width = "64")]
    #[inline]
    pub const fn as_mut_ptr<T>(self) -> *mut T {
        self.as_ptr::<T>() as *mut T
    }

    /// Convenience method for checking if a virtual address is null.
    #[inline]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    /// Aligns the virtual address upwards to the given alignment.
    ///
    /// See the `align_up` function for more information.
    #[inline]
    pub fn align_up<U>(self, align: U) -> Self
        where
            U: Into<u64>,
    {
        VirtAddr(align_up(self.0, align.into()))
    }

    /// Aligns the virtual address downwards to the given alignment.
    ///
    /// See the `align_down` function for more information.
    #[inline]
    pub fn align_down<U>(self, align: U) -> Self
        where
            U: Into<u64>,
    {
        VirtAddr(align_down(self.0, align.into()))
    }

    /// Checks whether the virtual address has the demanded alignment.
    #[inline]
    pub fn is_aligned<U>(self, align: U) -> bool
        where
            U: Into<u64>,
    {
        self.align_down(align) == self
    }

    /// Returns the 12-bit page offset of this virtual address.
    #[inline]
    pub const fn page_offset(self) -> PageOffset {
        PageOffset::new_truncate(self.0 as u16)
    }

    /// Returns the 9-bit level 1 page table index.
    #[inline]
    pub const fn p1_index(self) -> PageTableIndex {
        PageTableIndex::new_truncate((self.0 >> 12) as u16)
    }

    /// Returns the 9-bit level 2 page table index.
    #[inline]
    pub const fn p2_index(self) -> PageTableIndex {
        PageTableIndex::new_truncate((self.0 >> 12 >> 9) as u16)
    }

    /// Returns the 9-bit level 3 page table index.
    #[inline]
    pub const fn p3_index(self) -> PageTableIndex {
        PageTableIndex::new_truncate((self.0 >> 12 >> 9 >> 9) as u16)
    }

    /// Returns the 9-bit level 4 page table index.
    #[inline]
    pub const fn p4_index(self) -> PageTableIndex {
        PageTableIndex::new_truncate((self.0 >> 12 >> 9 >> 9 >> 9) as u16)
    }

    /// Returns the 9-bit level 5 page table index.
    #[inline]
    pub const fn p5_index(self) -> PageTableIndex {
        PageTableIndex::new_truncate((self.0 >> 12 >> 9 >> 9 >> 9 >> 9) as u16)
    }

    /// Returns the 9-bit level page table index.
    #[inline]
    pub const fn page_table_index(self, level: PageTableLevel) -> PageTableIndex {
        PageTableIndex::new_truncate((self.0 >> 12 >> ((level as u8 - 1) * 9)) as u16)
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("VirtAddr")
            .field(&format_args!("{:#x}", self.0))
            .finish()
    }
}

impl fmt::Binary for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Binary::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::Octal for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Octal::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl fmt::Pointer for VirtAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Pointer::fmt(&(self.0 as *const ()), f)
    }
}

impl Add<u64> for VirtAddr {
    type Output = Self;
    #[inline]
    fn add(self, rhs: u64) -> Self::Output {
        VirtAddr::new(self.0 + rhs)
    }
}

impl AddAssign<u64> for VirtAddr {
    #[inline]
    fn add_assign(&mut self, rhs: u64) {
        *self = *self + rhs;
    }
}

#[cfg(target_pointer_width = "64")]
impl Add<usize> for VirtAddr {
    type Output = Self;
    #[inline]
    fn add(self, rhs: usize) -> Self::Output {
        self + rhs as u64
    }
}

#[cfg(target_pointer_width = "64")]
impl AddAssign<usize> for VirtAddr {
    #[inline]
    fn add_assign(&mut self, rhs: usize) {
        self.add_assign(rhs as u64)
    }
}

impl Sub<u64> for VirtAddr {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: u64) -> Self::Output {
        VirtAddr::new(self.0.checked_sub(rhs).unwrap())
    }
}

impl SubAssign<u64> for VirtAddr {
    #[inline]
    fn sub_assign(&mut self, rhs: u64) {
        *self = *self - rhs;
    }
}

#[cfg(target_pointer_width = "64")]
impl Sub<usize> for VirtAddr {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: usize) -> Self::Output {
        self - rhs as u64
    }
}

#[cfg(target_pointer_width = "64")]
impl SubAssign<usize> for VirtAddr {
    #[inline]
    fn sub_assign(&mut self, rhs: usize) {
        self.sub_assign(rhs as u64)
    }
}

impl Sub<VirtAddr> for VirtAddr {
    type Output = u64;
    #[inline]
    fn sub(self, rhs: VirtAddr) -> Self::Output {
        self.as_u64().checked_sub(rhs.as_u64()).unwrap()
    }
}

#[cfg(feature = "step_trait")]
impl Step for VirtAddr {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        let mut steps = end.0.checked_sub(start.0)?;

        // Check if we jumped the gap.
        if end.0.get_bit(47) && !start.0.get_bit(47) {
            steps = steps.checked_sub(0xffff_0000_0000_0000).unwrap();
        }

        usize::try_from(steps).ok()
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        let offset = u64::try_from(count).ok()?;
        if offset > ADDRESS_SPACE_SIZE {
            return None;
        }

        let mut addr = start.0.checked_add(offset)?;

        match addr.get_bits(47..) {
            0x1 => {
                // Jump the gap by sign extending the 47th bit.
                addr.set_bits(47.., 0x1ffff);
            }
            0x2 => {
                // Address overflow
                return None;
            }
            _ => {}
        }

        Some(Self::new(addr))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        let offset = u64::try_from(count).ok()?;
        if offset > ADDRESS_SPACE_SIZE {
            return None;
        }

        let mut addr = start.0.checked_sub(offset)?;

        match addr.get_bits(47..) {
            0x1fffe => {
                // Jump the gap by sign extending the 47th bit.
                addr.set_bits(47.., 0);
            }
            0x1fffd => {
                // Address underflow
                return None;
            }
            _ => {}
        }

        Some(Self::new(addr))
    }
}

/// A passed `u64` was not a valid physical address.
///
/// This means that bits 52 to 64 were not all null.
///
/// Contains the invalid address.
pub struct PhysAddrNotValid(pub u64);

impl core::fmt::Debug for PhysAddrNotValid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PhysAddrNotValid")
            .field(&format_args!("{:#x}", self.0))
            .finish()
    }
}

impl PhysAddr {
    /// Creates a new physical address.
    ///
    /// ## Panics
    ///
    /// This function panics if a bit in the range 52 to 64 is set.
    #[inline]
    pub const fn new(addr: u64) -> Self {
        // TODO: Replace with .ok().expect(msg) when that works on stable.
        match Self::try_new(addr) {
            Ok(p) => p,
            Err(_) => panic!("physical addresses must not have any bits in the range 52 to 64 set"),
        }
    }

    /// Creates a new physical address, throwing bits 52..64 away.
    #[inline]
    pub const fn new_truncate(addr: u64) -> PhysAddr {
        PhysAddr(addr % (1 << 52))
    }

    /// Creates a new physical address, without any checks.
    ///
    /// ## Safety
    ///
    /// You must make sure bits 52..64 are zero. This is not checked.
    #[inline]
    pub const unsafe fn new_unsafe(addr: u64) -> PhysAddr {
        PhysAddr(addr)
    }

    /// Tries to create a new physical address.
    ///
    /// Fails if any bits in the range 52 to 64 are set.
    #[inline]
    pub const fn try_new(addr: u64) -> Result<Self, PhysAddrNotValid> {
        let p = Self::new_truncate(addr);
        if p.0 == addr {
            Ok(p)
        } else {
            Err(PhysAddrNotValid(addr))
        }
    }

    /// Creates a physical address that points to `0`.
    #[inline]
    pub const fn zero() -> PhysAddr {
        PhysAddr(0)
    }

    /// Converts the address to an `u64`.
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Convenience method for checking if a physical address is null.
    #[inline]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    /// Aligns the physical address upwards to the given alignment.
    ///
    /// See the `align_up` function for more information.
    #[inline]
    pub fn align_up<U>(self, align: U) -> Self
        where
            U: Into<u64>,
    {
        PhysAddr(align_up(self.0, align.into()))
    }

    /// Aligns the physical address downwards to the given alignment.
    ///
    /// See the `align_down` function for more information.
    #[inline]
    pub fn align_down<U>(self, align: U) -> Self
        where
            U: Into<u64>,
    {
        PhysAddr(align_down(self.0, align.into()))
    }

    /// Checks whether the physical address has the demanded alignment.
    #[inline]
    pub fn is_aligned<U>(self, align: U) -> bool
        where
            U: Into<u64>,
    {
        self.align_down(align) == self
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("PhysAddr")
            .field(&format_args!("{:#x}", self.0))
            .finish()
    }
}

impl fmt::Binary for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Binary::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl fmt::Octal for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Octal::fmt(&self.0, f)
    }
}

impl fmt::UpperHex for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl fmt::Pointer for PhysAddr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Pointer::fmt(&(self.0 as *const ()), f)
    }
}

impl Add<u64> for PhysAddr {
    type Output = Self;
    #[inline]
    fn add(self, rhs: u64) -> Self::Output {
        PhysAddr::new(self.0 + rhs)
    }
}

impl AddAssign<u64> for PhysAddr {
    #[inline]
    fn add_assign(&mut self, rhs: u64) {
        *self = *self + rhs;
    }
}

#[cfg(target_pointer_width = "64")]
impl Add<usize> for PhysAddr {
    type Output = Self;
    #[inline]
    fn add(self, rhs: usize) -> Self::Output {
        self + rhs as u64
    }
}

#[cfg(target_pointer_width = "64")]
impl AddAssign<usize> for PhysAddr {
    #[inline]
    fn add_assign(&mut self, rhs: usize) {
        self.add_assign(rhs as u64)
    }
}

impl Sub<u64> for PhysAddr {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: u64) -> Self::Output {
        PhysAddr::new(self.0.checked_sub(rhs).unwrap())
    }
}

impl SubAssign<u64> for PhysAddr {
    #[inline]
    fn sub_assign(&mut self, rhs: u64) {
        *self = *self - rhs;
    }
}

#[cfg(target_pointer_width = "64")]
impl Sub<usize> for PhysAddr {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: usize) -> Self::Output {
        self - rhs as u64
    }
}

#[cfg(target_pointer_width = "64")]
impl SubAssign<usize> for PhysAddr {
    #[inline]
    fn sub_assign(&mut self, rhs: usize) {
        self.sub_assign(rhs as u64)
    }
}

impl Sub<PhysAddr> for PhysAddr {
    type Output = u64;
    #[inline]
    fn sub(self, rhs: PhysAddr) -> Self::Output {
        self.as_u64().checked_sub(rhs.as_u64()).unwrap()
    }
}

/// Align address downwards.
///
/// Returns the greatest `x` with alignment `align` so that `x <= addr`.
///
/// Panics if the alignment is not a power of two.
#[inline]
pub const fn align_down(addr: u64, align: u64) -> u64 {
    assert!(align.is_power_of_two(), "`align` must be a power of two");
    addr & !(align - 1)
}

/// Align address upwards.
///
/// Returns the smallest `x` with alignment `align` so that `x >= addr`.
///
/// Panics if the alignment is not a power of two.
#[inline]
pub const fn align_up(addr: u64, align: u64) -> u64 {
    assert!(align.is_power_of_two(), "`align` must be a power of two");
    let align_mask = align - 1;
    if addr & align_mask == 0 {
        addr // already aligned
    } else {
        (addr | align_mask) + 1
    }
}
