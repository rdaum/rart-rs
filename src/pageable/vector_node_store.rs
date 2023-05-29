use crate::mapping::direct_mapping::DirectNodeMapping;
use crate::mapping::indexed_mapping::IndexedNodeMapping;
use crate::mapping::keyed_mapping::KeyedChildMapping;
use crate::node::NodeMapping;
use crate::pageable::node_store::{AddChildResult, NodeStore, RmChildResult};
use crate::pageable::pageable_tree::PrefixTraits;
use crate::utils::fillvector::{FillVector, FVIndex};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NodeT {
    Node2 = 0,
    Node4 = 1,
    Node16 = 2,
    Node48 = 3,
    Node256 = 4,
    Leaf = 5,
}

#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct Node {
    node_type: NodeT,
    index: FVIndex,
}

#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct NodeId {
    node: FVIndex,
    prefix: FVIndex
}

impl NodeId {
    pub fn new(mapping: FVIndex, prefix: FVIndex) -> Self {
        Self { node: mapping, prefix }
    }
}

/// A node store that uses our "fill vector" to store nodes and leaves.
pub struct VectorNodeStore<P: PrefixTraits, V> {
    prefixes: FillVector<P>,
    // A level of indirection -- node id to general node info.
    // So we can 'rewrite' what node ids back to be any of the key mappings below here, when we
    // do removals, splits, etc.
    // Unfortunately adds a measurable performance impact.
    nodes: FillVector<Node>,
    node_2s: FillVector<KeyedChildMapping<NodeId, 2>>,
    node_4s: FillVector<KeyedChildMapping<NodeId, 4>>,
    node_16s: FillVector<KeyedChildMapping<NodeId, 16>>,
    node_48s: FillVector<IndexedNodeMapping<NodeId, 48, 1>>,
    node_256s: FillVector<DirectNodeMapping<NodeId>>,
    leaves: FillVector<V>,
}

impl<P: PrefixTraits, V> VectorNodeStore<P, V> {
    pub fn new() -> Self {
        Self {
            prefixes: FillVector::with_capacity(256),
            nodes: FillVector::with_capacity(32),
            node_2s: FillVector::with_capacity(32),
            node_4s: FillVector::with_capacity(32),
            node_16s: FillVector::with_capacity(32),
            node_48s: FillVector::with_capacity(32),
            node_256s: FillVector::with_capacity(32),
            leaves: FillVector::with_capacity(256),
        }
    }
    fn max_width(&self, node_type: &NodeT) -> usize {
        match node_type {
            NodeT::Node2 => 2,
            NodeT::Node4 => 4,
            NodeT::Node16 => 16,
            NodeT::Node48 => 48,
            NodeT::Node256 => 256,
            NodeT::Leaf => 0,
        }
    }
    pub fn new_4(&mut self, prefix: P) -> NodeId {
        let prefix = self.prefixes.add(|_i| prefix);
        let index = self.node_4s.add(|_i| KeyedChildMapping::new());
        let node = self.nodes.add(|_| Node {
            node_type: NodeT::Node4,
            index,
        });
        NodeId::new(node, prefix)
    }
    pub fn new_16(&mut self, prefix: P) -> NodeId {
        let prefix = self.prefixes.add(|_i| prefix);
        let index = self.node_16s.add(|_i| KeyedChildMapping::new());
        let node = self.nodes.add(|_| Node {
            node_type: NodeT::Node16,
            index,
        });
        NodeId::new(node, prefix)
    }
    pub fn new_48(&mut self, prefix: P) -> NodeId {
        let prefix = self.prefixes.add(|_i| prefix);
        let index = self.node_48s.add(|_i| IndexedNodeMapping::new());
        let node = self.nodes.add(|_| Node {
            node_type: NodeT::Node48,
            index,
        });
        NodeId::new(node, prefix)
    }
    pub fn new_256(&mut self, prefix: P) -> NodeId {
        let prefix = self.prefixes.add(|_i| prefix);
        let index = self.node_256s.add(|_i| DirectNodeMapping::new());
        let node = self.nodes.add(|_| Node {
            node_type: NodeT::Node256,
            index,
        });
        NodeId::new(node, prefix)
    }

    fn is_full(&self, node: &NodeId) -> bool {
        let node_type = &self.nodes[node.node].node_type;
        self.num_children(node) == self.max_width(node_type)
    }

