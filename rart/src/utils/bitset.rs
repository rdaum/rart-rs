use std::cmp::min;
use std::ops::Index;

use num_traits::PrimInt;

pub trait BitsetTrait: Default {
    fn first_empty(&self) -> Option<usize>;
    fn set(&mut self, pos: usize);
    fn unset(&mut self, pos: usize);
    fn check(&self, pos: usize) -> bool;
    fn clear(&mut self);
    fn last(&self) -> Option<usize>;
    fn is_empty(&self) -> bool;
    fn size(&self) -> usize;
    fn bit_width(&self) -> usize;
    fn capacity(&self) -> usize;
    fn storage_width(&self) -> usize;
    fn as_bitmask(&self) -> u128;
}

// TODO: The bulk of these parameters can be deleted and automatically derived when
// generic_const_exprs lands in stable.
pub struct Bitset<
    StorageType,
    const BIT_WIDTH: usize,
    const SHIFT: usize,
    const STORAGE_WIDTH: usize,
> where
    StorageType: PrimInt,
{
    bitset: [StorageType; STORAGE_WIDTH],
}

impl<StorageType, const BIT_WIDTH: usize, const SHIFT: usize, const STORAGE_WIDTH: usize>
    Bitset<StorageType, BIT_WIDTH, SHIFT, STORAGE_WIDTH>
where
    StorageType: PrimInt,
{
    pub fn new() -> Self {
        Self {
            bitset: [StorageType::min_value(); STORAGE_WIDTH],
        }
    }

    pub fn first_empty(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if b.is_zero() {
                return Some(i << SHIFT);
            }
            if *b != StorageType::max_value() {
                return Some((i << SHIFT) + b.trailing_ones() as usize);
            }
        }
        None
    }

    #[inline]
    pub fn set(&mut self, pos: usize) {
        assert!(pos < STORAGE_WIDTH * BIT_WIDTH);
        let v = self.bitset[pos >> SHIFT];
        let shift: StorageType = StorageType::one() << (pos % BIT_WIDTH);
        let v = v.bitor(shift);
        self.bitset[pos >> SHIFT] = v;
    }

    #[inline]
    pub fn unset(&mut self, pos: usize) {
        assert!(pos < STORAGE_WIDTH * BIT_WIDTH);
        let v = self.bitset[pos >> SHIFT];
        let shift = StorageType::one() << (pos % BIT_WIDTH);
        let v = v & shift.not();
        self.bitset[pos >> SHIFT] = v;
    }

    #[inline]
    pub fn check(&self, pos: usize) -> bool {
        assert!(pos < STORAGE_WIDTH * BIT_WIDTH);
        let shift: StorageType = StorageType::one() << (pos % BIT_WIDTH);
        !(self.bitset[pos >> SHIFT] & shift).is_zero()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.bitset.fill(StorageType::zero());
    }

    pub fn last(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if !b.is_zero() {
                return Some((i << SHIFT) + (BIT_WIDTH - 1) - b.leading_zeros() as usize);
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.bitset.iter().all(|x| x.is_zero())
    }

    pub fn size(&self) -> usize {
        self.bitset.iter().map(|x| x.count_ones() as usize).sum()
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.bitset.iter().enumerate().flat_map(|(i, b)| {
            (0..BIT_WIDTH).filter_map(move |j| {
                let b: u64 = b.to_u64().unwrap();
                if (b) & (1 << j) != 0 {
                    Some((i << SHIFT) + j)
                } else {
                    None
                }
            })
        })
    }
}

impl<StorageType, const BIT_WIDTH: usize, const SHIFT: usize, const STORAGE_WIDTH: usize>
    BitsetTrait for Bitset<StorageType, BIT_WIDTH, SHIFT, STORAGE_WIDTH>
