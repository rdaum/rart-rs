use crate::mapping::direct_mapping::DirectMapping;
use crate::mapping::indexed_mapping::IndexedMapping;
use crate::mapping::keyed_mapping::KeyedMapping;
use crate::mapping::NodeMapping;
use crate::partials::Partial;
use crate::utils::bitset::{Bitset16, Bitset64, Bitset8};

pub trait Node<P: Partial, V> {
    fn new_leaf(partial: P, value: V) -> Self;
    fn new_inner(prefix: P) -> Self;
    fn value_mut(&mut self) -> Option<&mut V>;
    fn value(&self) -> Option<&V>;
    fn is_leaf(&self) -> bool;
    fn is_inner(&self) -> bool;
    fn num_children(&self) -> usize;
    fn seek_child(&self, key: u8) -> Option<&Self>;
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut Self>;
    fn delete_child(&mut self, key: u8) -> Option<Self>
    where
        Self: Sized;
    fn add_child(&mut self, key: u8, node: Self);
    fn capacity(&self) -> usize;
}

pub struct DefaultNode<P: Partial, V> {
    pub(crate) prefix: P,
    pub(crate) content: Content<P, V>,
}

pub(crate) enum Content<P: Partial, V> {
    Leaf(V),
    Node4(KeyedMapping<DefaultNode<P, V>, 4, Bitset8<1>>),
    Node16(KeyedMapping<DefaultNode<P, V>, 16, Bitset16<1>>),
    Node48(IndexedMapping<DefaultNode<P, V>, 48, Bitset64<1>>),
    Node256(DirectMapping<DefaultNode<P, V>>),
}

impl<P: Partial, V> Node<P, V> for DefaultNode<P, V> {
    #[inline]
    fn new_leaf(partial: P, value: V) -> Self {
        Self {
            prefix: partial,
            content: Content::Leaf(value),
        }
    }

    #[inline]
    fn new_inner(prefix: P) -> Self {
        let nt = Content::Node4(KeyedMapping::new());
        Self {
            prefix,
            content: nt,
        }
    }

    fn value(&self) -> Option<&V> {
        let Content::Leaf(value) = &self.content else {
            return None;
        };
        Some(value)
    }

    #[allow(dead_code)]
    fn value_mut(&mut self) -> Option<&mut V> {
        let Content::Leaf(value) = &mut self.content else {
            return None;
        };
        Some(value)
    }

    fn is_leaf(&self) -> bool {
        matches!(&self.content, Content::Leaf(_))
    }

    fn is_inner(&self) -> bool {
        !self.is_leaf()
    }