    fn grow_node(&mut self, node_ptr: &NodeId) {
        let node = &mut self.nodes[node_ptr.node];
        match node.node_type {
            NodeT::Node2 => {
                let km = &mut self.node_2s[node.index];
                let index = self.node_4s.add(|_idx| km.resized());
                self.node_2s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node4,
                    index,
                };
            }
            NodeT::Node4 => {
                let km = &mut self.node_4s[node.index];
                let index = self.node_16s.add(|_idx| km.resized());
                self.node_4s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node16,
                    index,
                };
            }
            NodeT::Node16 => {
                let km = &mut self.node_16s[node.index];
                let index = self.node_48s.add(|_idx| km.to_indexed());
                self.node_16s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node48,
                    index,
                };
            }
            NodeT::Node48 => {
                let im = &mut self.node_48s[node.index];
                let index = self.node_256s.add(|_idx| im.to_direct());
                self.node_48s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node256,
                    index,
                };
            }
            NodeT::Node256 => {
                unreachable!("grow called on node256")
            }
            NodeT::Leaf => {
                unreachable!("grow called on leaf node")
            }
        }
    }
    fn can_shrink(&self, nptr: &NodeId) -> bool {
        let node = &self.nodes[nptr.node];
        match &node.node_type {
            NodeT::Node2 => false,
            NodeT::Node4 => self.node_4s[node.index].num_children() <= 2,
            NodeT::Node16 => self.node_16s[node.index].num_children() <= 4,
            NodeT::Node48 => self.node_48s[node.index].num_children() <= 16,
            NodeT::Node256 => self.node_256s[node.index].num_children() <= 48,
            NodeT::Leaf => false
        }
    }
    fn shrink_node(&mut self, nptr: &NodeId) {
        let node = &mut self.nodes[nptr.node];
        match node.node_type {
            NodeT::Node2 => unreachable!("shrink called on node2"),
            NodeT::Node4 => {
                let km = &mut self.node_4s[node.index];
                let index = self.node_2s.add(|_idx| km.resized());
                self.node_4s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node2,
                    index,
                };
            }
            NodeT::Node16 => {
                let km = &mut self.node_16s[node.index];
                let index = self.node_4s.add(|_idx| km.resized());
                self.node_16s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node4,
                    index,
                };
            }
            NodeT::Node48 => {
                let km = &mut self.node_48s[node.index];
                let index = self.node_16s.add(|_idx| km.to_keyed());
                self.node_48s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node16,
                    index,
                };
            }
            NodeT::Node256 => {
                let km = &mut self.node_256s[node.index];
                let index = self.node_48s.add(|_idx| km.to_indexed());
                self.node_256s.free(node.index);
                *node = Node {
                    node_type: NodeT::Node48,
                    index,
                };
            }
            NodeT::Leaf => unreachable!("shrink called on leaf node")
        }
    }

    pub fn num_prefixes(&self) -> usize {
        self.prefixes.size()
    }

    pub fn num_leaves(&self) -> usize {
        self.leaves.size()
    }

    pub fn num_inners(&self) -> usize {
        self.node_2s.size()
            + self.node_4s.size()
            + self.node_16s.size()
            + self.node_48s.size()
            + self.node_256s.size()
    }
}

impl<P: PrefixTraits, V> Default for VectorNodeStore<P, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: PrefixTraits, V> NodeStore<NodeId, P, V> for VectorNodeStore<P, V> {
    fn is_leaf(&self, node: &NodeId) -> bool {
        let node = &self.nodes[node.node];
        matches!(node.node_type, NodeT::Leaf)
    }

    fn is_inner(&self, node: &NodeId) -> bool {
        !self.is_leaf(node)
    }