where
    StorageType: PrimInt,
{
    fn first_empty(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if b.is_zero() {
                return Some(i << SHIFT);
            }
            if *b != StorageType::max_value() {
                return Some((i << SHIFT) + b.trailing_ones() as usize);
            }
        }
        None
    }

    #[inline]
    fn set(&mut self, pos: usize) {
        assert!(pos < STORAGE_WIDTH * BIT_WIDTH);
        let v = self.bitset[pos >> SHIFT];
        let shift: StorageType = StorageType::one() << (pos % BIT_WIDTH);
        let v = v.bitor(shift);
        self.bitset[pos >> SHIFT] = v;
    }

    #[inline]
    fn unset(&mut self, pos: usize) {
        assert!(pos < STORAGE_WIDTH * BIT_WIDTH);
        let v = self.bitset[pos >> SHIFT];
        let shift = StorageType::one() << (pos % BIT_WIDTH);
        let v = v & shift.not();
        self.bitset[pos >> SHIFT] = v;
    }

    #[inline]
    fn check(&self, pos: usize) -> bool {
        assert!(pos < STORAGE_WIDTH * BIT_WIDTH);
        let shift: StorageType = StorageType::one() << (pos % BIT_WIDTH);
        !(self.bitset[pos >> SHIFT] & shift).is_zero()
    }

    #[inline]
    fn clear(&mut self) {
        self.bitset.fill(StorageType::zero());
    }

    fn last(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if !b.is_zero() {
                return Some((i << SHIFT) + (BIT_WIDTH - 1) - b.leading_zeros() as usize);
            }
        }
        None
    }

    fn is_empty(&self) -> bool {
        self.bitset.iter().all(|x| x.is_zero())
    }

    fn size(&self) -> usize {
        self.bitset.iter().map(|x| x.count_ones() as usize).sum()
    }

    fn bit_width(&self) -> usize {
        BIT_WIDTH
    }

    fn capacity(&self) -> usize {
        self.bitset.len() * BIT_WIDTH
    }

    fn storage_width(&self) -> usize {
        self.bitset.len()
    }

    fn as_bitmask(&self) -> u128 {
        assert!(BIT_WIDTH <= 128);
        let mut mask = 0u128;
        // copy bit-level representation, unsafe ptr copy
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.bitset.as_ptr() as *const u8,
                &mut mask as *mut u128 as *mut u8,
                min(128, STORAGE_WIDTH * BIT_WIDTH),
            );
        }
        mask
    }
}

impl<StorageType, const BIT_WIDTH: usize, const SHIFT: usize, const STORAGE_WIDTH: usize> Default
    for Bitset<StorageType, BIT_WIDTH, SHIFT, STORAGE_WIDTH>
where
    StorageType: PrimInt,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<StorageType, const BIT_WIDTH: usize, const SHIFT: usize, const STORAGE_WIDTH: usize>
    Index<usize> for Bitset<StorageType, BIT_WIDTH, SHIFT, STORAGE_WIDTH>
where
    StorageType: PrimInt,
{
    type Output = bool;

    #[inline]
    fn index(&self, pos: usize) -> &Self::Output {
        if self.check(pos) {
            &true
        } else {
            &false
        }
    }
}

pub type Bitset64<const STORAGE_WIDTH_U64: usize> = Bitset<u64, 64, 6, STORAGE_WIDTH_U64>;
pub type Bitset32<const STORAGE_WIDTH_U32: usize> = Bitset<u32, 32, 5, STORAGE_WIDTH_U32>;
pub type Bitset16<const STORAGE_WIDTH_U16: usize> = Bitset<u16, 16, 4, STORAGE_WIDTH_U16>;
pub type Bitset8<const STORAGE_WIDTH_U8: usize> = Bitset<u8, 8, 3, STORAGE_WIDTH_U8>;

#[cfg(test)]
mod tests {
    use crate::utils::bitset::BitsetTrait;

    #[test]
    fn test_first_free_8s() {
        let mut bs = super::Bitset8::<4>::new();
        bs.set(1);
        bs.set(3);
        assert_eq!(bs.first_empty(), Some(0));
        bs.set(0);
        assert_eq!(bs.first_empty(), Some(2));

        // Now fill it up and verify none.
        for i in 0..bs.capacity() {
            bs.set(i);
        }
        assert_eq!(bs.first_empty(), None);
    }

    #[test]
    fn test_first_free_32s() {
        let mut bs = super::Bitset32::<1>::new();
        bs.set(1);
        bs.set(3);
        assert_eq!(bs.first_empty(), Some(0));
        bs.set(0);
        assert_eq!(bs.first_empty(), Some(2));

        for i in 0..bs.capacity() {
            bs.set(i);
        }
        assert_eq!(bs.first_empty(), None);
    }

    #[test]
    fn test_iter_16s() {
        let mut bs = super::Bitset16::<4>::new();
        bs.set(0);
        bs.set(1);
        bs.set(2);
        bs.set(4);
        bs.set(8);
        bs.set(16);
        let v: Vec<usize> = bs.iter().collect();
        assert_eq!(v, vec![0, 1, 2, 4, 8, 16]);
    }

    #[test]
    fn test_first_free_64s() {
        let mut bs = super::Bitset64::<4>::new();
        bs.set(1);
        bs.set(3);
        assert_eq!(bs.first_empty(), Some(0));
        bs.set(0);
        assert_eq!(bs.first_empty(), Some(2));
    }

    #[test]
    fn test_iter_64s() {
        let mut bs = super::Bitset64::<4>::new();
        bs.set(0);
        bs.set(1);
        bs.set(2);
        bs.set(4);
        bs.set(8);
        bs.set(16);
        bs.set(32);
        bs.set(47);
        bs.set(48);
        bs.set(49);
        bs.set(127);
        let v: Vec<usize> = bs.iter().collect();
        assert_eq!(v, vec![0, 1, 2, 4, 8, 16, 32, 47, 48, 49, 127]);
    }
}
