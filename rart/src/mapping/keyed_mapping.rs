use std::mem::MaybeUninit;

use crate::mapping::indexed_mapping::IndexedMapping;
use crate::mapping::sorted_keyed_mapping::SortedKeyedMapping;
use crate::node::NodeMapping;
use crate::utils::bitset::Bitset16;
use crate::utils::u8_keys::u8_keys_find_key_position;

/// Maps a key to a node, using an unsorted array of keys and a corresponding array of nodes.
/// Presence of a key at a position means there is a node at the same position in children.
/// A bitmask is used to keep track of which keys are empty.
/// The nodes are kept unsorted, so a linear search is used to find the key, but SIMD operations
/// are used to speed up the search on platforms that have it.
/// Likewise, appends are done by inserting at the first empty slot (by scanning bitset)
pub struct KeyedMapping<N, const WIDTH: usize> {
    pub(crate) keys: [u8; WIDTH],
    pub(crate) children: Box<[MaybeUninit<N>; WIDTH]>,
    pub(crate) num_children: u8,
    pub(crate) occupied_bitset: Bitset16<1>,
}

impl<N, const WIDTH: usize> Default for KeyedMapping<N, WIDTH> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N, const WIDTH: usize> KeyedMapping<N, WIDTH> {
    #[inline]
    pub fn new() -> Self {
        Self {
            keys: [255; WIDTH],
            children: Box::new(unsafe { MaybeUninit::uninit().assume_init() }),
            num_children: 0,
            occupied_bitset: Default::default(),
        }
    }

    pub(crate) fn from_indexed<const IDX_WIDTH: usize, const IDX_BITSIZE: usize>(
        im: &mut IndexedMapping<N, IDX_WIDTH, IDX_BITSIZE>,
    ) -> Self {
        let mut new_mapping = KeyedMapping::new();
        im.num_children = 0;
        im.move_into(&mut new_mapping);
        new_mapping
    }

    pub fn from_sorted<const OLD_WIDTH: usize>(km: &mut SortedKeyedMapping<N, OLD_WIDTH>) -> Self {
        let mut new = KeyedMapping::new();
        for i in 0..km.num_children as usize {
            new.keys[i] = km.keys[i];
            new.children[i] = std::mem::replace(&mut km.children[i], MaybeUninit::uninit())
        }
        new.num_children = km.num_children;
        km.num_children = 0;
        new
    }

    pub fn from_resized_grow<const OLD_WIDTH: usize>(km: &mut KeyedMapping<N, OLD_WIDTH>) -> Self {
        assert!(WIDTH > OLD_WIDTH);
        let mut new = KeyedMapping::new();

        // Since we're larger than before, we can just copy over and expand everything, occupied
        // or not.
        new.occupied_bitset = std::mem::take(&mut km.occupied_bitset);
        for i in 0..OLD_WIDTH {
            new.keys[i] = km.keys[i];
            new.children[i] = std::mem::replace(&mut km.children[i], MaybeUninit::uninit());
        }
        km.occupied_bitset.clear();
        new.num_children = km.num_children;
        new
    }

    pub fn from_resized_shrink<const OLD_WIDTH: usize>(
        km: &mut KeyedMapping<N, OLD_WIDTH>,
    ) -> Self {
        assert!(WIDTH < OLD_WIDTH);
        let mut new = KeyedMapping::new();
        let mut cnt = 0;

        // Since we're smaller, we compact empty spots out.
        for i in 0..OLD_WIDTH {
            if km.occupied_bitset.check(i) {
                new.keys[cnt] = km.keys[i];
                new.children[cnt] = std::mem::replace(&mut km.children[i], MaybeUninit::uninit());

                new.occupied_bitset.set(cnt);
                cnt += 1;
            }
        }
        km.occupied_bitset.clear();
        new.num_children = km.num_children;
        km.num_children = 0;
        new
    }

    #[inline]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u8, &N)> {
        self.keys
            .iter()
            .enumerate()
            .filter(|p| self.occupied_bitset.check(p.0))
            .map(|(p, k)| (*k, unsafe { self.children[p].assume_init_ref() }))
    }
}

impl<N, const WIDTH: usize> NodeMapping<N> for KeyedMapping<N, WIDTH> {
    #[inline]
    fn add_child(&mut self, key: u8, node: N) {
        // Find an empty position by seeking for 255.
        let idx = self
            .occupied_bitset
            .first_empty()
            .expect("add_child: no space left");
        assert!(idx < WIDTH);
        self.keys[idx] = key;
        self.children[idx].write(node);
        self.occupied_bitset.set(idx);
        self.num_children += 1;
    }

    fn update_child(&mut self, key: u8, node: N) {
        *self.seek_child_mut(key).unwrap() = node;
    }

    fn seek_child(&self, key: u8) -> Option<&N> {
        let idx = u8_keys_find_key_position::<WIDTH>(key, &self.keys, WIDTH)?;
        if !self.occupied_bitset.check(idx) {
            return None;
        }
        Some(unsafe { self.children[idx].assume_init_ref() })
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        let idx = u8_keys_find_key_position::<WIDTH>(key, &self.keys, WIDTH)?;
        if !self.occupied_bitset.check(idx) {
            return None;
        }
        return Some(unsafe { self.children[idx].assume_init_mut() });
    }

    fn delete_child(&mut self, key: u8) -> Option<N> {
        // Find position of the key
        let idx = u8_keys_find_key_position::<WIDTH>(key, &self.keys, WIDTH)?;

        if !self.occupied_bitset.check(idx) {
            return None;
        }

        // Remove the value.
        let node = std::mem::replace(&mut self.children[idx], MaybeUninit::uninit());

        self.keys[idx] = 255;
        self.occupied_bitset.unset(idx);
        self.num_children -= 1;

        // Return what we deleted.
        Some(unsafe { node.assume_init() })
    }
    #[inline(always)]
    fn num_children(&self) -> usize {
        self.num_children as usize
    }

    #[inline(always)]
    fn width(&self) -> usize {
        WIDTH
    }
}

impl<N, const WIDTH: usize> Drop for KeyedMapping<N, WIDTH> {
    fn drop(&mut self) {
        for i in self.occupied_bitset.iter() {
            unsafe { self.children[i].assume_init_drop() }
        }
        self.num_children = 0;
        self.occupied_bitset.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::mapping::keyed_mapping::KeyedMapping;
    use crate::node::NodeMapping;

    #[test]
    fn test_add_seek_delete() {
        let mut node = KeyedMapping::<u8, 4>::new();
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
    // Verify that the memory width of the node is nice and compact.
    fn test_memory_width() {
        // num_children = 1 byte
        // keys = 4 bytes
        // children array ptr = 8
        // bitset = 2
        // total = 15 padded to 16 bytes
        assert_eq!(std::mem::size_of::<KeyedMapping<Box<u8>, 4>>(), 16);

        // num_children = 1 byte
        // keys = 16 bytes
        // children array ptr = 8
        // bitset = 2
        // total = 27 padded to 32 bytes
        assert_eq!(std::mem::size_of::<KeyedMapping<Box<u8>, 16>>(), 32);
    }
}
