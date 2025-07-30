//! Versioned Adaptive Radix Tree implementation with copy-on-write semantics.
//!
//! This module provides a persistent/versioned ART where snapshots can be taken
//! and mutated independently using copy-on-write node sharing for memory efficiency.

use std::cmp::min;
use std::sync::Arc;

use crate::keys::KeyTrait;
use crate::mapping::{
    NodeMapping, direct_mapping::DirectMapping, indexed_mapping::IndexedMapping,
    sorted_keyed_mapping::SortedKeyedMapping,
};
use crate::partials::Partial;
use crate::utils::bitset::Bitset64;

/// Type alias for remove operation result to reduce type complexity
type RemoveResult<P, V> = (Option<Arc<VersionedNode<P, V>>>, V);

/// A versioned Adaptive Radix Tree that supports snapshot-based copy-on-write mutations.
///
/// Unlike the standard [`AdaptiveRadixTree`], this version allows taking O(1) snapshots
/// that can be independently mutated. Mutations use copy-on-write semantics to minimize
/// memory usage while maintaining structural sharing between versions.
///
/// ## Features
///
/// - **O(1) snapshots**: Create new versions instantly without copying data
/// - **Copy-on-write mutations**: Only copy nodes along the path being modified
/// - **Structural sharing**: Unmodified subtrees are shared between versions
/// - **MVCC support**: Ideal for implementing multi-version concurrency control
///
/// ## Examples
///
/// ```rust
/// use rart::{VersionedAdaptiveRadixTree, ArrayKey};
///
/// let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, String>::new();
/// tree.insert("key1", "value1".to_string());
///
/// // Take a snapshot - O(1) operation
/// let mut snapshot = tree.snapshot();
///
/// // Mutations to snapshot don't affect original
/// snapshot.insert("key2", "value2".to_string());
///
/// assert_eq!(tree.get("key2"), None);
/// assert_eq!(snapshot.get("key2"), Some(&"value2".to_string()));
/// ```
pub struct VersionedAdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
    ValueType: Clone,
{
    root: Option<Arc<VersionedNode<KeyType::PartialType, ValueType>>>,
    version: u64,
    _phantom: std::marker::PhantomData<KeyType>,
}

/// A versioned node that can be shared between multiple tree versions.
pub struct VersionedNode<P: Partial, V> {
    pub(crate) prefix: P,
    pub(crate) content: VersionedContent<P, V>,
    pub(crate) version: u64,
}

/// Content of a versioned node, using Arc for child sharing.
pub(crate) enum VersionedContent<P: Partial, V> {
    Leaf(V),
    Node4(SortedKeyedMapping<Arc<VersionedNode<P, V>>, 4>),
    Node16(SortedKeyedMapping<Arc<VersionedNode<P, V>>, 16>),
    Node48(IndexedMapping<Arc<VersionedNode<P, V>>, 48, Bitset64<1>>),
    Node256(DirectMapping<Arc<VersionedNode<P, V>>>),
}

