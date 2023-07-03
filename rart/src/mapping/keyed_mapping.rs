use crate::mapping::indexed_mapping::IndexedMapping;
use crate::mapping::NodeMapping;
use crate::utils::bitarray::BitArray;
use crate::utils::bitset::BitsetTrait;
use crate::utils::u8_keys::u8_keys_find_key_position;

/// Maps a key to a node, using an unsorted array of keys and a corresponding array of nodes.
/// Presence of a key at a position means there is a node at the same position in children.
/// A bitmask is used to keep track of which keys are empty.
/// The nodes are kept unsorted, so a linear search is used to find the key, but SIMD operations
/// are used to speed up the search on platforms that have it.
/// Likewise, appends are done by inserting at the first empty slot (by scanning bitset)
pub struct KeyedMapping<N, const WIDTH: usize, Bitset>
where
    Bitset: BitsetTrait,
{
    pub(crate) keys: [u8; WIDTH],
    pub(crate) children: BitArray<N, WIDTH, Bitset>,
    pub(crate) num_children: u8,
}

impl<N, const WIDTH: usize, Bitset> Default for KeyedMapping<N, WIDTH, Bitset>
where
    Bitset: BitsetTrait,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<N, const WIDTH: usize, Bitset> KeyedMapping<N, WIDTH, Bitset>
where
    Bitset: BitsetTrait,
{
    #[inline]
    pub fn new() -> Self {
        Self {
            keys: [255; WIDTH],
            children: Default::default(),
            num_children: 0,
        }
    }

    pub(crate) fn from_indexed<const IDX_WIDTH: usize, FromBitset: BitsetTrait>(
        im: &mut IndexedMapping<N, IDX_WIDTH, FromBitset>,
    ) -> Self {
        let mut new_mapping = KeyedMapping::new();
        im.num_children = 0;
        im.move_into(&mut new_mapping);
        new_mapping
    }

    pub fn from_resized_grow<const OLD_WIDTH: usize, OldBitset: BitsetTrait>(
        km: &mut KeyedMapping<N, OLD_WIDTH, OldBitset>,
    ) -> Self {
        assert!(WIDTH > OLD_WIDTH);
        let mut new = KeyedMapping::new();

        // Since we're larger than before, we can just copy over and expand everything, occupied
        // or not.
        for i in 0..OLD_WIDTH {
            new.keys[i] = km.keys[i];
            let stolen = km.children.erase(i);
            if let Some(n) = stolen {
                new.children.set(i, n);
            }
        }
        km.children.clear();
        new.num_children = km.num_children;
        new
    }

    // Return the key and value of the only child, and remove it from the mapping.
    pub fn take_value_for_leaf(&mut self) -> (u8, N) {
        assert!(self.num_children == 1);
        let first_child_pos = self.children.first_used().unwrap();
        let key = self.keys[first_child_pos];
        let value = self.children.erase(first_child_pos).unwrap();
        self.num_children -= 1;
        (key, value)
    }

    pub fn from_resized_shrink<const OLD_WIDTH: usize, OldBitset: BitsetTrait>(
        km: &mut KeyedMapping<N, OLD_WIDTH, OldBitset>,
    ) -> Self {
        assert!(WIDTH < OLD_WIDTH);
        let mut new = KeyedMapping::new();
        let mut cnt = 0;

        // Since we're smaller, we compact empty spots out.
        for i in 0..OLD_WIDTH {
            if km.children.check(i) {
                new.keys[cnt] = km.keys[i];
                let stolen = km.children.erase(i);
                if let Some(n) = stolen {
                    new.children.set(cnt, n);
                }
                cnt += 1;
            }
        }
        km.children.clear();
        new.num_children = km.num_children;
        km.num_children = 0;
        new
    }

    #[inline]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u8, &N)> {
        self.keys
            .iter()
            .enumerate()
            .filter(|p| self.children.check(p.0))
            .map(|p| (*p.1, self.children.get(p.0).unwrap()))
    }
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> NodeMapping<N, WIDTH>
    for KeyedMapping<N, WIDTH, Bitset>
{
    #[inline]
    fn add_child(&mut self, key: u8, node: N) {
        // Find an empty position by looking into the bitset.
        let idx = self.children.first_empty().unwrap();
        assert!(idx < WIDTH);
        self.keys[idx] = key;
        self.children.set(idx, node);
        self.num_children += 1;
    }

    fn update_child(&mut self, key: u8, node: N) {
        *self.seek_child_mut(key).unwrap() = node;
    }

    fn seek_child(&self, key: u8) -> Option<&N> {
        let idx = u8_keys_find_key_position::<WIDTH, _>(key, &self.keys, &self.children.bitset)?;
        self.children.get(idx)
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        let idx = u8_keys_find_key_position::<WIDTH, _>(key, &self.keys, &self.children.bitset)?;
        self.children.get_mut(idx)
    }

    fn delete_child(&mut self, key: u8) -> Option<N> {
        // Find position of the key
        let idx = u8_keys_find_key_position::<WIDTH, _>(key, &self.keys, &self.children.bitset)?;
        let result = self.children.erase(idx);
        if result.is_some() {
            self.keys[idx] = 255;
            self.num_children -= 1;
        }

        // Return what we deleted, if any
        result
    }

    #[inline(always)]
    fn num_children(&self) -> usize {
        self.num_children as usize
    }
}

impl<N, const WIDTH: usize, Bitset: BitsetTrait> Drop for KeyedMapping<N, WIDTH, Bitset> {
    fn drop(&mut self) {
        self.children.clear();
        self.num_children = 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::mapping::keyed_mapping::KeyedMapping;
    use crate::mapping::NodeMapping;
    use crate::utils::bitset::{Bitset16, Bitset8};

    #[test]
    fn test_fits_in_cache_line() {
        assert!(std::mem::size_of::<KeyedMapping<u8, 4, Bitset8<4>>>() <= 64);
        assert!(std::mem::size_of::<KeyedMapping<u8, 16, Bitset16<1>>>() <= 64);
    }

    #[test]
    fn test_add_seek_delete() {
        let mut node = KeyedMapping::<u8, 4, Bitset8<4>>::new();
        node.add_child(1, 1);
        node.add_child(2, 2);
        node.add_child(3, 3);
        node.add_child(4, 4);
        assert_eq!(node.num_children(), 4);
        assert_eq!(node.seek_child(1), Some(&1));
        assert_eq!(node.seek_child(2), Some(&2));
        assert_eq!(node.seek_child(3), Some(&3));
        assert_eq!(node.seek_child(4), Some(&4));
        assert_eq!(node.seek_child(5), None);
        assert_eq!(node.seek_child_mut(1), Some(&mut 1));
        assert_eq!(node.seek_child_mut(2), Some(&mut 2));
        assert_eq!(node.seek_child_mut(3), Some(&mut 3));
        assert_eq!(node.seek_child_mut(4), Some(&mut 4));
        assert_eq!(node.seek_child_mut(5), None);
        assert_eq!(node.delete_child(1), Some(1));
        assert_eq!(node.delete_child(2), Some(2));
        assert_eq!(node.delete_child(3), Some(3));
        assert_eq!(node.delete_child(4), Some(4));
        assert_eq!(node.delete_child(5), None);
        assert_eq!(node.num_children(), 0);
    }

    #[test]
    fn test_ff_regression() {
        // Test for scenario where children with '255' keys disappeared.
        let mut node = KeyedMapping::<u8, 4, Bitset8<4>>::new();
        node.add_child(1, 1);
        node.add_child(2, 255);
        node.add_child(3, 3);
        node.delete_child(3);
    }
}
