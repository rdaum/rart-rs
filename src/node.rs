use crate::mapping::direct_mapping::DirectNodeMapping;
use crate::mapping::indexed_boxed_mapping::IndexedBoxedNodeMapping;
use crate::mapping::keyed_boxed_mapping::KeyedBoxedChildMapping;
use crate::Partial;

pub(crate) struct Node<P: Partial + Clone, V> {
    pub(crate) prefix: P,
    pub(crate) ntype: NodeType<P, V>,
}

pub trait NodeMapping<N> {
    fn add_child(&mut self, key: u8, node: N);
    fn update_child(&mut self, key: u8, node: N);
    fn seek_child(&self, key: u8) -> Option<&N>;
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N>;
    fn delete_child(&mut self, key: u8) -> Option<N>;
    fn num_children(&self) -> usize;
    fn width(&self) -> usize;
}

pub(crate) enum NodeType<P: Partial + Clone, V> {
    Leaf(V),
    Node2(KeyedBoxedChildMapping<Node<P, V>, 2>),
    Node4(KeyedBoxedChildMapping<Node<P, V>, 4>),
    Node16(KeyedBoxedChildMapping<Node<P, V>, 16>),
    Node48(IndexedBoxedNodeMapping<Node<P, V>, 48, 1>),
    Node256(DirectNodeMapping<Node<P, V>>),
}

impl<P: Partial + Clone, V> Node<P, V> {
    #[inline]
    pub(crate) fn new_leaf(key: P, value: V) -> Node<P, V> {
        Self {
            prefix: key,
            ntype: NodeType::Leaf(value),
        }
    }

    #[inline]
    pub fn new_inner(prefix: P) -> Self {
        let nt = NodeType::Node2(KeyedBoxedChildMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_4(prefix: P) -> Self {
        let nt = NodeType::Node4(KeyedBoxedChildMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_16(prefix: P) -> Self {
        let nt = NodeType::Node16(KeyedBoxedChildMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_48(prefix: P) -> Self {
        let nt = NodeType::Node48(IndexedBoxedNodeMapping::new());
        Self { prefix, ntype: nt }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_256(prefix: P) -> Self {
        let nt = NodeType::Node256(DirectNodeMapping::new());
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
            NodeType::Node2(n) => n.num_children(),
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
            NodeType::Node2(dm) => dm.seek_child(key),
            NodeType::Node4(dm) => dm.seek_child(key),
            NodeType::Node16(dm) => dm.seek_child(key),
            NodeType::Node48(im) => im.seek_child(key),
            NodeType::Node256(children) => children.seek_child(key),
            NodeType::Leaf(_) => None,
        }
    }

    pub(crate) fn seek_child_mut(&mut self, key: u8) -> Option<&mut Node<P, V>> {
        match &mut self.ntype {
            NodeType::Node2(dm) => dm.seek_child_mut(key),
            NodeType::Node4(dm) => dm.seek_child_mut(key),
            NodeType::Node16(dm) => dm.seek_child_mut(key),
            NodeType::Node48(im) => im.seek_child_mut(key),
            NodeType::Node256(children) => children.seek_child_mut(key),
            NodeType::Leaf(_) => None,
        }
    }

    pub(crate) fn add_child(&mut self, key: u8, node: Node<P, V>) {
        if self.is_full() {
            self.grow();
        }

        match &mut self.ntype {
            NodeType::Node2(dm) => {
                dm.add_child(key, node);
            }
            NodeType::Node4(dm) => {
                dm.add_child(key, node);
            }
            NodeType::Node16(dm) => {
                dm.add_child(key, node);
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
            NodeType::Node2(dm) => dm.delete_child(key),
            NodeType::Node4(dm) => {
                let node = dm.delete_child(key);

                if self.num_children() < 3 {
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
            NodeType::Node2(km) => self.num_children() >= km.width(),
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
            NodeType::Node2(_dm) => {
                unreachable!("Should never shrink a node4")
            }
            NodeType::Node4(km) => {
                self.ntype = NodeType::Node2(km.resized());
            }
            NodeType::Node16(km) => {
                self.ntype = NodeType::Node4(km.resized());
            }
            NodeType::Node48(im) => {
                let km = im.to_keyed();

                let new_node = NodeType::Node16(km);
                self.ntype = new_node;
            }
            NodeType::Node256(dm) => {
                let im = dm.to_indexed_boxed();
                self.ntype = NodeType::Node48(im);
            }
            NodeType::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    fn grow(&mut self) {
        match &mut self.ntype {
            NodeType::Node2(km) => {
                let new_node = NodeType::Node4(km.resized());
                self.ntype = new_node
            }
            NodeType::Node4(km) => {
                let new_node = NodeType::Node16(km.resized());
                self.ntype = new_node
            }
            NodeType::Node16(km) => {
                let im = km.to_indexed();
                self.ntype = NodeType::Node48(im)
            }
            NodeType::Node48(im) => {
                let dm = im.to_direct();
                self.ntype = NodeType::Node256(dm);
            }
            NodeType::Node256 { .. } => {
                unreachable!("Should never grow a node256")
            }
            NodeType::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        match &self.ntype {
            NodeType::Node2 { .. } => 2,
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
            NodeType::Node2(n) => Box::new(n.iter()),
            NodeType::Node4(n) => Box::new(n.iter()),
            NodeType::Node16(n) => Box::new(n.iter()),
            NodeType::Node48(n) => Box::new(n.iter()),
            NodeType::Node256(n) => Box::new(n.iter().map(|(k, v)| (k, v))),
            NodeType::Leaf(_) => Box::new(std::iter::empty()),
        };
    }

    pub fn node_type_name(&self) -> String {
        match &self.ntype {
            NodeType::Node2(_) => "Node2".to_string(),
            NodeType::Node4(_) => "Node4".to_string(),
            NodeType::Node16(_) => "Node16".to_string(),
            NodeType::Node48(_) => "Node48".to_string(),
            NodeType::Node256(_) => "Node256".to_string(),
            NodeType::Leaf(_) => "Leaf".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::node::Node;
    use crate::partials::shared_partial::{SharedPartial, SharedPartialRoot};

    #[test]
    fn test_n4() {
        let test_key: SharedPartial<16> = SharedPartialRoot::key("abc".as_bytes());

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
        let test_key: SharedPartial<16> = SharedPartialRoot::key("abc".as_bytes());

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
        let test_key: SharedPartial<16> = SharedPartialRoot::key("abc".as_bytes());

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
        let test_key: SharedPartial<16> = SharedPartialRoot::key("abc".as_bytes());

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