    fn num_children(&self) -> usize {
        match &self.content {
            Content::Node4(n) => n.num_children(),
            Content::Node16(n) => n.num_children(),
            Content::Node48(n) => n.num_children(),
            Content::Node256(n) => n.num_children(),
            Content::Leaf(_) => 0,
        }
    }
    fn seek_child(&self, key: u8) -> Option<&Self> {
        if self.num_children() == 0 {
            return None;
        }

        match &self.content {
            Content::Node4(km) => km.seek_child(key),
            Content::Node16(km) => km.seek_child(key),
            Content::Node48(km) => km.seek_child(key),
            Content::Node256(children) => children.seek_child(key),
            Content::Leaf(_) => None,
        }
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut Self> {
        match &mut self.content {
            Content::Node4(km) => km.seek_child_mut(key),
            Content::Node16(km) => km.seek_child_mut(key),
            Content::Node48(km) => km.seek_child_mut(key),
            Content::Node256(children) => children.seek_child_mut(key),
            Content::Leaf(_) => None,
        }
    }

    fn add_child(&mut self, key: u8, node: Self) {
        if self.is_full() {
            self.grow();
        }

        match &mut self.content {
            Content::Node4(km) => {
                km.add_child(key, node);
            }
            Content::Node16(km) => {
                km.add_child(key, node);
            }
            Content::Node48(im) => {
                im.add_child(key, node);
            }
            Content::Node256(pm) => {
                pm.add_child(key, node);
            }
            Content::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    fn delete_child(&mut self, key: u8) -> Option<Self> {
        match &mut self.content {
            Content::Node4(dm) => {
                let node = dm.delete_child(key);

                if self.num_children() == 1 {
                    self.shrink();
                }

                node
            }
            Content::Node16(dm) => {
                let node = dm.delete_child(key);

                if self.num_children() < 5 {
                    self.shrink();
                }
                node
            }
            Content::Node48(im) => {
                let node = im.delete_child(key);

                if self.num_children() < 17 {
                    self.shrink();
                }

                // Return what we deleted.
                node
            }
            Content::Node256(pm) => {
                let node = pm.delete_child(key);
                if self.num_children() < 49 {
                    self.shrink();
                }

                // Return what we deleted.
                node
            }
            Content::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    fn capacity(&self) -> usize {
        match &self.content {
            Content::Node4 { .. } => 4,
            Content::Node16 { .. } => 16,
            Content::Node48 { .. } => 48,
            Content::Node256 { .. } => 256,
            Content::Leaf(_) => 0,
        }
    }
}

impl<P: Partial, V> DefaultNode<P, V> {
    #[inline]
    #[allow(dead_code)]
    pub fn new_4(prefix: P) -> Self {
        let nt = Content::Node4(KeyedMapping::new());
        Self {
            prefix,
            content: nt,
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_16(prefix: P) -> Self {
        let nt = Content::Node16(KeyedMapping::new());
        Self {
            prefix,
            content: nt,
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_48(prefix: P) -> Self {
        let nt = Content::Node48(IndexedMapping::new());
        Self {
            prefix,
            content: nt,
        }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn new_256(prefix: P) -> Self {
        let nt = Content::Node256(DirectMapping::new());
        Self {
            prefix,
            content: nt,
        }
    }

    #[inline]
    fn is_full(&self) -> bool {
        match &self.content {
            Content::Node4(km) => self.num_children() >= km.width(),
            Content::Node16(km) => self.num_children() >= km.width(),
            Content::Node48(im) => self.num_children() >= im.width(),
            // Should not be possible.
            Content::Node256(_) => self.num_children() >= 256,
            Content::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    fn shrink(&mut self) {
        match &mut self.content {
            Content::Node4(km) => {
                // A node4 with only one child has its childed collapsed into it.
                // If our child is a leaf, that means we have become a leaf, and we can shrink no
                // more beyond this.
                let (_, child) = km.take_value_for_leaf();
                let prefix = child.prefix;
                self.content = child.content;
                self.prefix = self.prefix.partial_extended_with(&prefix);
            }
            Content::Node16(km) => {
                self.content = Content::Node4(KeyedMapping::from_resized_shrink(km));
            }
            Content::Node48(im) => {
                let new_node = Content::Node16(KeyedMapping::from_indexed(im));
                self.content = new_node;
            }
            Content::Node256(dm) => {
                self.content = Content::Node48(IndexedMapping::from_direct(dm));
            }
            Content::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    fn grow(&mut self) {
        match &mut self.content {
            Content::Node4(km) => {
                self.content = Content::Node16(KeyedMapping::from_resized_grow(km))
            }
            Content::Node16(km) => self.content = Content::Node48(IndexedMapping::from_keyed(km)),
            Content::Node48(im) => {
                self.content = Content::Node256(DirectMapping::from_indexed(im));
            }
            Content::Node256 { .. } => {
                unreachable!("Should never grow a node256")
            }
            Content::Leaf(_) => unreachable!("Should not be possible."),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn free(&self) -> usize {
        self.capacity() - self.num_children()
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> Box<dyn Iterator<Item = (u8, &Self)> + '_> {
        return match &self.content {
            Content::Node4(n) => Box::new(n.iter()),
            Content::Node16(n) => Box::new(n.iter()),
            Content::Node48(n) => Box::new(n.iter()),
            Content::Node256(n) => Box::new(n.iter().map(|(k, v)| (k, v))),
            Content::Leaf(_) => Box::new(std::iter::empty()),
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::node::{DefaultNode, Node};
    use crate::partials::array_partial::ArrPartial;

    #[test]
    fn test_n4() {
        let test_key: ArrPartial<16> = ArrPartial::key("abc".as_bytes());

        let mut n4 = DefaultNode::new_4(test_key.clone());
        n4.add_child(5, DefaultNode::new_leaf(test_key.clone(), 1));
        n4.add_child(4, DefaultNode::new_leaf(test_key.clone(), 2));
        n4.add_child(3, DefaultNode::new_leaf(test_key.clone(), 3));
        n4.add_child(2, DefaultNode::new_leaf(test_key.clone(), 4));

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

        n4.add_child(2, DefaultNode::new_leaf(test_key, 4));
        n4.delete_child(3);
        assert!(n4.seek_child(5).is_none());
        assert!(n4.seek_child(3).is_none());
    }

    #[test]
    fn test_n16() {
        let test_key: ArrPartial<16> = ArrPartial::key("abc".as_bytes());

        let mut n16 = DefaultNode::new_16(test_key.clone());

        // Fill up the node with keys in reverse order.
        for i in (0..16).rev() {
            n16.add_child(i, DefaultNode::new_leaf(test_key.clone(), i));
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

        let mut n48 = DefaultNode::new_48(test_key.clone());

        // indexes in n48 have no sort order, so we don't look at that
        for i in 0..48 {
            n48.add_child(i, DefaultNode::new_leaf(test_key.clone(), i));
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

        let mut n256 = DefaultNode::new_256(test_key.clone());

        for i in 0..=255 {
            n256.add_child(i, DefaultNode::new_leaf(test_key.clone(), i));
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