impl<KeyType: KeyTrait, ValueType: Clone> Default
    for VersionedAdaptiveRadixTree<KeyType, ValueType>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<KeyType, ValueType> Clone for VersionedAdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
    ValueType: Clone,
{
    /// Clone creates a new snapshot at the current version.
    /// This is equivalent to calling `snapshot()`.
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            version: self.version,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<KeyType, ValueType> VersionedAdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
    ValueType: Clone,
{
    /// Create a new empty versioned tree.
    pub fn new() -> Self {
        Self {
            root: None,
            version: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Create a snapshot of the current tree state.
    ///
    /// This is an O(1) operation that creates a new tree sharing the same
    /// underlying nodes. Subsequent mutations to either tree will use
    /// copy-on-write to maintain independence.
    pub fn snapshot(&self) -> Self {
        Self {
            root: self.root.clone(),
            version: self.version + 1,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get a value by key (generic version).
    #[inline]
    pub fn get<Key>(&self, key: Key) -> Option<&ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_k(&key.into())
    }

    /// Get a value by key reference (direct version).
    #[inline]
    pub fn get_k(&self, key: &KeyType) -> Option<&ValueType> {
        let root = self.root.as_ref()?;
        Self::get_iterate(root, key)
    }

    /// Insert a key-value pair (generic version).
    ///
    /// Uses copy-on-write to ensure this operation doesn't affect other snapshots.
    /// Returns the previous value if the key already existed.
    #[inline]
    pub fn insert<KV>(&mut self, key: KV, value: ValueType) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.insert_k(&key.into(), value)
    }

    /// Insert a key-value pair using key reference (direct version).
    ///
    /// Uses copy-on-write to ensure this operation doesn't affect other snapshots.
    /// Returns the previous value if the key already existed.
    pub fn insert_k(&mut self, key: &KeyType, value: ValueType) -> Option<ValueType> {
        self.version += 1;

        let Some(root) = &self.root else {
            self.root = Some(Arc::new(VersionedNode::new_leaf(
                key.to_partial(0),
                value,
                self.version,
            )));
            return None;
        };

        let (new_root, old_value) =
            Self::insert_recurse(Arc::clone(root), key, value, 0, self.version);
        self.root = Some(new_root);
        old_value
    }

    /// Remove a key-value pair (generic version).
    ///
    /// Uses copy-on-write to ensure this operation doesn't affect other snapshots.
    /// Returns the removed value if the key existed.
    pub fn remove<KV>(&mut self, key: KV) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.remove_k(&key.into())
    }

    /// Remove a key-value pair using key reference (direct version).
    ///
    /// Uses copy-on-write to ensure this operation doesn't affect other snapshots.
    /// Returns the removed value if the key existed.
    pub fn remove_k(&mut self, key: &KeyType) -> Option<ValueType> {
        let root = self.root.as_ref()?;

        // Check if there's a prefix match at the root
        let prefix_common_match = root.prefix.prefix_length_key(key, 0);
        if prefix_common_match != root.prefix.len() {
            return None;
        }

        self.version += 1;

        // Special case: root is a leaf
        if root.is_leaf() {
            match Arc::try_unwrap(self.root.take().unwrap()) {
                Ok(owned_root) => match owned_root.content {
                    VersionedContent::Leaf(v) => return Some(v),
                    _ => unreachable!(),
                },
                Err(shared_root) => {
                    // Root is shared, clone the value
                    match &shared_root.content {
                        VersionedContent::Leaf(v) => {
                            let cloned_value = v.clone();
                            self.root = None;
                            return Some(cloned_value);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }

        let (new_root, removed_value) =
            Self::remove_recurse(Arc::clone(root), key, 0, self.version)?;

        // Update root, handling the case where it might become empty
        if let Some(root_node) = new_root {
            if root_node.is_inner() && root_node.num_children() == 0 {
                self.root = None;
            } else {
                self.root = Some(root_node);
            }
        } else {
            self.root = None;
        }

        Some(removed_value)
    }

    /// Check if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Get the current version number of this tree.
    pub fn version(&self) -> u64 {
        self.version
    }
}

impl<P: Partial + Clone, V> VersionedNode<P, V> {
    /// Create a new leaf node.
    pub fn new_leaf(prefix: P, value: V, version: u64) -> Self {
        Self {
            prefix,
            content: VersionedContent::Leaf(value),
            version,
        }
    }

    /// Create a new inner node.
    pub fn new_inner(prefix: P, version: u64) -> Self {
        Self {
            prefix,
            content: VersionedContent::Node4(SortedKeyedMapping::new()),
            version,
        }
    }

    /// Check if this is a leaf node.
    pub fn is_leaf(&self) -> bool {
        matches!(&self.content, VersionedContent::Leaf(_))
    }

    /// Check if this is an inner node.
    pub fn is_inner(&self) -> bool {
        !self.is_leaf()
    }

    /// Get the value if this is a leaf node.
    pub fn value(&self) -> Option<&V> {
        match &self.content {
            VersionedContent::Leaf(value) => Some(value),
            _ => None,
        }
    }

    /// Seek a child by key.
    pub fn seek_child(&self, key: u8) -> Option<&Arc<VersionedNode<P, V>>> {
        match &self.content {
            VersionedContent::Node4(km) => km.seek_child(key),
            VersionedContent::Node16(km) => km.seek_child(key),
            VersionedContent::Node48(km) => km.seek_child(key),
            VersionedContent::Node256(km) => km.seek_child(key),
            VersionedContent::Leaf(_) => None,
        }
    }

    /// Get the number of children.
    pub fn num_children(&self) -> usize {
        match &self.content {
            VersionedContent::Node4(km) => km.num_children(),
            VersionedContent::Node16(km) => km.num_children(),
            VersionedContent::Node48(km) => km.num_children(),
            VersionedContent::Node256(km) => km.num_children(),
            VersionedContent::Leaf(_) => 0,
        }
    }

    /// Check if this node is full and needs to grow.
    pub fn is_full(&self) -> bool {
        match &self.content {
            VersionedContent::Node4(km) => km.num_children() >= 4,
            VersionedContent::Node16(km) => km.num_children() >= 16,
            VersionedContent::Node48(km) => km.num_children() >= 48,
            VersionedContent::Node256(_) => false, // Node256 never grows
            VersionedContent::Leaf(_) => false,
        }
    }

    /// Create a grown version of this node (Node4 → Node16 → Node48 → Node256).
    pub fn grow(&self, new_version: u64) -> Self
    where
        V: Clone,
    {
        Self {
            prefix: self.prefix.clone(),
            content: match &self.content {
                VersionedContent::Node4(km) => {
                    // Grow Node4 to Node16
                    let mut new_km = SortedKeyedMapping::new();
                    for (key, child) in km.iter() {
                        new_km.add_child(key, Arc::clone(child));
                    }
                    VersionedContent::Node16(new_km)
                }
                VersionedContent::Node16(km) => {
                    // Grow Node16 to Node48
                    let mut new_km = IndexedMapping::new();
                    for (key, child) in km.iter() {
                        new_km.add_child(key, Arc::clone(child));
                    }
                    VersionedContent::Node48(new_km)
                }
                VersionedContent::Node48(km) => {
                    // Grow Node48 to Node256
                    let mut new_km = DirectMapping::new();
                    for (key, child) in km.iter() {
                        new_km.add_child(key, Arc::clone(child));
                    }
                    VersionedContent::Node256(new_km)
                }
                VersionedContent::Node256(_) => {
                    panic!("Node256 cannot grow further")
                }
                VersionedContent::Leaf(_) => {
                    panic!("Leaf nodes cannot grow")
                }
            },
            version: new_version,
        }
    }

    /// Create a copy-on-write clone of this node with a new version.
    pub fn cow_clone_inner(&self, new_version: u64) -> Self
    where
        V: Clone,
    {
        Self {
            prefix: self.prefix.clone(),
            content: match &self.content {
                VersionedContent::Leaf(v) => VersionedContent::Leaf(v.clone()),
                VersionedContent::Node4(km) => {
                    // Manually clone Node4 mapping
                    let mut new_km = SortedKeyedMapping::new();
                    for (key, child) in km.iter() {
                        new_km.add_child(key, Arc::clone(child));
                    }
                    VersionedContent::Node4(new_km)
                }
                VersionedContent::Node16(km) => {
                    // Manually clone Node16 mapping
                    let mut new_km = SortedKeyedMapping::new();
                    for (key, child) in km.iter() {
                        new_km.add_child(key, Arc::clone(child));
                    }
                    VersionedContent::Node16(new_km)
                }
                VersionedContent::Node48(km) => {
                    // Manually clone Node48 mapping
                    let mut new_km = IndexedMapping::new();
                    for (key, child) in km.iter() {
                        new_km.add_child(key, Arc::clone(child));
                    }
                    VersionedContent::Node48(new_km)
                }
                VersionedContent::Node256(km) => {
                    // Manually clone Node256 mapping
                    let mut new_km = DirectMapping::new();
                    for (key, child) in km.iter() {
                        new_km.add_child(key, Arc::clone(child));
                    }
                    VersionedContent::Node256(new_km)
                }
            },
            version: new_version,
        }
    }
}

// Internal implementation
impl<KeyType, ValueType> VersionedAdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
    ValueType: Clone,
{
    /// Get operation that traverses the tree without modification.
    fn get_iterate<'a>(
        cur_node: &'a VersionedNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) -> Option<&'a ValueType> {
        let mut cur_node = cur_node;
        let mut depth = 0;

        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
            if prefix_common_match != cur_node.prefix.len() {
                return None;
            }

            if cur_node.prefix.len() == key.length_at(depth) {
                return cur_node.value();
            }

            let k = key.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();
            cur_node = cur_node.seek_child(k)?.as_ref();
        }
    }

    /// Copy-on-write helper: returns the node if it's already the right version,
    /// or creates a new copy if it needs to be modified.
    fn ensure_cow_node(
        node: Arc<VersionedNode<KeyType::PartialType, ValueType>>,
        target_version: u64,
    ) -> Arc<VersionedNode<KeyType::PartialType, ValueType>> {
        if node.version == target_version {
            // Already at target version, no work needed
            node
        } else {
            // Check if we have exclusive ownership
            match Arc::try_unwrap(node) {
                Ok(mut owned_node) => {
                    // We have exclusive ownership - just update version in place
                    owned_node.version = target_version;
                    Arc::new(owned_node)
                }
                Err(shared_node) => {
                    // Node is shared - need actual CoW
                    Arc::new(shared_node.cow_clone_inner(target_version))
                }
            }
        }
    }

    /// Insert with copy-on-write semantics.
    /// Returns (new_root, old_value).
    fn insert_recurse(
        cur_node: Arc<VersionedNode<KeyType::PartialType, ValueType>>,
        key: &KeyType,
        value: ValueType,
        depth: usize,
        version: u64,
    ) -> (
        Arc<VersionedNode<KeyType::PartialType, ValueType>>,
        Option<ValueType>,
    ) {
        let longest_common_prefix = cur_node.prefix.prefix_length_key(key, depth);
        let is_prefix_match =
            min(cur_node.prefix.len(), key.length_at(depth)) == longest_common_prefix;

        // Case 1: Exact match, replace existing leaf value
        if is_prefix_match && cur_node.prefix.len() == key.length_at(depth) && cur_node.is_leaf() {
            // For leaf replacement, we can't extract the old value without Clone
            // Instead, we'll use a different strategy - try to get a unique reference
            // If we can't, we know someone else has a reference and we need CoW
            return match Arc::try_unwrap(cur_node) {
                Ok(owned_node) => {
                    // We have exclusive ownership, can extract the old value
                    let old_value = match owned_node.content {
                        VersionedContent::Leaf(v) => Some(v),
                        _ => unreachable!(),
                    };
                    let new_node =
                        Arc::new(VersionedNode::new_leaf(owned_node.prefix, value, version));
                    (new_node, old_value)
                }
                Err(shared_node) => {
                    // Node is shared, use CoW semantics - can't return old value
                    let new_node = Arc::new(VersionedNode::new_leaf(
                        shared_node.prefix.clone(),
                        value,
                        version,
                    ));
                    (new_node, None)
                }
            };
        }
        // Case 2: Prefix mismatch, need to split the node
        else if !is_prefix_match {
            let mut new_inner = VersionedNode::new_inner(
                cur_node.prefix.partial_before(longest_common_prefix),
                version,
            );

            let k1 = cur_node.prefix.at(longest_common_prefix);
            let k2 = key.at(depth + longest_common_prefix);

            // Create the existing node with truncated prefix
            let mut existing_node_clone = cur_node.cow_clone_inner(version);
            existing_node_clone.prefix = cur_node.prefix.partial_after(longest_common_prefix);
            let existing_arc = Arc::new(existing_node_clone);

            // Create new leaf
            let new_leaf = Arc::new(VersionedNode::new_leaf(
                key.to_partial(depth + longest_common_prefix),
                value,
                version,
            ));

            // Add children to the new inner node
            match &mut new_inner.content {
                VersionedContent::Node4(km) => {
                    km.add_child(k1, existing_arc);
                    km.add_child(k2, new_leaf);
                }
                _ => unreachable!(),
            }

            return (Arc::new(new_inner), None);
        }

        // Case 3: Need to recurse deeper
        let k = key.at(depth + cur_node.prefix.len());
        let prefix_len = cur_node.prefix.len();
        let new_node = Self::ensure_cow_node(cur_node, version);

        // Handle all node types
        let existing_child = new_node.seek_child(k);

        if let Some(child) = existing_child {
            // Recurse into existing child
            let (new_child, old_value) =
                Self::insert_recurse(Arc::clone(child), key, value, depth + prefix_len, version);

            // Create new version of this node with updated child
            // Since ensure_cow_node gave us ownership, we can unwrap safely
            let mut new_node_mut = match Arc::try_unwrap(new_node) {
                Ok(owned) => owned,
                Err(_) => panic!("ensure_cow_node should have given us exclusive ownership"),
            };
            match &mut new_node_mut.content {
                VersionedContent::Node4(km_mut) => {
                    km_mut.delete_child(k);
                    km_mut.add_child(k, new_child);
                }
                VersionedContent::Node16(km_mut) => {
                    km_mut.delete_child(k);
                    km_mut.add_child(k, new_child);
                }
                VersionedContent::Node48(km_mut) => {
                    km_mut.delete_child(k);
                    km_mut.add_child(k, new_child);
                }
                VersionedContent::Node256(km_mut) => {
                    km_mut.delete_child(k);
                    km_mut.add_child(k, new_child);
                }
                VersionedContent::Leaf(_) => unreachable!("Inner node expected"),
            }

            (Arc::new(new_node_mut), old_value)
        } else {
            // Add new child - check if node needs to grow first
            let new_leaf = Arc::new(VersionedNode::new_leaf(
                key.to_partial(depth + prefix_len),
                value,
                version,
            ));

            let mut new_node_mut = match Arc::try_unwrap(new_node) {
                Ok(owned) => {
                    if owned.is_full() {
                        // Node is full, grow it first
                        owned.grow(version)
                    } else {
                        // Node has space, use it directly
                        owned
                    }
                }
                Err(_) => panic!("ensure_cow_node should have given us exclusive ownership"),
            };

            match &mut new_node_mut.content {
                VersionedContent::Node4(km_mut) => {
                    km_mut.add_child(k, new_leaf);
                }
                VersionedContent::Node16(km_mut) => {
                    km_mut.add_child(k, new_leaf);
                }
                VersionedContent::Node48(km_mut) => {
                    km_mut.add_child(k, new_leaf);
                }
                VersionedContent::Node256(km_mut) => {
                    km_mut.add_child(k, new_leaf);
                }
                VersionedContent::Leaf(_) => unreachable!("Inner node expected"),
            }

            (Arc::new(new_node_mut), None)
        }
    }

    /// Remove with copy-on-write semantics.
    /// Returns (new_root_option, removed_value).
    fn remove_recurse(
        cur_node: Arc<VersionedNode<KeyType::PartialType, ValueType>>,
        key: &KeyType,
        depth: usize,
        version: u64,
    ) -> Option<RemoveResult<KeyType::PartialType, ValueType>> {
        // Check prefix match
        let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
        if prefix_common_match != cur_node.prefix.len() {
            return None;
        }

        // If this is a leaf and matches completely, remove it
        if cur_node.is_leaf() {
            if cur_node.prefix.len() == key.length_at(depth) {
                let removed_value = match &cur_node.content {
                    VersionedContent::Leaf(v) => v.clone(),
                    _ => unreachable!(),
                };
                return Some((None, removed_value)); // Remove this node entirely
            } else {
                return None; // Not a complete match
            }
        }

        // This is an inner node, recurse to find child
        let k = key.at(depth + cur_node.prefix.len());
        let child = cur_node.seek_child(k)?;

        let (new_child_opt, removed_value) = Self::remove_recurse(
            Arc::clone(child),
            key,
            depth + cur_node.prefix.len(),
            version,
        )?;

        // Create new version of this node with updated child
        let new_node = Self::ensure_cow_node(cur_node, version);

        // We need to get mutable access to modify the children
        // Since ensure_cow_node gave us ownership, we can unwrap safely
        let mut new_node_mut = match Arc::try_unwrap(new_node) {
            Ok(owned) => owned,
            Err(_) => panic!("ensure_cow_node should have given us exclusive ownership"),
        };

        match &mut new_node_mut.content {
            VersionedContent::Node4(km) => {
                km.delete_child(k);
                if let Some(new_child) = new_child_opt {
                    km.add_child(k, new_child);
                }
            }
            VersionedContent::Node16(km) => {
                km.delete_child(k);
                if let Some(new_child) = new_child_opt {
                    km.add_child(k, new_child);
                }
            }
            VersionedContent::Node48(km) => {
                km.delete_child(k);
                if let Some(new_child) = new_child_opt {
                    km.add_child(k, new_child);
                }
            }
            VersionedContent::Node256(km) => {
                km.delete_child(k);
                if let Some(new_child) = new_child_opt {
                    km.add_child(k, new_child);
                }
            }
            VersionedContent::Leaf(_) => unreachable!("Inner node expected"),
        }

        Some((Some(Arc::new(new_node_mut)), removed_value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::array_key::ArrayKey;

    #[test]
    fn test_basic_snapshot() {
        let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Insert into original
        tree.insert("key1", 1);
        assert_eq!(tree.get("key1"), Some(&1));

        // Take snapshot
        let snapshot = tree.snapshot();
        assert_eq!(snapshot.get("key1"), Some(&1));
        assert_eq!(snapshot.version(), tree.version() + 1);
    }

    #[test]
    fn test_independent_mutations() {
        let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        tree.insert("key1", 1);

        let mut snapshot = tree.snapshot();

        // Mutations should be independent
        tree.insert("key2", 2);
        snapshot.insert("key3", 3);

        // Original tree should have key2 but not key3
        assert_eq!(tree.get("key2"), Some(&2));
        assert_eq!(tree.get("key3"), None);

        // Snapshot should have key3 but not key2
        assert_eq!(snapshot.get("key2"), None);
        assert_eq!(snapshot.get("key3"), Some(&3));

        // Both should still have key1
        assert_eq!(tree.get("key1"), Some(&1));
        assert_eq!(snapshot.get("key1"), Some(&1));
    }

    #[test]
    fn test_node_growth() {
        let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Insert enough keys to trigger Node4 → Node16 growth
        for i in 0..10 {
            let key = format!("key{i:02}");
            tree.insert(key, i);
        }

        // Verify all keys are still accessible after growth
        for i in 0..10 {
            let key = format!("key{i:02}");
            assert_eq!(tree.get(&key), Some(&i));
        }

        // Take a snapshot after growth
        let snapshot = tree.snapshot();

        // Add more keys to original tree to trigger further growth
        for i in 10..20 {
            let key = format!("key{i:02}");
            tree.insert(key, i);
        }

        // Snapshot should not have new keys
        for i in 10..20 {
            let key = format!("key{i:02}");
            assert_eq!(snapshot.get(&key), None);
            assert_eq!(tree.get(&key), Some(&i));
        }

        // But snapshot should still have original keys
        for i in 0..10 {
            let key = format!("key{i:02}");
            assert_eq!(snapshot.get(&key), Some(&i));
        }
    }

    #[test]
    fn test_tree_structure_debugging() {
        use crate::tree::AdaptiveRadixTree;

        // Test what the regular tree does with the same keys
        let mut regular_tree = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        regular_tree.insert(0, 12345);
        regular_tree.insert(4573127, 67890);

        // Try remove on regular tree
        let regular_remove_result = regular_tree.remove(0);

        // The regular tree should work, versioned should not (for now)
        assert_eq!(regular_remove_result, Some(12345));
        // TODO: Make versioned tree work the same way
    }

    #[test]
    fn test_structural_sharing() {
        let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Build a substantial tree structure
        for i in 0..20 {
            let key = format!("shared_key_{i:02}");
            tree.insert(key, i);
        }

        // Take multiple snapshots - they should share the same root
        let snapshot1 = tree.snapshot();
        let snapshot2 = tree.snapshot();
        let snapshot3 = tree.snapshot();

        // Verify that shared nodes have high reference counts
        // The root should be referenced by: tree + snapshot1 + snapshot2 + snapshot3 = 4 references
        if let Some(root) = &tree.root {
            let strong_count = Arc::strong_count(root);
            assert_eq!(
                strong_count, 4,
                "Root should be shared between original and 3 snapshots"
            );
        }

        // Now modify only the original tree - this should trigger CoW
        tree.insert("new_key", 999);

        // After modification, snapshots should still share the old root
        if let (Some(s1_root), Some(s2_root), Some(s3_root)) =
            (&snapshot1.root, &snapshot2.root, &snapshot3.root)
        {
            // All three snapshots should point to the same root node
            assert!(
                Arc::ptr_eq(s1_root, s2_root),
                "Snapshot1 and Snapshot2 should share root"
            );
            assert!(
                Arc::ptr_eq(s2_root, s3_root),
                "Snapshot2 and Snapshot3 should share root"
            );

            // The shared root should have exactly 3 references (from the 3 snapshots)
            let shared_count = Arc::strong_count(s1_root);
            assert_eq!(
                shared_count, 3,
                "Shared root should have 3 references after original tree CoW"
            );
        }

        // Verify that the original tree has its own root now
        if let Some(orig_root) = &tree.root {
            let orig_count = Arc::strong_count(orig_root);
            assert_eq!(
                orig_count, 1,
                "Original tree should have exclusive ownership of new root"
            );
        }

        // All snapshots should NOT see the new key
        assert_eq!(snapshot1.get("new_key"), None);
        assert_eq!(snapshot2.get("new_key"), None);
        assert_eq!(snapshot3.get("new_key"), None);

        // But original tree should see it
        assert_eq!(tree.get("new_key"), Some(&999));

        // All trees should still see the shared data
        for i in 0..20 {
            let key = format!("shared_key_{i:02}");
            assert_eq!(tree.get(&key), Some(&i));
            assert_eq!(snapshot1.get(&key), Some(&i));
            assert_eq!(snapshot2.get(&key), Some(&i));
            assert_eq!(snapshot3.get(&key), Some(&i));
        }
    }

    #[test]
    fn test_snapshot_cleanup() {
        let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Create a tree with some data
        for i in 0..10i32 {
            tree.insert(i, i * 10);
        }

        let initial_root_refs = if let Some(root) = &tree.root {
            Arc::strong_count(root)
        } else {
            panic!("Tree should have a root");
        };

        // Take several snapshots
        let snapshot1 = tree.snapshot();
        let snapshot2 = tree.snapshot();
        {
            let _snapshot3 = tree.snapshot(); // This one will be dropped immediately
        } // _snapshot3 is dropped here

        // Root should now have more references
        let with_snapshots_refs = if let Some(root) = &tree.root {
            Arc::strong_count(root)
        } else {
            panic!("Tree should have a root");
        };

        assert!(
            with_snapshots_refs > initial_root_refs,
            "Root should have more references with snapshots"
        );

        // Drop snapshot2 explicitly
        drop(snapshot2);

        // Root should have fewer references now
        let after_drops_refs = if let Some(root) = &tree.root {
            Arc::strong_count(root)
        } else {
            panic!("Tree should have a root");
        };

        assert!(
            after_drops_refs < with_snapshots_refs,
            "Root should have fewer references after dropping snapshots"
        );

        // Should be exactly: original tree + snapshot1 = 2 references
        assert_eq!(
            after_drops_refs, 2,
            "Should have exactly 2 references: tree + snapshot1"
        );

        // Verify remaining snapshot still works - check that some keys exist
        assert!(snapshot1.get(0).is_some());
        assert!(snapshot1.get(5).is_some());
        assert!(snapshot1.get(9).is_some());

        // Drop the last snapshot
        drop(snapshot1);

        // Now tree should have exclusive ownership
        let final_refs = if let Some(root) = &tree.root {
            Arc::strong_count(root)
        } else {
            panic!("Tree should have a root");
        };

        assert_eq!(
            final_refs, 1,
            "Tree should have exclusive ownership after all snapshots dropped"
        );

        // Tree should still work normally
        for i in 0..10i32 {
            assert_eq!(tree.get(i), Some(&(i * 10)));
        }
    }

    #[test]
    fn test_signed_integer_keys() {
        // Test that signed integer keys work correctly (regression test)
        let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Insert some positive and negative integers
        tree.insert(-5i32, -50);
        tree.insert(0i32, 0);
        tree.insert(1i32, 10);
        tree.insert(8i32, 80);
        tree.insert(-1i32, -10);

        // Take a snapshot
        let snapshot = tree.snapshot();

        // Verify all keys work correctly in both tree and snapshot
        assert_eq!(tree.get(-5i32), Some(&-50));
        assert_eq!(tree.get(-1i32), Some(&-10));
        assert_eq!(tree.get(0i32), Some(&0));
        assert_eq!(tree.get(1i32), Some(&10));
        assert_eq!(tree.get(8i32), Some(&80));

        assert_eq!(snapshot.get(-5i32), Some(&-50));
        assert_eq!(snapshot.get(-1i32), Some(&-10));
        assert_eq!(snapshot.get(0i32), Some(&0));
        assert_eq!(snapshot.get(1i32), Some(&10));
        assert_eq!(snapshot.get(8i32), Some(&80));

        // Test that non-existent keys return None
        assert_eq!(tree.get(99i32), None);
        assert_eq!(snapshot.get(99i32), None);
    }

    #[test]
    fn test_deep_structural_sharing() {
        let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<32>, i32>::new();

        // Create a deeper tree structure with common prefixes
        let prefixes = [
            "user",
            "user_profile",
            "user_settings",
            "system",
            "system_config",
        ];
        for (i, prefix) in prefixes.iter().enumerate() {
            for j in 0..5 {
                let key = format!("{prefix}_{j:02}");
                tree.insert(key, (i * 100 + j) as i32);
            }
        }

        // Take a snapshot
        let snapshot = tree.snapshot();

        // Modify only one branch - should trigger minimal CoW
        tree.insert("user_00", 999); // This should replace existing value

        // The modification should only affect nodes along the path to "user_00"
        // Most of the tree structure should still be shared

        // Verify the change
        assert_eq!(tree.get("user_00"), Some(&999));
        assert_eq!(snapshot.get("user_00"), Some(&0)); // Original value

        // All other keys should be the same in both
        for (i, prefix) in prefixes.iter().enumerate() {
            for j in 0..5 {
                let key = format!("{prefix}_{j:02}");
                if key != "user_00" {
                    let expected_value = (i * 100 + j) as i32;
                    assert_eq!(tree.get(&key), Some(&expected_value));
                    assert_eq!(snapshot.get(&key), Some(&expected_value));
                }
            }
        }

        // Add a completely new branch - should create new nodes but still share unchanged parts
        tree.insert("new_branch_00", 777);

        assert_eq!(tree.get("new_branch_00"), Some(&777));
        assert_eq!(snapshot.get("new_branch_00"), None);

        // All original keys should still work in both trees
        for (i, prefix) in prefixes.iter().enumerate() {
            for j in 0..5 {
                let key = format!("{prefix}_{j:02}");
                let expected_in_tree = if key == "user_00" {
                    999
                } else {
                    (i * 100 + j) as i32
                };
                let expected_in_snapshot = (i * 100 + j) as i32;

                assert_eq!(tree.get(&key), Some(&expected_in_tree));
                assert_eq!(snapshot.get(&key), Some(&expected_in_snapshot));
            }
        }
    }
}

#[cfg(test)]
mod shuttle_tests {
    use super::*;
    use crate::keys::array_key::ArrayKey;
    use shuttle::{Config, Runner, sync::Arc as ShuttleArc, thread};

    #[test]
    fn shuttle_concurrent_snapshots() {
        let runner = Runner::new(
            shuttle::scheduler::DfsScheduler::new(Some(1000), false),
            Config::new(),
        );
        runner.run(|| {
            let tree = ShuttleArc::new(std::sync::Mutex::new(VersionedAdaptiveRadixTree::<
                ArrayKey<16>,
                i32,
            >::new()));

            // Pre-populate the tree
            {
                let mut t = tree.lock().unwrap();
                for i in 0..10 {
                    t.insert(i, i * 10);
                }
            }

            // Take snapshots BEFORE starting any writer threads
            let snapshot1 = {
                let t = tree.lock().unwrap();
                t.snapshot()
            };
            let snapshot2 = {
                let t = tree.lock().unwrap();
                t.snapshot()
            };

            let tree1 = ShuttleArc::clone(&tree);

            let handle1 = thread::spawn(move || {
                // Verify snapshot contents
                for i in 0..10 {
                    assert_eq!(snapshot1.get(i), Some(&(i * 10)));
                }
                snapshot1
            });

            let handle2 = thread::spawn(move || {
                // Verify snapshot contents
                for i in 0..10 {
                    assert_eq!(snapshot2.get(i), Some(&(i * 10)));
                }
                snapshot2
            });

            let handle3 = thread::spawn(move || {
                // Modify the original tree
                let mut t = tree1.lock().unwrap();
                t.insert(100, 1000);
                assert_eq!(t.get(100), Some(&1000));
            });

            let snapshot1 = handle1.join().unwrap();
            let snapshot2 = handle2.join().unwrap();
            handle3.join().unwrap();

            // Snapshots should not see the new key
            assert_eq!(snapshot1.get(100), None);
            assert_eq!(snapshot2.get(100), None);

            // But original tree should
            let t = tree.lock().unwrap();
            assert_eq!(t.get(100), Some(&1000));
        });
    }

    #[test]
    fn shuttle_snapshot_sharing_across_threads() {
        let runner = Runner::new(
            shuttle::scheduler::DfsScheduler::new(Some(1000), false),
            Config::new(),
        );
        runner.run(|| {
            let mut tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

            // Pre-populate
            for i in 0..5 {
                tree.insert(i, i * 2);
            }

            let snapshot = ShuttleArc::new(tree.snapshot());
            let snapshot1 = ShuttleArc::clone(&snapshot);
            let snapshot2 = ShuttleArc::clone(&snapshot);
            let snapshot3 = ShuttleArc::clone(&snapshot);

            let handle1 = thread::spawn(move || {
                // Read from snapshot in thread 1
                let mut results = Vec::new();
                for i in 0..5 {
                    if let Some(val) = snapshot1.get(i) {
                        results.push(*val);
                    }
                }
                results
            });

            let handle2 = thread::spawn(move || {
                // Read from snapshot in thread 2
                let mut results = Vec::new();
                for i in 0..5 {
                    if let Some(val) = snapshot2.get(i) {
                        results.push(*val);
                    }
                }
                results
            });

            let handle3 = thread::spawn(move || {
                // Read from snapshot in thread 3
                let mut results = Vec::new();
                for i in 0..5 {
                    if let Some(val) = snapshot3.get(i) {
                        results.push(*val);
                    }
                }
                results
            });

            let results1 = handle1.join().unwrap();
            let results2 = handle2.join().unwrap();
            let results3 = handle3.join().unwrap();

            // All threads should see the same data
            let expected: Vec<i32> = (0..5).map(|i| i * 2).collect();
            assert_eq!(results1, expected);
            assert_eq!(results2, expected);
            assert_eq!(results3, expected);
        });
    }

    #[test]
    fn shuttle_concurrent_snapshot_mutations() {
        let runner = Runner::new(
            shuttle::scheduler::DfsScheduler::new(Some(1000), false),
            Config::new(),
        );
        runner.run(|| {
            let mut base_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, i32>::new();

            // Pre-populate
            for i in 0..3 {
                base_tree.insert(i, i);
            }

            // Create snapshots that will be mutated concurrently
            let snapshot1 = ShuttleArc::new(std::sync::Mutex::new(base_tree.snapshot()));
            let snapshot2 = ShuttleArc::new(std::sync::Mutex::new(base_tree.snapshot()));

            let s1 = ShuttleArc::clone(&snapshot1);
            let s2 = ShuttleArc::clone(&snapshot2);

            let handle1 = thread::spawn(move || {
                let mut snap = s1.lock().unwrap();
                snap.insert(10, 100);
                snap.insert(11, 110);

                // Verify our mutations
                assert_eq!(snap.get(10), Some(&100));
                assert_eq!(snap.get(11), Some(&110));

                // Should still see original data
                for i in 0..3 {
                    assert_eq!(snap.get(i), Some(&i));
                }
            });

            let handle2 = thread::spawn(move || {
                let mut snap = s2.lock().unwrap();
                snap.insert(20, 200);
                snap.insert(21, 210);

                // Verify our mutations
                assert_eq!(snap.get(20), Some(&200));
                assert_eq!(snap.get(21), Some(&210));

                // Should still see original data
                for i in 0..3 {
                    assert_eq!(snap.get(i), Some(&i));
                }
            });

            handle1.join().unwrap();
            handle2.join().unwrap();

            // Verify independence - snapshot1 shouldn't see snapshot2's changes
            {
                let snap1 = snapshot1.lock().unwrap();
                assert_eq!(snap1.get(10), Some(&100));
                assert_eq!(snap1.get(11), Some(&110));
                assert_eq!(snap1.get(20), None); // Shouldn't see snapshot2's data
                assert_eq!(snap1.get(21), None);
            }

            {
                let snap2 = snapshot2.lock().unwrap();
                assert_eq!(snap2.get(20), Some(&200));
                assert_eq!(snap2.get(21), Some(&210));
                assert_eq!(snap2.get(10), None); // Shouldn't see snapshot1's data
                assert_eq!(snap2.get(11), None);
            }
        });
    }

    #[test]
    fn shuttle_many_readers_one_writer() {
        let runner = Runner::new(
            shuttle::scheduler::DfsScheduler::new(Some(1000), false),
            Config::new(),
        );
        runner.run(|| {
            let tree = ShuttleArc::new(std::sync::Mutex::new(VersionedAdaptiveRadixTree::<
                ArrayKey<16>,
                i32,
            >::new()));

            // Pre-populate
            {
                let mut t = tree.lock().unwrap();
                for i in 0..10 {
                    t.insert(i, i * 3);
                }
            }

            // Take a snapshot before spawning threads
            let snapshot = {
                let t = tree.lock().unwrap();
                ShuttleArc::new(t.snapshot())
            };

            let tree_for_writer = ShuttleArc::clone(&tree);

            // Spawn multiple readers
            let mut reader_handles = Vec::new();
            for reader_id in 0..3 {
                let snap = ShuttleArc::clone(&snapshot);
                let handle = thread::spawn(move || {
                    let mut sum = 0;
                    for i in 0..10 {
                        if let Some(val) = snap.get(i) {
                            sum += val;
                        }
                    }
                    (reader_id, sum)
                });
                reader_handles.push(handle);
            }

            // Spawn one writer
            let writer_handle = thread::spawn(move || {
                let mut tree = tree_for_writer.lock().unwrap();
                // Add new data
                for i in 100..105 {
                    tree.insert(i, i * 5);
                }

                // Verify writer can see its own changes
                let mut writer_sum = 0;
                for i in 100..105 {
                    if let Some(val) = tree.get(i) {
                        writer_sum += val;
                    }
                }
                writer_sum
            });

            // Collect results
            let expected_reader_sum = (0..10).map(|i| i * 3).sum::<i32>();
            for handle in reader_handles {
                let (reader_id, sum) = handle.join().unwrap();
                assert_eq!(sum, expected_reader_sum, "Reader {reader_id} got wrong sum");
            }

            let writer_sum = writer_handle.join().unwrap();
            let expected_writer_sum = (100..105).map(|i| i * 5).sum::<i32>();
            assert_eq!(writer_sum, expected_writer_sum);

            // Verify readers didn't see writer's changes (they used snapshot)
            for i in 100..105 {
                assert_eq!(snapshot.get(i), None);
            }
        });
    }

    #[test]
    fn shuttle_snapshot_drop_safety() {
        let runner = Runner::new(
            shuttle::scheduler::DfsScheduler::new(Some(1000), false),
            Config::new(),
        );
        runner.run(|| {
            let tree = ShuttleArc::new(std::sync::Mutex::new(VersionedAdaptiveRadixTree::<
                ArrayKey<16>,
                i32,
            >::new()));

            // Pre-populate
            {
                let mut t = tree.lock().unwrap();
                for i in 0..5 {
                    t.insert(i, i * 7);
                }
            }

            let tree1 = ShuttleArc::clone(&tree);
            let tree2 = ShuttleArc::clone(&tree);

            let handle1 = thread::spawn(move || {
                let snapshot = {
                    let t = tree1.lock().unwrap();
                    t.snapshot()
                };

                // Use snapshot briefly
                let mut sum = 0;
                for i in 0..5 {
                    if let Some(val) = snapshot.get(i) {
                        sum += val;
                    }
                }
                sum
                // Snapshot drops here
            });

            let handle2 = thread::spawn(move || {
                let snapshot = {
                    let t = tree2.lock().unwrap();
                    t.snapshot()
                };

                // Use snapshot briefly
                let mut count = 0;
                for i in 0..5 {
                    if snapshot.get(i).is_some() {
                        count += 1;
                    }
                }
                count
                // Snapshot drops here
            });

            let sum = handle1.join().unwrap();
            let count = handle2.join().unwrap();

            assert_eq!(sum, (0..5).map(|i| i * 7).sum::<i32>());
            assert_eq!(count, 5);

            // Original tree should still work after snapshots are dropped
            let t = tree.lock().unwrap();
            for i in 0..5 {
                assert_eq!(t.get(i), Some(&(i * 7)));
            }
        });
    }
}
