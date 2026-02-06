use std::mem::MaybeUninit;

use crate::mapping::NodeMapping;
use crate::mapping::direct_mapping::DirectMapping;
use crate::mapping::keyed_mapping::KeyedMapping;
use crate::mapping::sorted_keyed_mapping::SortedKeyedMapping;
use crate::utils::bitarray::BitArray;
use crate::utils::bitset::{Bitset64, BitsetTrait};

/// A mapping from keys to separate child pointers. 256 keys, usually 48 children.
pub struct IndexedMapping<N, const WIDTH: usize, Bitset: BitsetTrait> {
    pub(crate) child_ptr_indexes: BitArray<u8, 256, Bitset64<4>>,
    pub(crate) children: BitArray<N, WIDTH, Bitset>,
    pub(crate) num_children: u8,
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> Default for IndexedMapping<N, WIDTH, Bitset> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> IntoIterator for IndexedMapping<N, WIDTH, Bitset> {
    type Item = (u8, N);
    type IntoIter = IndexedMappingIntoIter<N, WIDTH, Bitset>;

    fn into_iter(self) -> Self::IntoIter {
        IndexedMappingIntoIter {
            mapping: self,
            current_key: 0,
        }
    }
}

pub struct IndexedMappingIntoIter<N, const WIDTH: usize, Bitset: BitsetTrait> {
    mapping: IndexedMapping<N, WIDTH, Bitset>,
    current_key: usize,
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> Iterator
    for IndexedMappingIntoIter<N, WIDTH, Bitset>
{
    type Item = (u8, N);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_key < 256 {
            let key = self.current_key as u8;
            self.current_key += 1;

            if let Some(child) = self.mapping.delete_child(key) {
                return Some((key, child));
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.mapping.num_children as usize;
        (remaining, Some(remaining))
    }
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> ExactSizeIterator
    for IndexedMappingIntoIter<N, WIDTH, Bitset>
{
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> IndexedMapping<N, WIDTH, Bitset> {
    pub fn new() -> Self {
        Self {
            child_ptr_indexes: Default::default(),
            children: BitArray::new(),
            num_children: 0,
        }
    }

    pub(crate) fn from_direct(dm: &mut DirectMapping<N>) -> Self {
        let mut indexed = IndexedMapping::new();

        let keys: Vec<usize> = dm.children.iter_keys().collect();
        for key in keys {
            let child = dm.children.erase(key).unwrap();
            indexed.add_child(key as u8, child);
        }
        indexed
    }

    pub fn from_sorted_keyed<const KM_WIDTH: usize>(
        km: &mut SortedKeyedMapping<N, KM_WIDTH>,
    ) -> Self {
        let mut im: IndexedMapping<N, WIDTH, Bitset> = IndexedMapping::new();
        for i in 0..km.num_children as usize {
            let stolen = std::mem::replace(&mut km.children[i], MaybeUninit::uninit());
            im.add_child(km.keys[i], unsafe { stolen.assume_init() });
        }
        km.num_children = 0;
        im
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn from_keyed<const KM_WIDTH: usize, FromBitset: BitsetTrait>(
        km: &mut KeyedMapping<N, KM_WIDTH, FromBitset>,
    ) -> Self {
        let mut im: IndexedMapping<N, WIDTH, Bitset> = IndexedMapping::new();
        for i in 0..KM_WIDTH {
            let Some(stolen) = km.children.erase(i) else {
                continue;
            };
            im.add_child(km.keys[i], stolen);
        }
        km.children.clear();
        km.num_children = 0;
        im
    }

    pub(crate) fn move_into<const NEW_WIDTH: usize, NM: NodeMapping<N, NEW_WIDTH>>(
        &mut self,
        nm: &mut NM,
    ) {
        for (key, pos) in self.child_ptr_indexes.iter() {
            let node = self.children.erase(*pos as usize).unwrap();
            nm.add_child(key as u8, node);
        }
    }

    pub fn iter(&self) -> IndexedMappingIter<'_, N, WIDTH, Bitset> {
        IndexedMappingIter {
            mapping: self,
            current_key: 0,
        }
    }
}

pub struct IndexedMappingIter<'a, N, const WIDTH: usize, Bitset: BitsetTrait> {
    mapping: &'a IndexedMapping<N, WIDTH, Bitset>,
    current_key: usize,
}

impl<'a, N, const WIDTH: usize, Bitset: BitsetTrait> Iterator
    for IndexedMappingIter<'a, N, WIDTH, Bitset>
{
    type Item = (u8, &'a N);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_key < 256 {
            let key = self.current_key as u8;
            self.current_key += 1;

            if let Some(pos) = self.mapping.child_ptr_indexes.get(key as usize) {
                return Some((key, &self.mapping.children[*pos as usize]));
            }
        }
        None
    }
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> NodeMapping<N, WIDTH>
    for IndexedMapping<N, WIDTH, Bitset>
{
    fn add_child(&mut self, key: u8, node: N) {
        let pos = self.children.first_empty().unwrap();
        self.child_ptr_indexes.set(key as usize, pos as u8);
        self.children.set(pos, node);
        self.num_children += 1;
    }

    fn seek_child(&self, key: u8) -> Option<&N> {
        if let Some(pos) = self.child_ptr_indexes.get(key as usize) {
            return self.children.get(*pos as usize);
        }
        None
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        if let Some(pos) = self.child_ptr_indexes.get(key as usize) {
            return self.children.get_mut(*pos as usize);
        }
        None
    }

    fn delete_child(&mut self, key: u8) -> Option<N> {
        let pos = self.child_ptr_indexes.erase(key as usize)?;

        let old = self.children.erase(pos as usize);
        self.num_children -= 1;

        // Return what we deleted.
        old
    }

    fn num_children(&self) -> usize {
        self.num_children as usize
    }
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> Drop for IndexedMapping<N, WIDTH, Bitset> {
    fn drop(&mut self) {
        if self.num_children == 0 {
            return;
        }
        self.num_children = 0;
        self.child_ptr_indexes.clear();
        self.children.clear();
    }
}

#[cfg(test)]
mod test {
    use crate::mapping::NodeMapping;
    use crate::utils::bitset::Bitset16;

    #[test]
    fn test_fits_in_cache_line() {
        assert!(std::mem::size_of::<super::IndexedMapping<u8, 48, Bitset16<3>>>() <= 64);
    }

    #[test]
    fn test_basic_mapping() {
        let mut mapping = super::IndexedMapping::<u8, 48, Bitset16<3>>::new();
        for i in 0..48 {
            mapping.add_child(i, i);
            assert_eq!(*mapping.seek_child(i).unwrap(), i);
        }
        for i in 0..48 {
            debug_assert_eq!(*mapping.seek_child(i).unwrap(), i);
        }
        for i in 0..48 {
            debug_assert_eq!(mapping.delete_child(i).unwrap(), i);
        }
        for i in 0..48 {
            debug_assert!(mapping.seek_child(i as u8).is_none());
        }
    }
}
