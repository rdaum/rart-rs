use std::ops::Index;

// WIDTH_U64 is the number of U64 in the bitset. So WIDTH_U64 * 64 is the number of bits.
// When generic_const_exprs is stabilized, we can use that to calculate this from a RANGE_WIDTH.
pub struct Bitset<const WIDTH_U64: usize> {
    bitset: [u64; WIDTH_U64],
}

impl<const WIDTH_U64: usize> Bitset<WIDTH_U64> {
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
        for b in self.bitset.iter_mut() {
            *b = 0;
        }
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

impl<const WIDTH_U64: usize> Default for Bitset<WIDTH_U64> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const WIDTH_U64: usize> Index<usize> for Bitset<WIDTH_U64> {
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
    fn test_iter() {
        let mut bs = super::Bitset::<4>::new();
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
