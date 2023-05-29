use crate::mapping::direct_mapping::DirectNodeMapping;
use crate::mapping::keyed_mapping::KeyedChildMapping;
use crate::node::NodeMapping;
use crate::utils::bitarray::BitArray;

// A mapping from keys to separate child pointers.
// This is the non-boxed version which only works with Sized node types
pub struct IndexedNodeMapping<N, const WIDTH: usize, const BITWIDTH: usize> {
    child_ptr_indexes: BitArray<u8, 256, 4>,
    children: BitArray<N, WIDTH, BITWIDTH>,
    num_children: u8,
}

impl<N, const WIDTH: usize, const BITWIDTH: usize> IndexedNodeMapping<N, WIDTH, BITWIDTH> {
    pub fn new() -> Self {
        Self {
            child_ptr_indexes: BitArray::new(),
            children: BitArray::new(),
            num_children: 0,
        }
    }

    pub(crate) fn to_keyed<const NEW_WIDTH: usize>(&mut self) -> KeyedChildMapping<N, NEW_WIDTH> {
        let mut new_mapping = KeyedChildMapping::<N, NEW_WIDTH>::new();
        self.num_children = 0;
        self.move_into(&mut new_mapping);
        new_mapping
    }

    pub(crate) fn to_direct(&mut self) -> DirectNodeMapping<N> {
        let mut new_mapping = DirectNodeMapping::<N>::new();
        self.num_children = 0;
        self.move_into(&mut new_mapping);
        new_mapping
    }

    pub(crate) fn resized<const NEW_WIDTH: usize>(
        &mut self,
    ) -> IndexedNodeMapping<N, NEW_WIDTH, BITWIDTH> {
        let mut new_mapping = IndexedNodeMapping::<N, NEW_WIDTH, BITWIDTH>::new();
        self.num_children = 0;
        self.move_into(&mut new_mapping);
        new_mapping
    }

    fn move_into<NM: NodeMapping<N>>(&mut self, nm: &mut NM) {
        for (key, pos) in self.child_ptr_indexes.iter() {
            let node = self.children.erase(*pos as usize).unwrap();
            nm.add_child(key as u8, node);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (u8, &N)> {
        self.child_ptr_indexes
            .iter()
            .map(move |(key, pos)| (key as u8, &self.children[*pos as usize]))
    }
}

impl<N, const WIDTH: usize, const BITWIDTH: usize> NodeMapping<N>
    for IndexedNodeMapping<N, WIDTH, BITWIDTH>
{
    fn add_child(&mut self, key: u8, node: N) {
        let pos = self.children.first_free_pos().unwrap();
        self.child_ptr_indexes.set(key as usize, pos as u8);
        self.children.set(pos, node);
        self.num_children += 1;
    }

    fn update_child(&mut self, key: u8, node: N) {
        if let Some(pos) = self.child_ptr_indexes.get(key as usize) {
            self.children.set(*pos as usize, node);
        }
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

    #[inline]
    fn width(&self) -> usize {
        WIDTH
    }
}

impl<N, const WIDTH: usize, const BITWIDTH: usize> Drop for IndexedNodeMapping<N, WIDTH, BITWIDTH> {
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
    use crate::node::NodeMapping;

    #[test]
    fn test_basic_mapping() {
        let mut mapping = super::IndexedNodeMapping::<u8, 48, 1>::new();
        for i in 0..48 {
            mapping.add_child(i, i);
            assert_eq!(*mapping.seek_child(i).unwrap(), i);
        }
        for i in 0..48 {
            assert_eq!(*mapping.seek_child(i).unwrap(), i);
        }
        for i in 0..48 {
            assert_eq!(mapping.delete_child(i).unwrap(), i);
        }
        for i in 0..48 {
            assert!(mapping.seek_child(i as u8).is_none());
        }
    }
}
