use crate::mapping::NodeMapping;
use crate::mapping::indexed_mapping::IndexedMapping;
use crate::utils::bitarray::BitArray;
use crate::utils::bitset::{Bitset64, BitsetOnesIter, BitsetTrait};

pub struct DirectMapping<N> {
    pub(crate) children: BitArray<N, 256, Bitset64<4>>,
    num_children: usize,
}

impl<N> Default for DirectMapping<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N> IntoIterator for DirectMapping<N> {
    type Item = (u8, N);
    type IntoIter = DirectMappingIntoIter<N>;

    fn into_iter(self) -> Self::IntoIter {
        DirectMappingIntoIter {
            key_iter: self.children.bitset.iter(),
            mapping: self,
        }
    }
}

pub struct DirectMappingIntoIter<N> {
    key_iter: BitsetOnesIter<u64, 4>,
    mapping: DirectMapping<N>,
}

impl<N> Iterator for DirectMappingIntoIter<N> {
    type Item = (u8, N);

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.key_iter.next()? as u8;
        let child = self.mapping.delete_child(key)?;
        Some((key, child))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.mapping.num_children;
        (remaining, Some(remaining))
    }
}

impl<N> ExactSizeIterator for DirectMappingIntoIter<N> {}

impl<N> DirectMapping<N> {
    pub fn new() -> Self {
        Self {
            children: BitArray::new(),
            num_children: 0,
        }
    }

    pub fn from_indexed<const WIDTH: usize, FromBitset: BitsetTrait>(
        im: &mut IndexedMapping<N, WIDTH, FromBitset>,
    ) -> Self {
        let mut new_mapping = DirectMapping::<N>::new();
        im.num_children = 0;
        im.move_into(&mut new_mapping);
        new_mapping
    }

    #[inline]
    pub fn iter(&self) -> DirectMappingIter<'_, N> {
        DirectMappingIter {
            key_iter: self.children.bitset.iter(),
            mapping: self,
        }
    }
}

pub struct DirectMappingIter<'a, N> {
    key_iter: BitsetOnesIter<u64, 4>,
    mapping: &'a DirectMapping<N>,
}

impl<'a, N> Iterator for DirectMappingIter<'a, N> {
    type Item = (u8, &'a N);

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.key_iter.next()? as u8;
        let child = self.mapping.children.get(key as usize)?;
        Some((key, child))
    }
}

impl<N> NodeMapping<N, 256> for DirectMapping<N> {
    #[inline]
    fn add_child(&mut self, key: u8, node: N) {
        self.children.set(key as usize, node);
        self.num_children += 1;
    }

    #[inline]
    fn seek_child(&self, key: u8) -> Option<&N> {
        self.children.get(key as usize)
    }

    #[inline]
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        self.children.get_mut(key as usize)
    }

    #[inline]
    fn delete_child(&mut self, key: u8) -> Option<N> {
        let n = self.children.erase(key as usize);
        if n.is_some() {
            self.num_children -= 1;
        }
        n
    }

    #[inline]
    fn num_children(&self) -> usize {
        self.num_children
    }
}

#[cfg(test)]
mod tests {
    use crate::mapping::NodeMapping;

    #[test]
    fn direct_mapping_test() {
        let mut dm = super::DirectMapping::new();
        for i in 0..255 {
            dm.add_child(i, i);
            assert_eq!(*dm.seek_child(i).unwrap(), i);
            assert_eq!(dm.delete_child(i), Some(i));
            assert_eq!(dm.seek_child(i), None);
        }
    }

    #[test]
    fn iter_preserves_key_order_for_sparse_children() {
        let mut dm = super::DirectMapping::new();
        for key in [200u8, 3, 250, 17, 128] {
            dm.add_child(key, key);
        }

        let keys: Vec<u8> = dm.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![3, 17, 128, 200, 250]);
    }
}
