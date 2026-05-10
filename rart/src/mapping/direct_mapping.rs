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
        let child_count = im.num_children as usize;

        while let Some(key) = im.child_ptr_indexes.first_used() {
            // SAFETY: `first_used()` only returns initialized index entries.
            let pos = unsafe { im.child_ptr_indexes.erase_known_present(key) } as usize;
            // SAFETY: a live index entry points at an initialized child slot.
            let child = unsafe { im.children.erase_known_present(pos) };
            new_mapping.children.set(key, child);
        }

        new_mapping.num_children = child_count;
        im.num_children = 0;
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

impl<N: Clone> DirectMapping<N> {
    pub(crate) fn clone_mapping(&self) -> Self {
        Self {
            children: self.children.clone_live_slots(),
            num_children: self.num_children,
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
        let pos = key as usize;
        if self.children.check(pos) {
            // SAFETY: the presence check above guarantees that `pos` is initialized.
            Some(unsafe { self.children.get_known_present(pos) })
        } else {
            None
        }
    }

    #[inline]
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        let pos = key as usize;
        if self.children.check(pos) {
            // SAFETY: the presence check above guarantees that `pos` is initialized.
            Some(unsafe { self.children.get_known_present_mut(pos) })
        } else {
            None
        }
    }

    #[inline]
    fn delete_child(&mut self, key: u8) -> Option<N> {
        let pos = key as usize;
        if self.children.check(pos) {
            self.num_children -= 1;
            // SAFETY: the presence check above guarantees that `pos` is initialized.
            Some(unsafe { self.children.erase_known_present(pos) })
        } else {
            None
        }
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

    #[test]
    fn from_indexed_preserves_sparse_children() {
        let mut indexed = crate::mapping::indexed_mapping::IndexedMapping::<
            u8,
            48,
            crate::utils::bitset::Bitset16<3>,
        >::new();
        for key in [200u8, 3, 47, 17, 129] {
            indexed.add_child(key, key);
        }

        let dm = super::DirectMapping::from_indexed(&mut indexed);
        assert_eq!(indexed.num_children(), 0);

        let keys: Vec<u8> = dm.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![3, 17, 47, 129, 200]);

        for key in [3u8, 17, 47, 129, 200] {
            assert_eq!(dm.seek_child(key), Some(&key));
        }
    }

    #[test]
    fn clone_mapping_preserves_sparse_slots() {
        let mut dm = super::DirectMapping::new();
        for key in [0u8, 17, 63, 128, 255] {
            dm.add_child(key, key as usize);
        }

        let mut cloned = dm.clone_mapping();
        assert_eq!(cloned.num_children(), dm.num_children());

        for key in [0u8, 17, 63, 128, 255] {
            assert_eq!(cloned.seek_child(key), Some(&(key as usize)));
        }

        assert_eq!(cloned.delete_child(63), Some(63));
        assert_eq!(cloned.seek_child(63), None);
        assert_eq!(dm.seek_child(63), Some(&63));
    }
}