    fn node_prefix(&self, node: &NodeId) -> &P {
        &self.prefixes[node.prefix]
    }
    fn set_node_prefix(&mut self, node: &NodeId, prefix: P) {
        self.prefixes[node.prefix] = prefix;
    }
    fn leaf_value(&self, node: &NodeId) -> Option<&V> {
        let node = &self.nodes[node.node];
        match node.node_type {
            NodeT::Leaf => Some(&self.leaves[node.index]),
            _ => None,
        }
    }
    fn set_leaf_value(&mut self, nptr: &NodeId, value: V) -> V {
        let node = &mut self.nodes[nptr.node];
        match node.node_type {
            NodeT::Leaf => std::mem::replace(&mut self.leaves[node.index], value),
            _ => unreachable!("set_value called on non-leaf node"),
        }
    }
    fn seek_child(&self, parent: &NodeId, key: u8) -> Option<&NodeId> {
        let parent = &self.nodes[parent.node];
        let node_mapping: &dyn NodeMapping<NodeId> = match parent.node_type {
            NodeT::Node2 => &self.node_2s[parent.index],
            NodeT::Node4 => &self.node_4s[parent.index],
            NodeT::Node16 => &self.node_16s[parent.index],
            NodeT::Node48 => &self.node_48s[parent.index],
            NodeT::Node256 => &self.node_256s[parent.index],
            NodeT::Leaf => {
                return None;
            }
        };
        node_mapping.seek_child(key)
    }
    fn add_child_to_node(
        &mut self,
        node: &NodeId,
        key: u8,
        child: NodeId,
    ) -> AddChildResult {
        let mut replaced = false;
        if self.is_full(node) {
            replaced = true;
            self.grow_node(node)
        }
        let node = &self.nodes[node.node];
        let node_mapping: &mut dyn NodeMapping<NodeId> = match node.node_type {
            NodeT::Node2 => &mut self.node_2s[node.index],
            NodeT::Node4 => &mut self.node_4s[node.index],
            NodeT::Node16 => &mut self.node_16s[node.index],
            NodeT::Node48 => &mut self.node_48s[node.index],
            NodeT::Node256 => &mut self.node_256s[node.index],
            NodeT::Leaf => {
                unreachable!("add_child_to_node called on leaf node")
            }
        };
        node_mapping.add_child(key, child);
        if replaced {
            AddChildResult::Grown
        } else {
            AddChildResult::Same
        }
    }
    fn update_child_in_node(&mut self, node: &NodeId, key: u8, child: NodeId) {
        let node = &self.nodes[node.node];
        let node_mapping: &mut dyn NodeMapping<NodeId> = match node.node_type {
            NodeT::Node2 => &mut self.node_2s[node.index],
            NodeT::Node4 => &mut self.node_4s[node.index],
            NodeT::Node16 => &mut self.node_16s[node.index],
            NodeT::Node48 => &mut self.node_48s[node.index],
            NodeT::Node256 => &mut self.node_256s[node.index],
            NodeT::Leaf => {
                return;
            }
        };
        node_mapping.update_child(key, child);
    }
    fn delete_node_from_parent(&mut self, pptr: &NodeId, key: u8) -> RmChildResult {
        let parent = &self.nodes[pptr.node];
        let node_mapping: &mut dyn NodeMapping<NodeId> = match parent.node_type {
            NodeT::Node2 => &mut self.node_2s[parent.index],
            NodeT::Node4 => &mut self.node_4s[parent.index],
            NodeT::Node16 => &mut self.node_16s[parent.index],
            NodeT::Node48 => &mut self.node_48s[parent.index],
            NodeT::Node256 => &mut self.node_256s[parent.index],
            NodeT::Leaf => {
                unreachable!("free_node_from_parent called on leaf node");
            }
        };
        let child = node_mapping.delete_child(key).expect("child not found");
        assert!(self.free_node(child));
        if self.can_shrink(pptr) {
            self.shrink_node(pptr);
            return RmChildResult::Shrunk;
        }
        RmChildResult::Same
    }

    fn free_node(&mut self, nodeid: NodeId) -> bool {
        let node = &self.nodes[nodeid.node];
        match node.node_type {
            NodeT::Node2 => self.node_2s.free(node.index),
            NodeT::Node4 => self.node_4s.free(node.index),
            NodeT::Node16 => self.node_16s.free(node.index),
            NodeT::Node48 => self.node_48s.free(node.index),
            NodeT::Node256 => self.node_256s.free(node.index),
            NodeT::Leaf => self.leaves.free(node.index),
        };
        self.prefixes.free(nodeid.prefix)
    }

    fn num_children(&self, node: &NodeId) -> usize {
        let node = &self.nodes[node.node];
        match node.node_type {
            NodeT::Node2 => self.node_2s[node.index].num_children(),
            NodeT::Node4 => self.node_4s[node.index].num_children(),
            NodeT::Node16 => self.node_16s[node.index].num_children(),
            NodeT::Node48 => self.node_48s[node.index].num_children(),
            NodeT::Node256 => self.node_256s[node.index].num_children(),
            NodeT::Leaf => 0,
        }
    }
    fn new_leaf(&mut self, key: &[u8], value: V) -> NodeId {
        let index = self.leaves.add(|_i| value);
        let prefix = self.prefixes.add(|_i| key.into());
        let node = self.nodes.add(|_| {
            Node {
                node_type: NodeT::Leaf,
                index,
            }
        });
        NodeId::new(node, prefix)
    }
    fn new_inner(&mut self, prefix: P) -> NodeId {
        let prefix = self.prefixes.add(|_i| prefix);
        let index = self.node_2s.add(|_i| KeyedChildMapping::new());
        let node = self.nodes.add(|_| {
            Node {
                node_type: NodeT::Node2,
                index,
            }
        });
        NodeId::new(node, prefix)
    }
}

