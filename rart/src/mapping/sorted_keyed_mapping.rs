use std::mem::MaybeUninit;

use crate::mapping::indexed_mapping::IndexedMapping;
use crate::mapping::NodeMapping;
use crate::utils::bitset::BitsetTrait;
use crate::utils::u8_keys::{u8_keys_find_insert_position, u8_keys_find_key_position_sorted};

/// Maps a key to a node, using a sorted array of keys and a corresponding array of nodes.
/// Presence of a key at a position means there is a node at the same position in children.
/// Empty nodes are represented by 255.
/// By keeping nodes in a sorted array, we can use binary search to find the key, but we also
/// use SIMD instructions to speed up the search on platforms that have it.
/// When an item is inserted or deleted the items to the left and right of it are shifted, in
/// order to keep the array sorted.
/// *Note* this version is currently unused, as it is slower than the unsorted version on x86_64
/// with sse. If benching on other platforms shows it to be faster, then we can use it, so the
/// code is kept here.
pub struct SortedKeyedMapping<N, const WIDTH: usize> {
    pub(crate) keys: [u8; WIDTH],
    pub(crate) children: Box<[MaybeUninit<N>; WIDTH]>,
    pub(crate) num_children: u8,
}

impl<N, const WIDTH: usize> Default for SortedKeyedMapping<N, WIDTH> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N, const WIDTH: usize> SortedKeyedMapping<N, WIDTH> {
    #[inline]
    pub fn new() -> Self {
        Self {
            keys: [255; WIDTH],
            children: Box::new(unsafe { MaybeUninit::uninit().assume_init() }),
            num_children: 0,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_indexed<const IDX_WIDTH: usize, FromBitset: BitsetTrait>(
        im: &mut IndexedMapping<N, IDX_WIDTH, FromBitset>,
    ) -> Self {
        let mut new_mapping = SortedKeyedMapping::new();
        im.num_children = 0;
        im.move_into(&mut new_mapping);
        new_mapping
    }

    pub fn from_resized<const OLD_WIDTH: usize>(km: &mut SortedKeyedMapping<N, OLD_WIDTH>) -> Self {
        let mut new = SortedKeyedMapping::new();
        for i in 0..km.num_children as usize {
            new.keys[i] = km.keys[i];
            new.children[i] = std::mem::replace(&mut km.children[i], MaybeUninit::uninit())
        }
        new.num_children = km.num_children;
        km.num_children = 0;
        new
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u8, &N)> {
        self.keys
            .iter()
            .zip(self.children.iter())
            .take(self.num_children as usize)
            .map(|(&k, c)| (k, unsafe { c.assume_init_ref() }))
    }
}

impl<N, const WIDTH: usize> NodeMapping<N, WIDTH> for SortedKeyedMapping<N, WIDTH> {
    #[inline]
    fn add_child(&mut self, key: u8, node: N) {
        let idx =
            u8_keys_find_insert_position::<WIDTH>(key, &self.keys, self.num_children as usize)
                .expect("add_child: no space left");
        for i in (idx..self.num_children as usize).rev() {
            self.keys[i + 1] = self.keys[i];
            self.children[i + 1] = std::mem::replace(&mut self.children[i], MaybeUninit::uninit());
        }
        self.keys[idx] = key;
        self.children[idx].write(node);
        self.num_children += 1;
    }

    fn update_child(&mut self, key: u8, node: N) {
        *self.seek_child_mut(key).unwrap() = node;
    }

    fn seek_child(&self, key: u8) -> Option<&N> {
        let idx =
            u8_keys_find_key_position_sorted::<WIDTH>(key, &self.keys, self.num_children as usize)?;
        Some(unsafe { self.children[idx].assume_init_ref() })
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        let idx =
            u8_keys_find_key_position_sorted::<WIDTH>(key, &self.keys, self.num_children as usize)?;
        return Some(unsafe { self.children[idx].assume_init_mut() });
    }

    fn delete_child(&mut self, key: u8) -> Option<N> {
        // Find position of the key
        let idx =
            u8_keys_find_key_position_sorted::<WIDTH>(key, &self.keys, self.num_children as usize)?;

        // Remove the value.
        let node = std::mem::replace(&mut self.children[idx], MaybeUninit::uninit());

        // Shift keys and children to the left.
        for i in idx..(WIDTH - 1) {
            self.keys[i] = self.keys[i + 1];
            self.children[i] = std::mem::replace(&mut self.children[i + 1], MaybeUninit::uninit());
        }

        // Fix the last key and child and adjust count.
        self.keys[WIDTH - 1] = 255;
        self.children[WIDTH - 1] = MaybeUninit::uninit();

        self.num_children -= 1;

        // Return what we deleted.
        Some(unsafe { node.assume_init() })
    }
    #[inline(always)]
    fn num_children(&self) -> usize {
        self.num_children as usize
    }
}

impl<N, const WIDTH: usize> Drop for SortedKeyedMapping<N, WIDTH> {
    fn drop(&mut self) {
        for value in &mut self.children[..self.num_children as usize] {
            unsafe { value.assume_init_drop() }
        }
        self.num_children = 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::mapping::sorted_keyed_mapping::{NodeMapping, SortedKeyedMapping};

    #[test]
    fn test_add_seek_delete() {
        let mut node = SortedKeyedMapping::<u8, 4>::new();
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
        // 16 is padded width for 4 children
        // num_children = 1
        // keys = 4
        // children array ptr = 8
        // total = 13 pads out to 16
        assert_eq!(std::mem::size_of::<SortedKeyedMapping<Box<u8>, 4>>(), 16);

        // 32 is the padded size of the struct on account of
        // num_children + keys (u8) + children ptrs
        assert_eq!(std::mem::size_of::<SortedKeyedMapping<Box<u8>, 16>>(), 32);
    }
}
