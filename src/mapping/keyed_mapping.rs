use std::mem::MaybeUninit;

use crate::mapping::indexed_mapping::IndexedNodeMapping;
use crate::node::NodeMapping;
use crate::utils::u8_keys::{u8_keys_find_insert_position, u8_keys_find_key_position};

/// A keyed mapping in which the child array are stored in an unboxed (non-heap) array, which seems
/// to offer a performance improvement over the boxed version.
/// Works for Sized node types only.
pub struct KeyedChildMapping<N, const WIDTH: usize> {
    keys: [u8; WIDTH],
    children: [MaybeUninit<N>; WIDTH],
    num_children: u8,
}

impl<N, const WIDTH: usize> KeyedChildMapping<N, WIDTH> {
    #[inline]
    pub fn new() -> Self {
        Self {
            keys: [255; WIDTH],
            // TODO for the 4-wide version, keeping this in box is excessive, as the node size
            // will still be small. But to make the code simpler, we'll keep it in a box for now.
            children: unsafe { MaybeUninit::uninit().assume_init() },
            num_children: 0,
        }
    }

    pub fn resized<const NEW_WIDTH: usize>(&mut self) -> KeyedChildMapping<N, NEW_WIDTH> {
        let mut new: KeyedChildMapping<N, NEW_WIDTH> = KeyedChildMapping::new();
        for i in 0..self.num_children as usize {
            new.keys[i] = self.keys[i];
            new.children[i] = std::mem::replace(&mut self.children[i], MaybeUninit::uninit())
        }
        new.num_children = self.num_children;
        self.num_children = 0;
        new
    }

    pub fn to_indexed<const NEW_WIDTH: usize, const BITWIDTH: usize>(
        &mut self,
    ) -> IndexedNodeMapping<N, NEW_WIDTH, BITWIDTH> {
        let mut im: IndexedNodeMapping<N, NEW_WIDTH, BITWIDTH> = IndexedNodeMapping::new();
        for i in 0..self.num_children as usize {
            let stolen = std::mem::replace(&mut self.children[i], MaybeUninit::uninit());
            im.add_child(self.keys[i], unsafe { stolen.assume_init() });
        }
        self.num_children = 0;
        im
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

impl<N, const WIDTH: usize> NodeMapping<N> for KeyedChildMapping<N, WIDTH> {
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

impl<N, const WIDTH: usize> Drop for KeyedChildMapping<N, WIDTH> {
    fn drop(&mut self) {
        for value in &mut self.children[..self.num_children as usize] {
            unsafe { value.assume_init_drop() }
        }
        self.num_children = 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::mapping::keyed_mapping::KeyedChildMapping;
    use crate::node::NodeMapping;

    #[test]
    fn test_add_seek_delete() {
        let mut node = KeyedChildMapping::<u8, 4>::new();
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
}