#[cfg(test)]
mod tests {
    use crate::pageable::node_store::{AddChildResult, RmChildResult};
    use crate::pageable::vector_node_store::{
        NodeStore, VectorNodeStore,
    };
    use crate::partials::array_partial::ArrPartial;

    // Test the update_child_in_node function for keyed mappings
    #[test]
    fn test_add_replace_node_keyed() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_inner(ArrPartial::from_slice(&[0, 1, 2, 3]));
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 1);
        let AddChildResult::Same = store.add_child_to_node(&node, 0, new_leaf) else {
            panic!("should not have grown");
        };
        let child = store.seek_child(&node, 0).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&1));

        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 2);
        store.update_child_in_node(&node, 0, new_leaf);
        assert_eq!(store.num_children(&node), 1);
        let child = store.seek_child(&node, 0).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&2));
    }

    // Test the update_child_in_node function for indexed mappings
    #[test]
    fn test_add_replace_node_indexed() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_48(ArrPartial::from_slice(&[0, 1, 2, 3]));
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 1);
        let AddChildResult::Same = store.add_child_to_node(&node, 0, new_leaf) else {
            panic!("should not have grown");
        };
        let child = store.seek_child(&node, 0).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&1));

        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 2);
        store.update_child_in_node(&node, 0, new_leaf);
        assert_eq!(store.num_children(&node), 1);
        let child = store.seek_child(&node, 0).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&2));
    }

    #[test]
    fn test_add_replace_node_direct() {
        // Test the update_child_in_node function for indexed mappings
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_256(ArrPartial::from_slice(&[0, 1, 2, 3]));
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 1);
        let AddChildResult::Same = store.add_child_to_node(&node, 0, new_leaf) else {
            panic!("should not have grown");
        };
        let child = store.seek_child(&node, 0).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&1));

        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 2);
        store.update_child_in_node(&node, 0, new_leaf);
        assert_eq!(store.num_children(&node), 1);
        let child = store.seek_child(&node, 0).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&2));
    }

    #[test]
    fn split_leaf_to_node() {
        // Test the equivalent of what the tree code does when it takes a leaf node, and reparents
        // it under a new n2, along with a sibling.
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();

        // Start with a 'root' node
        let root = store.new_inner(ArrPartial::from_slice(&[0, 1, 2, 3]));

        // Then add a leaf inside it.
        let initial_leaf = store.new_leaf(&[0, 1, 2, 3], 1);

        let AddChildResult::Same = store.add_child_to_node(&root, 0, initial_leaf) else {
            panic!("should not have grown");
        };

        // Now create a new n2 node, and add the leaf as a child.
        let new_node = store.new_inner(ArrPartial::from_slice(&[0, 1, 2]));
        let new_leaf = store.new_leaf(&[0, 1, 2, 4], 2);

        // Update the prefix of our new version of our leaf, so we make sure that the update "takes"
        store.set_node_prefix(&initial_leaf, ArrPartial::from_slice(&[1, 2]));
        let AddChildResult::Same = store.add_child_to_node(&new_node, 2, initial_leaf) else {
            panic!("should not have grown");
        };
        let AddChildResult::Same = store.add_child_to_node(&new_node, 3, new_leaf) else {
            panic!("should not have grown");
        };
        assert_eq!(store.num_children(&new_node), 2);

        // And rewrite the root's link to the new node.
        store.update_child_in_node(&root, 0, new_node);

        // Now start from the root, we should find our new node.
        let new_2 = store.seek_child(&root, 0).expect("should have my child");
        assert_eq!(store.num_children(new_2), 2);
        assert_eq!(new_2, &new_node);

        let child = store.seek_child(new_2, 2).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&1));
        assert_eq!(store.node_prefix(child), &ArrPartial::from_slice(&[1, 2]));

        let child = store.seek_child(new_2, 3).expect("should have my child");
        assert!(store.is_leaf(child));
        assert_eq!(store.leaf_value(child), Some(&2));
    }

    #[test]
    fn test_grow_node2() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_inner(ArrPartial::from_slice(&[0, 1, 2, 3]));
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 1);
        let AddChildResult::Same = store.add_child_to_node(&node, 0, new_leaf) else {
            panic!("should not have grown");
        };
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 2);
        let AddChildResult::Same = store.add_child_to_node(&node, 1, new_leaf) else {
            panic!("should not have grown");
        };
        assert_eq!(store.num_children(&node), 2);
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 4], 3);

        let AddChildResult::Grown = store.add_child_to_node(&node, 2, new_leaf) else {
            panic!("should have grown");
        };
        assert_eq!(store.num_children(&node), 3);
    }

    #[test]
    fn test_shrink_node16() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_16(ArrPartial::from_slice(&[0, 1, 2, 3]));
        for i in 0..16 {
            let new_leaf = store.new_leaf(&[0, 1, 2, 3, i], i as u64);
            let AddChildResult::Same = store.add_child_to_node(&node, i, new_leaf) else {
                panic!("should not have grown");
            };
        }
        assert_eq!(store.num_children(&node), 16);
        for i in 5..16 {
            let RmChildResult::Same = store.delete_node_from_parent(&node, i) else {
                panic!("should not have shrunk");
            };
        }
        assert_eq!(store.num_children(&node), 5);
        let RmChildResult::Shrunk = store.delete_node_from_parent(&node, 2) else {
            panic!("should shrunk");
        };
        assert_eq!(store.num_children(&node), 4);
    }

    #[test]
    fn test_grow_node16() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_16(ArrPartial::from_slice(&[0, 1, 2, 3]));
        for i in 0..16 {
            let new_leaf = store.new_leaf(&[0, 1, 2, 3, i], i as u64);
            let AddChildResult::Same = store.add_child_to_node(&node, i, new_leaf) else {
                panic!("should not have grown");
            };
        }
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 16], 16);
        let AddChildResult::Grown = store.add_child_to_node(&node, 16, new_leaf) else {
            panic!("should have grown");
        };
        assert_eq!(store.num_children(&node), 17);
    }

    #[test]
    fn test_shrink_node48() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_48(ArrPartial::from_slice(&[0, 1, 2, 3]));
        for i in 0..48 {
            let new_leaf = store.new_leaf(&[0, 1, 2, 3, i], i as u64);
            let AddChildResult::Same = store.add_child_to_node(&node, i, new_leaf) else {
                panic!("should not have grown");
            };
        }
        assert_eq!(store.num_children(&node), 48);
        for i in 17..48 {
            let RmChildResult::Same = store.delete_node_from_parent(&node, i) else {
                panic!("should not have shrunk (i={})", i);
            };
        }
        assert_eq!(store.num_children(&node), 17);
        let RmChildResult::Shrunk = store.delete_node_from_parent(&node, 2) else {
            panic!("should have shrunk");
        };
        assert_eq!(store.num_children(&node), 16);
    }

    #[test]
    fn test_grow_node48() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_48(ArrPartial::from_slice(&[0, 1, 2, 3]));
        for i in 0..48 {
            let new_leaf = store.new_leaf(&[0, 1, 2, 3, i], i as u64);
            let AddChildResult::Same = store.add_child_to_node(&node, i, new_leaf) else {
                panic!("should not have grown");
            };
        }
        let new_leaf = store.new_leaf(&[0, 1, 2, 3, 48], 48);
        let AddChildResult::Grown = store.add_child_to_node(&node, 48, new_leaf) else {
            panic!("should have grown");
        };
        assert_eq!(store.num_children(&node), 49);
    }

    #[test]
    fn test_shrink_node256() {
        let mut store = VectorNodeStore::<ArrPartial<16>, u64>::new();
        let node = store.new_256(ArrPartial::from_slice(&[0, 1, 2, 3]));
        for i in 0..49 {
            let new_leaf = store.new_leaf(&[0, 1, 2, 3, i], i as u64);
            let AddChildResult::Same = store.add_child_to_node(&node, i, new_leaf) else {
                panic!("should not have grown");
            };
        }
        assert_eq!(store.num_children(&node), 49);
        let RmChildResult::Shrunk = store.delete_node_from_parent(&node, 2) else {
            panic!("should have shrunk");
        };
        assert_eq!(store.num_children(&node), 48);
    }
}
