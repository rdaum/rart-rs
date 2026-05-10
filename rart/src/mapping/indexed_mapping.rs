use std::mem::MaybeUninit;

use crate::mapping::NodeMapping;
use crate::mapping::direct_mapping::DirectMapping;
use crate::mapping::keyed_mapping::KeyedMapping;
use crate::mapping::sorted_keyed_mapping::SortedKeyedMapping;
use crate::utils::bitarray::BitArray;
use crate::utils::bitset::{Bitset64, BitsetOnesIter, BitsetTrait};

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
            key_iter: self.child_ptr_indexes.bitset.iter(),
            mapping: self,
        }
    }
}

pub struct IndexedMappingIntoIter<N, const WIDTH: usize, Bitset: BitsetTrait> {
    key_iter: BitsetOnesIter<u64, 4>,
    mapping: IndexedMapping<N, WIDTH, Bitset>,
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> Iterator
    for IndexedMappingIntoIter<N, WIDTH, Bitset>
{
    type Item = (u8, N);

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.key_iter.next()? as u8;
        let child = self.mapping.delete_child(key)?;
        Some((key, child))
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
        let child_count = dm.num_children();
        let mut next_slot = 0usize;

        while let Some(key) = dm.children.first_used() {
            // SAFETY: `first_used()` only returns initialized child slots.
            let child = unsafe { dm.children.erase_known_present(key) };
            indexed.child_ptr_indexes.set(key, next_slot as u8);
            indexed.children.set(next_slot, child);
            next_slot += 1;
        }
        indexed.num_children = child_count as u8;
        indexed
    }

    pub fn from_sorted_keyed<const KM_WIDTH: usize>(
        km: &mut SortedKeyedMapping<N, KM_WIDTH>,
    ) -> Self {
        let mut im: IndexedMapping<N, WIDTH, Bitset> = IndexedMapping::new();
        let child_count = km.num_children as usize;
        for i in 0..child_count {
            let stolen = unsafe { km.children[i].assume_init_read() };
            km.children[i] = MaybeUninit::uninit();
            im.child_ptr_indexes.set(km.keys[i] as usize, i as u8);
            im.children.set(i, stolen);
        }
        im.num_children = km.num_children;
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
            key_iter: self.child_ptr_indexes.bitset.iter(),
            mapping: self,
        }
    }

    #[inline]
    pub(crate) fn add_child_sorted(&mut self, key: u8, node: N) {
        let pos = self.num_children as usize;
        debug_assert!(pos < WIDTH);
        debug_assert!(!self.child_ptr_indexes.check(key as usize));

        self.child_ptr_indexes.set(key as usize, pos as u8);
        self.children.set(pos, node);
        self.num_children += 1;
    }
}

impl<N, const WIDTH: usize, Bitset> IndexedMapping<N, WIDTH, Bitset>
where
    N: Clone,
    Bitset: BitsetTrait + Clone,
{
    pub(crate) fn clone_mapping(&self) -> Self {
        Self {
            child_ptr_indexes: self.child_ptr_indexes.clone_live_slots(),
            children: self.children.clone_live_slots(),
            num_children: self.num_children,
        }
    }
}

pub struct IndexedMappingIter<'a, N, const WIDTH: usize, Bitset: BitsetTrait> {
    key_iter: BitsetOnesIter<u64, 4>,
    mapping: &'a IndexedMapping<N, WIDTH, Bitset>,
}

impl<'a, N, const WIDTH: usize, Bitset: BitsetTrait> Iterator
    for IndexedMappingIter<'a, N, WIDTH, Bitset>
{
    type Item = (u8, &'a N);

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.key_iter.next()? as u8;
        let pos = self.mapping.child_ptr_indexes.get(key as usize)?;
        Some((key, &self.mapping.children[*pos as usize]))
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
            // SAFETY: a live entry in the index map points at an initialized child slot.
            return Some(unsafe { self.children.get_known_present(*pos as usize) });
        }
        None
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        if let Some(pos) = self.child_ptr_indexes.get(key as usize) {
            // SAFETY: a live entry in the index map points at an initialized child slot.
            return Some(unsafe { self.children.get_known_present_mut(*pos as usize) });
        }
        None
    }

    fn delete_child(&mut self, key: u8) -> Option<N> {
        let pos = self.child_ptr_indexes.erase(key as usize)?;

        // SAFETY: a live entry in the index map points at an initialized child slot.
        let old = unsafe { self.children.erase_known_present(pos as usize) };
        self.num_children -= 1;

        // Return what we deleted.
        Some(old)
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

    #[test]
    fn iter_preserves_key_order_for_sparse_children() {
        let mut mapping = super::IndexedMapping::<u8, 48, Bitset16<3>>::new();
        for key in [200u8, 3, 47, 17, 129] {
            mapping.add_child(key, key);
        }

        let keys: Vec<u8> = mapping.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![3, 17, 47, 129, 200]);
    }

    #[test]
    fn from_sorted_keyed_preserves_children() {
        let mut source = crate::mapping::sorted_keyed_mapping::SortedKeyedMapping::<u8, 16>::new();
        for key in [17u8, 3, 47, 9, 31] {
            source.add_child(key, key);
        }

        let mapping = super::IndexedMapping::<u8, 48, Bitset16<3>>::from_sorted_keyed(&mut source);
        assert_eq!(source.num_children(), 0);

        let keys: Vec<u8> = mapping.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![3, 9, 17, 31, 47]);

        for key in [3u8, 9, 17, 31, 47] {
            assert_eq!(mapping.seek_child(key), Some(&key));
        }
    }

    #[test]
    fn clone_mapping_preserves_sparse_slots() {
        let mut mapping = super::IndexedMapping::<usize, 48, Bitset16<3>>::new();
        for key in [200u8, 3, 47, 17, 129] {
            mapping.add_child(key, key as usize);
        }

        mapping.delete_child(47);
        mapping.add_child(99, 99);

        let mut cloned = mapping.clone_mapping();
        assert_eq!(cloned.num_children(), mapping.num_children());

        for key in [3u8, 17, 99, 129, 200] {
            assert_eq!(cloned.seek_child(key), Some(&(key as usize)));
        }
        assert_eq!(cloned.seek_child(47), None);

        assert_eq!(cloned.delete_child(99), Some(99));
        assert_eq!(cloned.seek_child(99), None);
        assert_eq!(mapping.seek_child(99), Some(&99));
    }
}
