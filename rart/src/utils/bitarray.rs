use std::mem::MaybeUninit;
use std::ops::Index;

use crate::utils::bitset::BitsetTrait;

// BITSET_WIDTH must be RANGE_WIDTH / 16
// Once generic_const_exprs is stabilized, we can use that to calculate this from a RANGE_WIDTH.
// Until then, don't mess up.
pub struct BitArray<X, const RANGE_WIDTH: usize, BitsetType>
where
    BitsetType: BitsetTrait + std::default::Default,
{
    pub(crate) bitset: BitsetType,
    storage: Box<[MaybeUninit<X>; RANGE_WIDTH]>,
}

impl<X, const RANGE_WIDTH: usize, BitsetType> BitArray<X, RANGE_WIDTH, BitsetType>
where
    BitsetType: BitsetTrait + std::default::Default,
{
    pub fn new() -> Self {
        Self {
            bitset: Default::default(),
            storage: Box::new(unsafe { MaybeUninit::uninit().assume_init() }),
        }
    }

    pub fn push(&mut self, x: X) -> Option<usize> {
        let pos = self.bitset.first_empty()?;
        assert!(pos < RANGE_WIDTH);
        self.bitset.set(pos);
        unsafe {
            self.storage[pos].as_mut_ptr().write(x);
        }
        Some(pos)
    }

    pub fn pop(&mut self) -> Option<X> {
        let pos = self.bitset.last()?;
        self.bitset.unset(pos);
        let old = std::mem::replace(&mut self.storage[pos], MaybeUninit::uninit());
        Some(unsafe { old.assume_init() })
    }

    pub fn last(&self) -> Option<&X> {
        self.bitset
            .last()
            .map(|pos| unsafe { self.storage[pos].assume_init_ref() })
    }

    #[inline]
    pub fn last_used_pos(&self) -> Option<usize> {
        self.bitset.last()
    }

    #[inline]
    pub fn first_used(&self) -> Option<usize> {
        self.bitset.first_set()
    }

    #[inline]
    pub fn first_empty(&mut self) -> Option<usize> {
        // Storage size of the bitset can be larger than the range width.
        // For example: we have a RANGE_WIDTH of 48 and a bitset of 64x1 or 32x2.
        // So we need to check that the first empty bit is within the range width, or people could
        // get the idea they could append beyond our permitted range.
        let Some(first_empty) = self.bitset.first_empty() else {
            return None;
        };
        if first_empty > RANGE_WIDTH {
            return None;
        }
        Some(first_empty)
    }

    #[inline]
    pub fn check(&self, pos: usize) -> bool {
        self.bitset.check(pos)
    }

    #[inline]
    pub fn get(&self, pos: usize) -> Option<&X> {
        assert!(pos < RANGE_WIDTH);
        if self.bitset.check(pos) {
            Some(unsafe { self.storage[pos].assume_init_ref() })
        } else {
            None
        }
    }

    #[inline]
    pub fn get_mut(&mut self, pos: usize) -> Option<&mut X> {
        assert!(pos < RANGE_WIDTH);
        if self.bitset.check(pos) {
            Some(unsafe { self.storage[pos].assume_init_mut() })
        } else {
            None
        }
    }

    #[inline]
    pub fn set(&mut self, pos: usize, x: X) {
        assert!(pos < RANGE_WIDTH);
        unsafe {
            self.storage[pos].as_mut_ptr().write(x);
        };
        self.bitset.set(pos);
    }

    #[inline]
    pub fn update(&mut self, pos: usize, x: X) -> Option<X> {
        let old = self.take_internal(pos);
        unsafe {
            self.storage[pos].as_mut_ptr().write(x);
        };
        self.bitset.set(pos);
        old
    }

    #[inline]
    pub fn erase(&mut self, pos: usize) -> Option<X> {
        let old = self.take_internal(pos)?;
        self.bitset.unset(pos);
        Some(old)
    }

    // Erase without updating index, used by update and erase
    #[inline]
    fn take_internal(&mut self, pos: usize) -> Option<X> {
        assert!(pos < RANGE_WIDTH);
        if self.bitset.check(pos) {
            let old = std::mem::replace(&mut self.storage[pos], MaybeUninit::uninit());
            Some(unsafe { old.assume_init() })
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        for i in 0..RANGE_WIDTH {
            if self.bitset.check(i) {
                unsafe { self.storage[i].assume_init_drop() }
            }
        }
        self.bitset.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.bitset.is_empty()
    }

    pub fn size(&mut self) -> usize {
        self.bitset.size()
    }

    pub fn iter_keys(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        self.storage.iter().enumerate().filter_map(|x| {
            if !self.bitset.check(x.0) {
                None
            } else {
                Some(x.0)
            }
        })
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (usize, &X)> {
        self.storage.iter().enumerate().filter_map(|x| {
            if !self.bitset.check(x.0) {
                None
            } else {
                Some((x.0, unsafe { x.1.assume_init_ref() }))
            }
        })
    }

    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = (usize, &mut X)> {
        self.storage.iter_mut().enumerate().filter_map(|x| {
            if !self.bitset.check(x.0) {
                None
            } else {
                Some((x.0, unsafe { x.1.assume_init_mut() }))
            }
        })
    }
}

impl<X, const RANGE_WIDTH: usize, BitsetType> Default for BitArray<X, RANGE_WIDTH, BitsetType>
where
    BitsetType: BitsetTrait + std::default::Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<X, const RANGE_WIDTH: usize, BitsetType> Index<usize> for BitArray<X, RANGE_WIDTH, BitsetType>
where
    BitsetType: BitsetTrait + std::default::Default,
{
    type Output = X;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).unwrap()
    }
}

impl<X, const RANGE_WIDTH: usize, BitsetType> Drop for BitArray<X, RANGE_WIDTH, BitsetType>
where
    BitsetType: BitsetTrait + std::default::Default,
{
    fn drop(&mut self) {
        for i in 0..RANGE_WIDTH {
            if self.bitset.check(i) {
                unsafe { self.storage[i].assume_init_drop() }
            }
        }
        self.bitset.clear();
    }
}

#[cfg(test)]
mod test {
    use crate::utils::bitarray::BitArray;
    use crate::utils::bitset::Bitset16;

    #[test]
    fn u8_vector() {
        let mut vec: BitArray<u8, 48, Bitset16<3>> = BitArray::new();
        assert_eq!(vec.first_empty(), Some(0));
        assert_eq!(vec.last_used_pos(), None);
        assert_eq!(vec.push(123).unwrap(), 0);
        assert_eq!(vec.first_empty(), Some(1));
        assert_eq!(vec.last_used_pos(), Some(0));
        assert_eq!(vec.get(0), Some(&123));
        assert_eq!(vec.push(124).unwrap(), 1);
        assert_eq!(vec.push(55).unwrap(), 2);
        assert_eq!(vec.push(126).unwrap(), 3);
        assert_eq!(vec.pop(), Some(126));
        assert_eq!(vec.first_empty(), Some(3));
        vec.erase(0);
        assert_eq!(vec.first_empty(), Some(0));
        assert_eq!(vec.last_used_pos(), Some(2));
        assert_eq!(vec.size(), 2);
        vec.set(0, 126);
        assert_eq!(vec.get(0), Some(&126));
        assert_eq!(vec.update(0, 123), Some(126));
    }
}
