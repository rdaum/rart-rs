use crate::mapping::NodeMapping;
use crate::mapping::indexed_mapping::IndexedMapping;
use crate::utils::bitarray::BitArray;
use crate::utils::bitset::{Bitset64, BitsetTrait};

pub struct DirectMapping<N> {
    pub(crate) children: BitArray<N, 256, Bitset64<4>>,
    num_children: usize,
}

impl<N> Default for DirectMapping<N> {
    fn default() -> Self {
        Self::new()
    }
}

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
    pub fn iter(&self) -> impl Iterator<Item = (u8, &N)> {
        self.children.iter().map(|(key, node)| (key as u8, node))
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
}
