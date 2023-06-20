use std::mem::MaybeUninit;

use crate::mapping::indexed_mapping::IndexedMapping;
use crate::node::NodeMapping;
use crate::utils::u8_keys::{u8_keys_find_insert_position, u8_keys_find_key_position};

pub struct KeyedMapping<N, const WIDTH: usize> {
    pub(crate) keys: [u8; WIDTH],
    pub(crate) children: Box<[MaybeUninit<N>; WIDTH]>,
    pub(crate) num_children: u8,
}

impl<N, const WIDTH: usize> KeyedMapping<N, WIDTH> {
    #[inline]
    pub fn new() -> Self {
        Self {
            keys: [255; WIDTH],
            // TODO for the 4-wide version, keeping this in box is excessive, as the node size
            // will still be small. But to make the code simpler, we'll keep it in a box for now.
            children: Box::new(unsafe { MaybeUninit::uninit().assume_init() }),
            num_children: 0,
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

    pub fn from_resized<const OLD_WIDTH: usize>(km: &mut KeyedMapping<N, OLD_WIDTH>) -> Self {
        let mut new = KeyedMapping::new();
        for i in 0..km.num_children as usize {
            new.keys[i] = km.keys[i];
            new.children[i] = std::mem::replace(&mut km.children[i], MaybeUninit::uninit())
        }
        new.num_children = km.num_children;
        km.num_children = 0;
        new
    }

    #[inline]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u8, &N)> {
        self.keys
            .iter()
            .zip(self.children.iter())
            .take(self.num_children as usize)
            .map(|(&k, c)| (k, unsafe { c.assume_init_ref() }))
    }
}

impl<N, const WIDTH: usize> NodeMapping<N> for KeyedMapping<N, WIDTH> {
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
        let idx = u8_keys_find_key_position::<WIDTH>(key, &self.keys, self.num_children as usize)?;
        Some(unsafe { self.children[idx].assume_init_ref() })
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        let idx = u8_keys_find_key_position::<WIDTH>(key, &self.keys, self.num_children as usize)?;
        return Some(unsafe { self.children[idx].assume_init_mut() });
    }

    fn delete_child(&mut self, key: u8) -> Option<N> {
        // Find position of the key
        let idx = self
            .keys
            .iter()
            .take(self.num_children as usize)
            .position(|&k| k == key)?;

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
    #[inline(always)]
    fn width(&self) -> usize {
        WIDTH
    }
}

impl<N, const WIDTH: usize> Drop for KeyedMapping<N, WIDTH> {
    fn drop(&mut self) {
        for value in &mut self.children[..self.num_children as usize] {
            unsafe { value.assume_init_drop() }
        }
        self.num_children = 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::mapping::keyed_mapping::{KeyedMapping, NodeMapping};

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
        // 16 is padded width for 4 children
        // num_children = 1
        // keys = 4
        // children array ptr = 8
        // total = 13 pads out to 16
        assert_eq!(std::mem::size_of::<KeyedMapping<Box<u8>, 4>>(), 16);

        // 32 is the padded size of the struct on account of
        // num_children + keys (u8) + children ptrs
        assert_eq!(std::mem::size_of::<KeyedMapping<Box<u8>, 16>>(), 32);
    }
}
