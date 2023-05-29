use crate::tree::PrefixTraits;

/// The result of adding a child to a parent node in the node store.
pub enum AddChildResult {
    /// The node has not grown.
    Same,
    /// The node has grown.
    Grown,
}

/// The result of removing a child node from a parent node in the store.
pub enum RmChildResult {
    /// The node has not shrunk.
    Same,
    /// The node has shrunk .
    Shrunk,
}

pub trait NodeStore<NP, P: PrefixTraits, V> {
    fn is_leaf(&self, node: &NP) -> bool;
    fn is_inner(&self, node: &NP) -> bool;

    /// Return the prefix of the given node.
    fn node_prefix(&self, node: &NP) -> &P;

    /// Set the prefix of the given node.
    fn set_node_prefix(&mut self, node: &NP, prefix: P);

    /// Return the value of the given leaf node.
    fn leaf_value(&self, node: &NP) -> Option<&V>;

    /// Set the value of the given leaf node.
    fn set_leaf_value(&mut self, node: &NP, value: V) -> V;

    /// Seek a pointer to the child of the given node with the given key.
    fn seek_child(&self, parent: &NP, key: u8) -> Option<&NP>;

    /// Add a child to the given node, with the given key.
    /// Returns the new node pointer, and whether the node has grown.
    fn add_child_to_node(&mut self, node: &NP, key: u8, child: NP) -> AddChildResult;

    /// Update the child of the given node with the given key.
    fn update_child_in_node(&mut self, node: &NP, key: u8, child: NP);

    /// Free the given node at the given key and remove it from its parent.
    fn delete_node_from_parent(&mut self, parent: &NP, key: u8) -> RmChildResult;

    /// Free the given node (used when the node is not referenced by a parent node, e.g. is a root).
    fn free_node(&mut self, node: NP) -> bool;

    /// Return the number of children of this node.
    fn num_children(&self, node: &NP) -> usize;

    /// Create a new leaf node in this store, with the given key prefix (relative to its parent)
    /// and value.
    fn new_leaf(&mut self, key: &[u8], value: V) -> NP;

    /// Create a new new inner node of the smallest possible size, with the given key prefix (
    /// relative to its parent node). The node must be wide enough to hold at least 2 children.
    fn new_inner(&mut self, prefix: P) -> NP;
}
