use std::ops::Index;

pub struct Bitset16<const WIDTH_U16: usize> {
    bitset: [u16; WIDTH_U16],
}

impl<const WIDTH_U16: usize> Bitset16<WIDTH_U16> {
    pub fn new() -> Self {
        Self {
            bitset: [0; WIDTH_U16],
        }
    }

    pub fn first_empty(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if *b == 0 {
                return Some(i << 4);
            }
            if *b != u16::MAX {
                return Some((i << 4) + b.trailing_ones() as usize);
            }
        }
        None
    }

    #[inline]
    pub fn set(&mut self, pos: usize) {
        assert!(pos < WIDTH_U16 * 16);
        self.bitset[pos >> 4] |= 1 << (pos % 16);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.bitset.fill(0);
    }

    #[inline]
    pub fn unset(&mut self, pos: usize) {
        assert!(pos < WIDTH_U16 * 16);
        self.bitset[pos >> 4] &= !(1 << (pos % 16));
    }

    #[inline]
    pub fn check(&self, pos: usize) -> bool {
        assert!(pos < WIDTH_U16 * 16);
        self.bitset[pos >> 4] & (1 << (pos % 16)) != 0
    }

    pub fn last(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if *b != 0 {
                return Some((i << 4) + 15 - b.leading_zeros() as usize);
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.bitset.iter().all(|x| *x == 0)
    }

    pub fn size(&self) -> usize {
        self.bitset.iter().map(|x| x.count_ones() as usize).sum()
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.bitset.iter().enumerate().flat_map(|(i, b)| {
            (0..16).filter_map(move |j| {
                if b & (1 << j) != 0 {
                    Some((i << 4) + j)
                } else {
                    None
                }
            })
        })
    }
}

impl<const WIDTH_U16: usize> Default for Bitset16<WIDTH_U16> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const WIDTH_U16: usize> Index<usize> for Bitset16<WIDTH_U16> {
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

/// A bitset composed of a fixed number of u64s, for representing (relatively) wide sets. E.g. 256
/// elements can be represented using 4 u64s.
/// WIDTH_U64 is the number of U64 in the bitset. So WIDTH_U64 * 64 is the number of bits.
/// When generic_const_exprs is stabilized, we can use that to calculate this from a RANGE_WIDTH.
pub struct Bitset64<const WIDTH_U64: usize> {
    bitset: [u64; WIDTH_U64],
}

impl<const WIDTH_U64: usize> Bitset64<WIDTH_U64> {
    pub fn new() -> Self {
        Self {
            bitset: [0; WIDTH_U64],
        }
    }

    pub fn first_empty(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if *b == 0 {
                return Some(i << 6);
            }
            if *b != u64::MAX {
                return Some((i << 6) + b.trailing_ones() as usize);
            }
        }
        None
    }

    #[inline]
    pub fn set(&mut self, pos: usize) {
        assert!(pos < WIDTH_U64 * 64);
        self.bitset[pos >> 6] |= 1 << (pos % 64);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.bitset.fill(0);
    }

    #[inline]
    pub fn unset(&mut self, pos: usize) {
        assert!(pos < WIDTH_U64 * 64);
        self.bitset[pos >> 6] &= !(1 << (pos % 64));
    }

    #[inline]
    pub fn check(&self, pos: usize) -> bool {
        assert!(pos < WIDTH_U64 * 64);
        self.bitset[pos >> 6] & (1 << (pos % 64)) != 0
    }

    pub fn last(&self) -> Option<usize> {
        for (i, b) in self.bitset.iter().enumerate() {
            if *b != 0 {
                return Some((i << 6) + 63 - b.leading_zeros() as usize);
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.bitset.iter().all(|x| *x == 0)
    }

    pub fn size(&self) -> usize {
        self.bitset.iter().map(|x| x.count_ones() as usize).sum()
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.bitset.iter().enumerate().flat_map(|(i, b)| {
            (0..64).filter_map(move |j| {
                if b & (1 << j) != 0 {
                    Some((i << 6) + j)
                } else {
                    None
                }
            })
        })
    }
}

impl<const WIDTH_U64: usize> Default for Bitset64<WIDTH_U64> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const WIDTH_U64: usize> Index<usize> for Bitset64<WIDTH_U64> {
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

#[cfg(test)]
mod tests {

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
