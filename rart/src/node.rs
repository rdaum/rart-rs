use crate::mapping::direct_mapping::DirectMapping;
use crate::mapping::indexed_mapping::IndexedMapping;
use crate::mapping::keyed_mapping::KeyedMapping;
use crate::mapping::NodeMapping;
use crate::partials::Partial;
use crate::utils::bitset::{Bitset16, Bitset64, Bitset8};

pub(crate) struct Node<P: Partial, V> {
    pub(crate) prefix: P,
    pub(crate) ntype: NodeType<P, V>,
}

pub(crate) enum NodeType<P: Partial, V> {
    Leaf(V),
    Node4(KeyedMapping<Node<P, V>, 4, Bitset8<1>>),
    Node16(KeyedMapping<Node<P, V>, 16, Bitset16<1>>),
    Node48(IndexedMapping<Node<P, V>, 48, Bitset64<1>>),
    Node256(DirectMapping<Node<P, V>>),
}

impl<P: Partial, V> Node<P, V> {
    #[inline]
    pub(crate) fn new_leaf(partial: P, value: V) -> Node<P, V> {
        Self {
            prefix: partial,
            ntype: NodeType::Leaf(value),
        }
    }

    #[inline]
    pub fn new_inner(prefix: P) -> Self {
        let nt = NodeType::Node4(KeyedMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_4(prefix: P) -> Self {
        let nt = NodeType::Node4(KeyedMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_16(prefix: P) -> Self {
        let nt = NodeType::Node16(KeyedMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_48(prefix: P) -> Self {
        let nt = NodeType::Node48(IndexedMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_256(prefix: P) -> Self {
        let nt = NodeType::Node256(DirectMapping::new());
        Self { prefix, ntype: nt }
    }

    pub fn value(&self) -> Option<&V> {
        let NodeType::Leaf(value) = &self.ntype else {
            return None;
        };
        Some(value)
    }

    #[allow(dead_code)]
    pub fn value_mut(&mut self) -> Option<&mut V> {
        let NodeType::Leaf(value) = &mut self.ntype else {
            return None;
        };
        Some(value)
    }

    pub fn is_leaf(&self) -> bool {
        matches!(&self.ntype, NodeType::Leaf(_))
    }

    pub fn is_inner(&self) -> bool {
        !self.is_leaf()
    }

    pub fn num_children(&self) -> usize {
        match &self.ntype {
            NodeType::Node4(n) => n.num_children(),
            NodeType::Node16(n) => n.num_children(),
            NodeType::Node48(n) => n.num_children(),
            NodeType::Node256(n) => n.num_children(),
            NodeType::Leaf(_) => 0,
        }
    }
    pub(crate) fn seek_child(&self, key: u8) -> Option<&Node<P, V>> {
        if self.num_children() == 0 {
            return None;
        }

        match &self.ntype {
            NodeType::Node4(km) => km.seek_child(key),
            NodeType::Node16(km) => km.seek_child(key),
            NodeType::Node48(km) => km.seek_child(key),
            NodeType::Node256(children) => children.seek_child(key),
            NodeType::Leaf(_) => None,
        }
    }

    pub(crate) fn seek_child_mut(&mut self, key: u8) -> Option<&mut Node<P, V>> {
        match &mut self.ntype {
            NodeType::Node4(km) => km.seek_child_mut(key),
            NodeType::Node16(km) => km.seek_child_mut(key),
            NodeType::Node48(km) => km.seek_child_mut(key),
            NodeType::Node256(children) => children.seek_child_mut(key),
            NodeType::Leaf(_) => None,
        }
    }

    pub(crate) fn add_child(&mut self, key: u8, node: Node<P, V>) {
        if self.is_full() {
            self.grow();
        }

        match &mut self.ntype {
            NodeType::Node4(km) => {
                km.add_child(key, node);
            }
            NodeType::Node16(km) => {
                km.add_child(key, node);
            }
            NodeType::Node48(im) => {
                im.add_child(key, node);
            }
            NodeType::Node256(pm) => {
                pm.add_child(key, node);
            }
            NodeType::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    pub(crate) fn delete_child(&mut self, key: u8) -> Option<Node<P, V>> {
        match &mut self.ntype {
            NodeType::Node4(dm) => {
                let node = dm.delete_child(key);

                if self.num_children() == 1 {
                    self.shrink();
                }

                node
            }
            NodeType::Node16(dm) => {
                let node = dm.delete_child(key);

                if self.num_children() < 5 {
                    self.shrink();
                }
                node
            }
            NodeType::Node48(im) => {
                let node = im.delete_child(key);

                if self.num_children() < 17 {
                    self.shrink();
                }

                // Return what we deleted.
                node
            }
            NodeType::Node256(pm) => {
                let node = pm.delete_child(key);
                if self.num_children() < 49 {
                    self.shrink();
                }

                // Return what we deleted.
                node
            }
            NodeType::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    #[inline]
    fn is_full(&self) -> bool {
        match &self.ntype {
            NodeType::Node4(km) => self.num_children() >= km.width(),
            NodeType::Node16(km) => self.num_children() >= km.width(),
            NodeType::Node48(im) => self.num_children() >= im.width(),
            // Should not be possible.
            NodeType::Node256(_) => self.num_children() >= 256,
            NodeType::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    fn shrink(&mut self) {
        match &mut self.ntype {
            NodeType::Node4(km) => {
                // A node4 with only one child has its childed collapsed into it.
                // If our child is a leaf, that means we have become a leaf, and we can shrink no
                // more beyond this.
                let (_, child) = km.take_value_for_leaf();
                let prefix = child.prefix;
                self.ntype = child.ntype;
                self.prefix = self.prefix.partial_extended_with(&prefix);
            }
            NodeType::Node16(km) => {
                self.ntype = NodeType::Node4(KeyedMapping::from_resized_shrink(km));
            }
            NodeType::Node48(im) => {
                let new_node = NodeType::Node16(KeyedMapping::from_indexed(im));
                self.ntype = new_node;
            }
            NodeType::Node256(dm) => {
                self.ntype = NodeType::Node48(IndexedMapping::from_direct(dm));
            }
            NodeType::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    fn grow(&mut self) {
        match &mut self.ntype {
            NodeType::Node4(km) => {
                self.ntype = NodeType::Node16(KeyedMapping::from_resized_grow(km))
            }
            NodeType::Node16(km) => self.ntype = NodeType::Node48(IndexedMapping::from_keyed(km)),
            NodeType::Node48(im) => {
                self.ntype = NodeType::Node256(DirectMapping::from_indexed(im));
            }
            NodeType::Node256 { .. } => {
                unreachable!("Should never grow a node256")
            }
            NodeType::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        match &self.ntype {
            NodeType::Node4 { .. } => 4,
            NodeType::Node16 { .. } => 16,
            NodeType::Node48 { .. } => 48,
            NodeType::Node256 { .. } => 256,
            NodeType::Leaf(_) => 0,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn free(&self) -> usize {
        self.capacity() - self.num_children()
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> Box<dyn Iterator<Item = (u8, &Self)> + '_> {
        return match &self.ntype {
            NodeType::Node4(n) => Box::new(n.iter()),
            NodeType::Node16(n) => Box::new(n.iter()),
            NodeType::Node48(n) => Box::new(n.iter()),
            NodeType::Node256(n) => Box::new(n.iter().map(|(k, v)| (k, v))),
            NodeType::Leaf(_) => Box::new(std::iter::empty()),
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::node::Node;
    use crate::partials::array_partial::ArrPartial;

    #[test]
    fn test_n4() {
        let test_key: ArrPartial<16> = ArrPartial::key("abc".as_bytes());

        let mut n4 = Node::new_4(test_key.clone());
        n4.add_child(5, Node::new_leaf(test_key.clone(), 1));
        n4.add_child(4, Node::new_leaf(test_key.clone(), 2));
        n4.add_child(3, Node::new_leaf(test_key.clone(), 3));
        n4.add_child(2, Node::new_leaf(test_key.clone(), 4));

        assert_eq!(*n4.seek_child(5).unwrap().value().unwrap(), 1);
        assert_eq!(*n4.seek_child(4).unwrap().value().unwrap(), 2);
        assert_eq!(*n4.seek_child(3).unwrap().value().unwrap(), 3);
        assert_eq!(*n4.seek_child(2).unwrap().value().unwrap(), 4);

        n4.delete_child(5);
        assert!(n4.seek_child(5).is_none());
        assert_eq!(*n4.seek_child(4).unwrap().value().unwrap(), 2);
        assert_eq!(*n4.seek_child(3).unwrap().value().unwrap(), 3);
        assert_eq!(*n4.seek_child(2).unwrap().value().unwrap(), 4);

        n4.delete_child(2);
        assert!(n4.seek_child(5).is_none());
        assert!(n4.seek_child(2).is_none());

        n4.add_child(2, Node::new_leaf(test_key, 4));
        n4.delete_child(3);
        assert!(n4.seek_child(5).is_none());
        assert!(n4.seek_child(3).is_none());
    }

    #[test]
    fn test_n16() {
        let test_key: ArrPartial<16> = ArrPartial::key("abc".as_bytes());

        let mut n16 = Node::new_16(test_key.clone());

        // Fill up the node with keys in reverse order.
        for i in (0..16).rev() {
            n16.add_child(i, Node::new_leaf(test_key.clone(), i));
        }

        for i in 0..16 {
            assert_eq!(*n16.seek_child(i).unwrap().value().unwrap(), i);
        }

        // Delete from end doesn't affect position of others.
        n16.delete_child(15);
        n16.delete_child(14);
        assert!(n16.seek_child(15).is_none());
        assert!(n16.seek_child(14).is_none());
        for i in 0..14 {
            assert_eq!(*n16.seek_child(i).unwrap().value().unwrap(), i);
        }

        n16.delete_child(0);
        n16.delete_child(1);
        assert!(n16.seek_child(0).is_none());
        assert!(n16.seek_child(1).is_none());
        for i in 2..14 {
            assert_eq!(*n16.seek_child(i).unwrap().value().unwrap(), i);
        }

        // Delete from the middle
        n16.delete_child(5);
        n16.delete_child(6);
        assert!(n16.seek_child(5).is_none());
        assert!(n16.seek_child(6).is_none());
        for i in 2..5 {
            assert_eq!(*n16.seek_child(i).unwrap().value().unwrap(), i);
        }
        for i in 7..14 {
            assert_eq!(*n16.seek_child(i).unwrap().value().unwrap(), i);
        }
    }

    #[test]
    fn test_n48() {
        let test_key: ArrPartial<16> = ArrPartial::key("abc".as_bytes());

        let mut n48 = Node::new_48(test_key.clone());

        // indexes in n48 have no sort order, so we don't look at that
        for i in 0..48 {
            n48.add_child(i, Node::new_leaf(test_key.clone(), i));
        }

        for i in 0..48 {
            assert_eq!(*n48.seek_child(i).unwrap().value().unwrap(), i);
        }

        n48.delete_child(47);
        n48.delete_child(46);
        assert!(n48.seek_child(47).is_none());
        assert!(n48.seek_child(46).is_none());
        for i in 0..46 {
            assert_eq!(*n48.seek_child(i).unwrap().value().unwrap(), i);
        }
    }

    #[test]
    fn test_n_256() {
        let test_key: ArrPartial<16> = ArrPartial::key("abc".as_bytes());

        let mut n256 = Node::new_256(test_key.clone());

        for i in 0..=255 {
            n256.add_child(i, Node::new_leaf(test_key.clone(), i));
        }
        for i in 0..=255 {
            assert_eq!(*n256.seek_child(i).unwrap().value().unwrap(), i);
        }

        n256.delete_child(47);
        n256.delete_child(46);
        assert!(n256.seek_child(47).is_none());
        assert!(n256.seek_child(46).is_none());
        for i in 0..46 {
            assert_eq!(*n256.seek_child(i).unwrap().value().unwrap(), i);
        }
        for i in 48..=255 {
            assert_eq!(*n256.seek_child(i).unwrap().value().unwrap(), i);
        }
    }
}
