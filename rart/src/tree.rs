//! Adaptive Radix Tree implementation.
//!
//! This module contains the main [`AdaptiveRadixTree`] implementation and related
//! functionality for the RART crate.

use std::cmp::min;
use std::ops::RangeBounds;

use crate::iter::{LinkedIterator, ValuesIter};
use crate::keys::KeyTrait;
use crate::node::{Content, DefaultNode, LeafData, Node};
use crate::partials::Partial;
use crate::range::Range;
use crate::stats::{TreeStats, TreeStatsTrait, update_tree_stats};

/// An Adaptive Radix Tree (ART) - a high-performance, memory-efficient trie data structure.
///
/// The Adaptive Radix Tree automatically adjusts its internal representation based on the
/// number of children at each node, providing excellent performance characteristics for
/// a wide range of workloads.
///
/// ## Features
///
/// - **Adaptive nodes**: Uses different node types (4, 16, 48, 256 children) based on density
/// - **Space efficient**: Compact representation that minimizes memory usage
/// - **Cache friendly**: Optimized memory layout for modern CPU architectures
/// - **Fast operations**: O(k) complexity for basic operations where k is the key length
/// - **Range queries**: Efficient iteration over key ranges with proper ordering
///
/// ## Type Parameters
///
/// - `KeyType`: The type of keys stored in the tree, must implement [`KeyTrait`]
/// - `ValueType`: The type of values associated with keys
///
/// ## Examples
///
/// Basic usage with string keys:
///
/// ```rust
/// use rart::{AdaptiveRadixTree, ArrayKey};
///
/// let mut tree = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
///
/// // Insert some data
/// tree.insert("apple", "fruit".to_string());
/// tree.insert("application", "software".to_string());
///
/// // Query the tree
/// debug_assert_eq!(tree.get("apple"), Some(&"fruit".to_string()));
/// debug_assert_eq!(tree.get("orange"), None);
///
/// // Iterate over all entries
/// for (key, value) in tree.iter() {
///     println!("{:?} -> {}", key.as_ref(), value);
/// }
/// ```
///
/// Range queries:
///
/// ```rust
/// use rart::{AdaptiveRadixTree, ArrayKey};
///
/// let mut tree = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
/// tree.insert("apple", 1);
/// tree.insert("banana", 2);
/// tree.insert("cherry", 3);
///
/// // Get all keys starting with "a"
/// let start: ArrayKey<16> = "a".into();
/// let end: ArrayKey<16> = "b".into();
/// let a_keys: Vec<_> = tree.range(start..end).collect();
/// debug_assert_eq!(a_keys.len(), 1); // Just "apple"
/// ```
pub struct AdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
{
    root: Option<DefaultNode<KeyType::PartialType, ValueType>>,
    min_leaf: Option<*mut LeafData<ValueType>>,
    max_leaf: Option<*mut LeafData<ValueType>>,
    _phantom: std::marker::PhantomData<KeyType>,
}

impl<KeyType: KeyTrait, ValueType> Default for AdaptiveRadixTree<KeyType, ValueType> {
    fn default() -> Self {
        Self::new()
    }
}

impl<KeyType, ValueType> AdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
{
    /// Create a new empty Adaptive Radix Tree.
    pub fn new() -> Self {
        Self {
            root: None,
            min_leaf: None,
            max_leaf: None,
            _phantom: Default::default(),
        }
    }

    /// Create a new Adaptive Radix Tree with the given root node.
    /// This is primarily used for internal conversions.
    pub(crate) fn from_root(root: DefaultNode<KeyType::PartialType, ValueType>) -> Self {
        let mut tree = Self {
            root: Some(root),
            min_leaf: None,
            max_leaf: None,
            _phantom: Default::default(),
        };
        tree.rebuild_leaf_tracking();
        tree
    }

    /// Get a value by key (generic version).
    ///
    /// This method accepts any type that can be converted into the tree's key type.
    #[inline]
    pub fn get<Key>(&self, key: Key) -> Option<&ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_k(&key.into())
    }

    /// Get a value by key reference (direct version).
    ///
    /// This method works directly with key references for optimal performance.
    #[inline]
    pub fn get_k(&self, key: &KeyType) -> Option<&ValueType> {
        AdaptiveRadixTree::get_iterate(self.root.as_ref()?, key)
    }

    /// Get a mutable reference to a value by key (generic version).
    #[inline]
    pub fn get_mut<Key>(&mut self, key: Key) -> Option<&mut ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_mut_k(&key.into())
    }

    /// Get a mutable reference to a value by key reference (direct version).
    #[inline]
    pub fn get_mut_k(&mut self, key: &KeyType) -> Option<&mut ValueType> {
        AdaptiveRadixTree::get_iterate_mut(self.root.as_mut()?, key)
    }

    /// Insert a key-value pair (generic version).
    ///
    /// Follows standard Rust container conventions by returning the old value
    /// when a key is replaced.
    ///
    /// # Returns
    ///
    /// - `Some(old_value)` if a previous value was replaced
    /// - `None` if this was a new key
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rart::{AdaptiveRadixTree, ArrayKey};
    ///
    /// let mut tree = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
    ///
    /// // Insert new key returns None
    /// assert_eq!(tree.insert("key1", 100), None);
    ///
    /// // Insert same key returns old value
    /// assert_eq!(tree.insert("key1", 200), Some(100));
    /// assert_eq!(tree.get("key1"), Some(&200));
    /// ```
    #[inline]
    pub fn insert<KV>(&mut self, key: KV, value: ValueType) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.insert_k(&key.into(), value)
    }

    /// Insert a key-value pair using key reference (direct version).
    ///
    /// Follows standard Rust container conventions by returning the old value
    /// when a key is replaced.
    ///
    /// # Returns
    ///
    /// - `Some(old_value)` if a previous value was replaced
    /// - `None` if this was a new key
    #[inline]
    pub fn insert_k(&mut self, key: &KeyType, value: ValueType) -> Option<ValueType> {
        if self.root.is_none() {
            let mut new_root = DefaultNode::new_leaf(key.to_partial(0), value);
            self.insert_leaf_into_linked_list(&mut new_root, key);
            self.root = Some(new_root);
            return None;
        }

        // Safe to unwrap since we just checked it's Some
        let root = self.root.as_mut().unwrap();
        Self::insert_recurse_with_list(root, key, value, 0, &mut self.min_leaf, &mut self.max_leaf)
    }

    /// Remove a key-value pair (generic version).
    ///
    /// Returns the removed value if the key existed.
    pub fn remove<KV>(&mut self, key: KV) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.remove_k(&key.into())
    }

    /// Remove a key-value pair using key reference (direct version).
    ///
    /// Returns the removed value if the key existed.
    pub fn remove_k(&mut self, key: &KeyType) -> Option<ValueType> {
        self.root.as_ref()?;

        let root = self.root.as_mut().unwrap();

        // Don't bother doing anything if there's no prefix match on the root at all.
        let prefix_common_match = root.prefix.prefix_length_key(key, 0);
        if prefix_common_match != root.prefix.len() {
            return None;
        }

        // Special case, if the root is a leaf and matches the key, we can just remove it
        // immediately. If it doesn't match our key, then we have nothing to do here anyways.
        if root.is_leaf() {
            // Remove from linked list before taking the leaf
            if let Content::Leaf(ref mut leaf_data) = root.content {
                unsafe {
                    leaf_data.remove_from_list();
                }
            }

            // Clear min/max tracking since we're removing the only leaf
            self.min_leaf = None;
            self.max_leaf = None;

            // Move the value of the leaf in root. To do this, replace self.root  with None and
            // then unwrap the value out of the Option & Leaf.
            let stolen = self.root.take().unwrap();
            let leaf_data = match stolen.content {
                Content::Leaf(leaf_data) => leaf_data,
                _ => unreachable!(),
            };

            return Some(leaf_data.value);
        }

        let result = Self::remove_recurse_with_list_static(
            root,
            key,
            prefix_common_match,
            &mut self.min_leaf,
            &mut self.max_leaf,
        );

        // Prune root out if it's now empty.
        if root.is_inner() && root.num_children() == 0 {
            self.root = None;
        }
        result
    }

    /// Create an iterator over all key-value pairs in the tree.
    ///
    /// This iterator prioritizes performance and yields items in insertion order.
    /// For lexicographically sorted iteration, use `sorted_iter()`.
    pub fn iter(&self) -> impl Iterator<Item = (KeyType, &ValueType)> + '_ {
        // Use the O(1) LinkedIterator for best performance
        unsafe { LinkedIterator::new(self.min_leaf) }
    }

    /// Create an O(1) per-element iterator using the internal linked list.
    ///
    /// This is an alias for `iter()` and provides the same functionality.
    /// Kept for backwards compatibility.
    pub fn linked_iter(&self) -> impl Iterator<Item = (KeyType, &ValueType)> + '_ {
        // Direct LinkedIterator for best performance
        unsafe { LinkedIterator::new(self.min_leaf) }
    }

    /// Find the leaf node that would contain the given key or the next larger key.
    /// Returns a pointer to the leaf data for range query starting points.
    ///
    /// This performs O(log n) tree traversal to find the starting position for range queries.
    pub(crate) fn find_leaf_for_range_start(
        &self,
        key: &KeyType,
    ) -> Option<*mut LeafData<ValueType>> {
        let Some(root) = &self.root else {
            return None;
        };

        Self::find_leaf_for_key_or_next(root, key, 0)
    }

    /// Recursively find the leaf that contains the key or the next larger key.
    fn find_leaf_for_key_or_next(
        node: &DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        depth: usize,
    ) -> Option<*mut LeafData<ValueType>> {
        let longest_common_prefix = node.prefix.prefix_length_key(key, depth);

        // If we don't match the full prefix, we need to find where we diverge
        if longest_common_prefix < node.prefix.len() {
            let node_byte = node.prefix.at(longest_common_prefix);
            let key_byte = key.at(depth + longest_common_prefix);

            if key_byte < node_byte {
                // The key we're looking for is smaller than this node's prefix
                // This means this entire subtree is "too large", so we want the minimum leaf here
                return Some(Self::find_minimum_leaf_in_subtree(node));
            } else {
                // The key is larger than this node, we need to find the next larger subtree
                // This is complex and would require parent traversal - for now return None
                return None;
            }
        }

        // We match the full prefix, so continue into the node
        if node.is_leaf() {
            // Found a leaf - check if it's >= our target key
            let leaf_data = match &node.content {
                crate::node::Content::Leaf(leaf_data) => leaf_data.as_ref(),
                _ => unreachable!(),
            };
            let leaf_key = KeyType::new_from_slice(&leaf_data.key_bytes);
            if leaf_key >= *key {
                return Some(leaf_data as *const LeafData<ValueType> as *mut LeafData<ValueType>);
            } else {
                // This leaf is smaller than our target, need the next leaf in linked list
                return leaf_data.next;
            }
        }

        // Inner node - find the appropriate child
        let key_byte = key.at(depth + longest_common_prefix);

        // Try to find exact match first
        if let Some(child) = node.seek_child(key_byte) {
            return Self::find_leaf_for_key_or_next(child, key, depth + longest_common_prefix + 1);
        }

        // No exact match - find the next larger child
        for (child_key, child) in node.iter() {
            if child_key >= key_byte {
                return Some(Self::find_minimum_leaf_in_subtree(child));
            }
        }

        // No larger child found in this subtree
        None
    }

    /// Find the minimum leaf in a subtree.
    fn find_minimum_leaf_in_subtree(
        node: &DefaultNode<KeyType::PartialType, ValueType>,
    ) -> *mut LeafData<ValueType> {
        match &node.content {
            crate::node::Content::Leaf(leaf_data) => {
                leaf_data.as_ref() as *const LeafData<ValueType> as *mut LeafData<ValueType>
            }
            _ => {
                // Find the first (smallest) child
                if let Some((_, child)) = node.iter().next() {
                    return Self::find_minimum_leaf_in_subtree(child);
                }
                panic!("Inner node with no children");
            }
        }
    }


    /// Create an iterator over only the values in the tree.
    ///
    /// This iterator skips key reconstruction entirely and only yields values.
    /// It's more efficient when you don't need the keys.
    pub fn values_iter(&self) -> ValuesIter<'_, KeyType::PartialType, ValueType> {
        ValuesIter::new(self.root.as_ref())
    }

    /// Rebuild min/max leaf tracking by traversing the tree.
    /// This is used when creating a tree from a root node without tracking.
    fn rebuild_leaf_tracking(&mut self) {
        self.min_leaf = None;
        self.max_leaf = None;

        if let Some(root) = &self.root {
            Self::find_min_max_leaves_helper(root, &mut self.min_leaf, &mut self.max_leaf);
        }
    }

    /// Recursively find the minimum and maximum leaf nodes in the tree.
    fn find_min_max_leaves_helper(
        node: &DefaultNode<KeyType::PartialType, ValueType>,
        min_leaf: &mut Option<*mut LeafData<ValueType>>,
        max_leaf: &mut Option<*mut LeafData<ValueType>>,
    ) {
        match &node.content {
            Content::Leaf(leaf_data) => {
                let leaf_ptr =
                    leaf_data.as_ref() as *const LeafData<ValueType> as *mut LeafData<ValueType>;

                if min_leaf.is_none() {
                    *min_leaf = Some(leaf_ptr);
                }
                *max_leaf = Some(leaf_ptr);
            }
            _ => {
                // For inner nodes, traverse children in order
                for (_, child) in node.iter() {
                    Self::find_min_max_leaves_helper(child, min_leaf, max_leaf);
                }
            }
        }
    }

    /// Add a newly created leaf node to the linked list in the correct position.
    /// This maintains lexicographical ordering of the linked list.
    fn insert_leaf_into_linked_list(
        &mut self,
        new_leaf: &mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) {
        Self::insert_leaf_into_linked_list_helper(
            new_leaf,
            key,
            &mut self.min_leaf,
            &mut self.max_leaf,
        );
    }

    /// Static helper for inserting a leaf into the linked list in lexicographical order.
    fn insert_leaf_into_linked_list_helper(
        new_leaf: &mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        min_leaf: &mut Option<*mut LeafData<ValueType>>,
        max_leaf: &mut Option<*mut LeafData<ValueType>>,
    ) {
        if let Content::Leaf(ref mut new_leaf_data) = new_leaf.content {
            // Set the key bytes for the leaf
            new_leaf_data.set_key_bytes(key.as_ref().to_vec());
            let new_leaf_ptr = new_leaf_data.as_mut() as *mut LeafData<ValueType>;

            unsafe {
                // If this is the first leaf
                if min_leaf.is_none() {
                    *min_leaf = Some(new_leaf_ptr);
                    *max_leaf = Some(new_leaf_ptr);
                    return;
                }

                // Find the correct insertion position by comparing keys
                let new_key_bytes = key.as_ref();
                let mut current = *min_leaf;
                let mut previous = None;

                // Walk through the linked list to find insertion point
                while let Some(current_ptr) = current {
                    let current_leaf = &*current_ptr;
                    let current_key_bytes = &current_leaf.key_bytes;

                    // If new key is less than current key, insert before current
                    if new_key_bytes < current_key_bytes.as_slice() {
                        if let Some(prev_ptr) = previous {
                            // Insert between previous and current
                            (*new_leaf_data).insert_after(prev_ptr);
                        } else {
                            // Insert at beginning
                            (*new_leaf_data).insert_before(current_ptr);
                            *min_leaf = Some(new_leaf_ptr);
                        }
                        return;
                    }

                    previous = Some(current_ptr);
                    current = current_leaf.next;
                }

                // If we reach here, insert at the end
                if let Some(max_leaf_ptr) = *max_leaf {
                    (*new_leaf_data).insert_after(max_leaf_ptr);
                    *max_leaf = Some(new_leaf_ptr);
                }
            }
        }
    }

    /// Create an iterator over key-value pairs within a specified range.
    ///
    /// The range can be any type that implements `RangeBounds<KeyType>`.
    pub fn range<'a, R>(&'a self, range: R) -> Range<'a, KeyType, ValueType>
    where
        R: RangeBounds<KeyType> + 'a,
    {
        let Some(_) = &self.root else {
            return Range::empty();
        };

        let start_bound = range.start_bound().cloned();
        let end_bound = range.end_bound().cloned();

        // Use hybrid approach: tree seek + linked list iteration
        match start_bound {
            std::collections::Bound::Unbounded => {
                // No start bound, start from minimum leaf
                Range::for_linked_list(self.min_leaf, end_bound)
            }
            std::collections::Bound::Included(ref start_key) => {
                // Seek to start position using tree traversal, then use linked list
                let start_leaf = self.find_leaf_for_range_start(start_key);
                let effective_start = start_leaf.or(self.min_leaf);
                Range::for_linked_list_with_bounds(effective_start, start_bound, end_bound)
            }
            std::collections::Bound::Excluded(ref start_key) => {
                // Seek to start position using tree traversal, then use linked list
                let start_leaf = self.find_leaf_for_range_start(start_key);
                let effective_start = start_leaf.or(self.min_leaf);
                Range::for_linked_list_with_bounds(effective_start, start_bound, end_bound)
            }
        }
    }

    /// Check if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }
}

impl<KeyType, ValueType> TreeStatsTrait for AdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
{
    fn get_tree_stats(&self) -> TreeStats {
        let mut stats = TreeStats::default();

        if self.root.is_none() {
            return stats;
        }

        AdaptiveRadixTree::<KeyType, ValueType>::get_tree_stats_recurse(
            self.root.as_ref().unwrap(),
            &mut stats,
            1,
        );

        let total_inner_nodes = stats
            .node_stats
            .values()
            .map(|ns| ns.total_nodes)
            .sum::<usize>();
        let mut total_children = 0;
        let mut total_width = 0;
        for ns in stats.node_stats.values_mut() {
            total_children += ns.total_children;
            total_width += ns.width * ns.total_nodes;
            ns.density = ns.total_children as f64 / (ns.width * ns.total_nodes) as f64;
        }
        let total_density = total_children as f64 / total_width as f64;
        stats.num_inner_nodes = total_inner_nodes;
        stats.total_density = total_density;

        stats
    }
}

// Internals implementation
impl<KeyType, ValueType> AdaptiveRadixTree<KeyType, ValueType>
where
    KeyType: KeyTrait,
{
    fn get_iterate<'a>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
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
            cur_node = cur_node.seek_child(k)?
        }
    }

    fn get_iterate_mut<'a>(
        cur_node: &'a mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) -> Option<&'a mut ValueType> {
        let mut cur_node = cur_node;
        let mut depth = 0;
        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
            if prefix_common_match != cur_node.prefix.len() {
                return None;
            }

            if cur_node.prefix.len() == key.length_at(depth) {
                return cur_node.value_mut();
            }

            let k = key.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();
            cur_node = cur_node.seek_child_mut(k)?;
        }
    }

    fn insert_recurse_with_list(
        cur_node: &mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        value: ValueType,
        depth: usize,
        min_leaf: &mut Option<*mut LeafData<ValueType>>,
        max_leaf: &mut Option<*mut LeafData<ValueType>>,
    ) -> Option<ValueType> {
        let longest_common_prefix = cur_node.prefix.prefix_length_key(key, depth);

        let is_prefix_match =
            min(cur_node.prefix.len(), key.length_at(depth)) == longest_common_prefix;

        // Prefix fully covers this node.
        // Either sets the value or replaces the old value already here.
        if is_prefix_match
            && cur_node.prefix.len() == key.length_at(depth)
            && let Content::Leaf(leaf_data) = &mut cur_node.content
        {
            return Some(std::mem::replace(&mut leaf_data.value, value));
        }

        // Prefix is part of the current node, but doesn't fully cover it.
        // We have to break this node up, creating a new parent node, and a sibling for our leaf.
        if !is_prefix_match {
            let new_prefix = cur_node.prefix.partial_after(longest_common_prefix);
            let old_node_prefix = std::mem::replace(&mut cur_node.prefix, new_prefix);

            // We will replace this leaf node with a new inner node. The new value will join the
            // current node as sibling, both a child of the new node.
            let n4 = DefaultNode::new_inner(old_node_prefix.partial_before(longest_common_prefix));

            let k1 = old_node_prefix.at(longest_common_prefix);
            let k2 = key.at(depth + longest_common_prefix);

            let replacement_current = std::mem::replace(cur_node, n4);

            // We've deferred creating the leaf til now so that we can take ownership over the
            // key after other things are done peering at it.
            let mut new_leaf =
                DefaultNode::new_leaf(key.to_partial(depth + longest_common_prefix), value);
            Self::insert_leaf_into_linked_list_helper(&mut new_leaf, key, min_leaf, max_leaf);

            // Add the old leaf node as a child of the new inner node.
            cur_node.add_child(k1, replacement_current);
            cur_node.add_child(k2, new_leaf);

            return None;
        }

        // We must be an inner node, and either we need a new baby, or one of our children does, so
        // we'll hunt and see.
        let k = key.at(depth + longest_common_prefix);

        let Some(child) = cur_node.seek_child_mut(k) else {
            // We should not be a leaf at this point. If so, something bad has happened.
            debug_assert!(cur_node.is_inner());
            let mut new_leaf =
                DefaultNode::new_leaf(key.to_partial(depth + longest_common_prefix), value);
            Self::insert_leaf_into_linked_list_helper(&mut new_leaf, key, min_leaf, max_leaf);
            cur_node.add_child(k, new_leaf);
            return None;
        };

        Self::insert_recurse_with_list(
            child,
            key,
            value,
            depth + longest_common_prefix,
            min_leaf,
            max_leaf,
        )
    }

    fn remove_recurse_with_list_static(
        parent_node: &mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        depth: usize,
        min_leaf: &mut Option<*mut LeafData<ValueType>>,
        max_leaf: &mut Option<*mut LeafData<ValueType>>,
    ) -> Option<ValueType> {
        // Seek the child that matches the key at this depth, which is the first character at the
        // depth we're at.
        let c = key.at(depth);
        let child_node = parent_node.seek_child_mut(c)?;

        let prefix_common_match = child_node.prefix.prefix_length_key(key, depth);
        if prefix_common_match != child_node.prefix.len() {
            return None;
        }

        // If the child is a leaf, and the prefix matches the key, we can remove it from this parent
        // node. If the prefix does not match, then we have nothing to do here.
        if child_node.is_leaf() {
            if child_node.prefix.len() != (key.length_at(depth)) {
                return None;
            }

            // Remove from linked list before deleting from parent
            if let Content::Leaf(ref mut leaf_data) = child_node.content {
                let leaf_ptr = leaf_data.as_mut() as *mut LeafData<ValueType>;

                // Update min/max leaf tracking
                if Some(leaf_ptr) == *min_leaf {
                    *min_leaf = leaf_data.next;
                }
                if Some(leaf_ptr) == *max_leaf {
                    *max_leaf = leaf_data.prev;
                }

                unsafe {
                    leaf_data.remove_from_list();
                }
            }

            let node = parent_node.delete_child(c).unwrap();
            let leaf_data = match node.content {
                Content::Leaf(leaf_data) => leaf_data,
                _ => unreachable!(),
            };
            return Some(leaf_data.value);
        }

        // Otherwise, recurse down the branch in that direction.
        let result = Self::remove_recurse_with_list_static(
            child_node,
            key,
            depth + child_node.prefix.len(),
            min_leaf,
            max_leaf,
        );

        // If after this our child we just recursed into no longer has children of its own, it can
        // be collapsed into us. In this way we can prune the tree as we go.
        if result.is_some() && child_node.is_inner() && child_node.num_children() == 0 {
            let prefix = child_node.prefix.clone();
            let deleted = parent_node.delete_child(c).unwrap();
            debug_assert_eq!(prefix.to_slice(), deleted.prefix.to_slice());
        }

        result
    }

    fn get_tree_stats_recurse(
        node: &DefaultNode<KeyType::PartialType, ValueType>,
        tree_stats: &mut TreeStats,
        height: usize,
    ) {
        if height > tree_stats.max_height {
            tree_stats.max_height = height;
        }
        if node.value().is_some() {
            tree_stats.num_values += 1;
        }
        match node.content {
            Content::Leaf(_) => {
                tree_stats.num_leaves += 1;
            }
            _ => {
                update_tree_stats(tree_stats, node);
            }
        }
        for (_k, child) in node.iter() {
            AdaptiveRadixTree::<KeyType, ValueType>::get_tree_stats_recurse(
                child,
                tree_stats,
                height + 1,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, btree_map};
    use std::fmt::Debug;

    use rand::seq::SliceRandom;
    use rand::{Rng, rng};

    use crate::keys::KeyTrait;
    use crate::keys::array_key::ArrayKey;
    use crate::stats::TreeStatsTrait;
    use crate::tree;
    use crate::tree::AdaptiveRadixTree;

    #[test]
    fn test_root_set_get() {
        let mut q = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let key: ArrayKey<16> = "abc".into();
        assert!(q.insert("abc", 1).is_none());
        assert_eq!(q.get_k(&key), Some(&1));
    }

    #[test]
    fn test_string_keys_get_set() {
        let mut q = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        q.insert("abcd", 1);
        q.insert("abc", 2);
        q.insert("abcde", 3);
        q.insert("xyz", 4);
        q.insert("xyz", 5);
        q.insert("axyz", 6);
        q.insert("1245zzz", 6);

        assert_eq!(*q.get("abcd").unwrap(), 1);
        assert_eq!(*q.get("abc").unwrap(), 2);
        assert_eq!(*q.get("abcde").unwrap(), 3);
        assert_eq!(*q.get("axyz").unwrap(), 6);
        assert_eq!(*q.get("xyz").unwrap(), 5);

        assert_eq!(q.remove("abcde"), Some(3));
        assert_eq!(q.get("abcde"), None);
        assert_eq!(*q.get("abc").unwrap(), 2);
        assert_eq!(*q.get("axyz").unwrap(), 6);
        assert_eq!(q.remove("abc"), Some(2));
        assert_eq!(q.get("abc"), None);
    }

    #[test]
    fn test_int_keys_get_set() {
        let mut q = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        q.insert_k(&500i32.into(), 3);
        assert_eq!(q.get_k(&500i32.into()), Some(&3));
        q.insert_k(&666i32.into(), 2);
        assert_eq!(q.get_k(&666i32.into()), Some(&2));
        q.insert_k(&1i32.into(), 1);
        assert_eq!(q.get_k(&1i32.into()), Some(&1));
    }

    fn gen_random_string_keys<const S: usize>(
        l1_prefix: usize,
        l2_prefix: usize,
        suffix: usize,
    ) -> Vec<(ArrayKey<S>, String)> {
        let mut keys = Vec::new();
        let chars: Vec<char> = ('a'..='z').collect();
        for i in 0..chars.len() {
            let level1_prefix = chars[i].to_string().repeat(l1_prefix);
            for i in 0..chars.len() {
                let level2_prefix = chars[i].to_string().repeat(l2_prefix);
                let key_prefix = level1_prefix.clone() + &level2_prefix;
                for _ in 0..10 {
                    let suffix: String = (0..suffix)
                        .map(|_| chars[rng().random_range(0..chars.len())])
                        .collect();
                    let string = key_prefix.clone() + &suffix;
                    let k = string.clone().into();
                    keys.push((k, string));
                }
            }
        }

        keys.shuffle(&mut rng());
        keys
    }

    #[test]
    fn test_bulk_random_string_query() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, String>::new();
        let keys = gen_random_string_keys(3, 2, 3);
        let mut num_inserted = 0;
        for key in keys.iter() {
            if tree.insert_k(&key.0, key.1.clone()).is_none() {
                num_inserted += 1;
                assert!(tree.get_k(&key.0).is_some());
            }
        }
        let mut rng = rng();
        for _i in 0..10_000 {
            let entry = &keys[rng.random_range(0..keys.len())];
            let val = tree.get_k(&entry.0);
            debug_assert!(val.is_some());
            debug_assert_eq!(*val.unwrap(), entry.1);
        }

        let stats = tree.get_tree_stats();
        debug_assert_eq!(stats.num_values, num_inserted);
    }

    #[test]
    fn test_random_numeric_insert_get() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let count = 10_000;
        let mut rng = rng();
        let mut keys_inserted = vec![];
        for i in 0..count {
            let value = i;
            let rnd_key = rng.random_range(0..count);
            if tree.get(rnd_key).is_none() && tree.insert(rnd_key, value).is_none() {
                let result = tree.get(rnd_key);
                assert!(result.is_some());
                assert_eq!(*result.unwrap(), value);
                keys_inserted.push((rnd_key, value));
            }
        }

        let stats = tree.get_tree_stats();
        debug_assert_eq!(stats.num_values, keys_inserted.len());

        for (key, value) in &keys_inserted {
            let result = tree.get(key);
            debug_assert!(result.is_some(),);
            debug_assert_eq!(*result.unwrap(), *value,);
        }
    }

    #[test]
    fn test_iter() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let count = 100000;
        let mut rng = rng();
        let mut keys_inserted = BTreeSet::new();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.random_range(0..count);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            if tree.get_k(&rnd_key).is_none() && tree.insert_k(&rnd_key, rnd_val).is_none() {
                let result = tree.get_k(&rnd_key);
                assert!(result.is_some());
                assert_eq!(*result.unwrap(), rnd_val);
                keys_inserted.insert((rnd_val, rnd_val));
            }
        }

        // Iteration of keys_inserted and tree should be *roughly* the same, but the iteration order
        // within a KeyedMapping is not guaranteed to be lexicographical, so we can't compare
        // directly.
        let mut tree_iter = tree.iter();
        let keys_inserted_iter = keys_inserted.iter();
        for btree_entry in keys_inserted_iter {
            let art_entry = tree_iter.next();
            debug_assert!(art_entry.is_some());
            let art_entry = art_entry.unwrap();
            debug_assert_eq!(*art_entry.1, btree_entry.1);
            let art_key = art_entry.0.to_be_u64();
            debug_assert_eq!(art_key, btree_entry.0);
        }
    }

    #[test]
    fn test_iter_one_regression() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        tree.insert(123, 456);
        let mut iter = tree.iter();
        let result = iter.next().expect("Expected an entry");
        assert_eq!(result.1, &456)
    }

    #[test]
    // The following cases were found by fuzzing, and identified bugs in `remove`
    fn test_delete_regressions() {
        // DO_INSERT,12297829382473034287,72245244022401706
        // DO_INSERT,12297829382473034410,5425513372477729450
        // DO_DELETE,12297829382473056255,Some(5425513372477729450),None
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        assert!(
            tree.insert(12297829382473034287usize, 72245244022401706usize)
                .is_none()
        );
        assert!(
            tree.insert(12297829382473034410usize, 5425513372477729450usize)
                .is_none()
        );
        // assert!(tree.remove(&ArrayKey::new_from_unsigned(12297829382473056255usize)).is_none());

        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        // DO_INSERT,0,8101975729639522304
        // DO_INSERT,4934144,18374809624973934592
        // DO_DELETE,0,None,Some(8101975729639522304)
        assert!(tree.insert(0usize, 8101975729639522304usize).is_none());
        assert!(
            tree.insert(4934144usize, 18374809624973934592usize)
                .is_none()
        );
        assert_eq!(tree.get(0usize), Some(&8101975729639522304usize));
        assert_eq!(tree.remove(0usize), Some(8101975729639522304usize));
        assert_eq!(tree.get(4934144usize), Some(&18374809624973934592usize));

        // DO_INSERT,8102098874941833216,8101975729639522416
        // DO_INSERT,8102099357864587376,18374810107896688752
        // DO_DELETE,0,Some(8101975729639522416),None
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
        assert!(
            tree.insert(8102098874941833216usize, 8101975729639522416usize)
                .is_none()
        );
        assert!(
            tree.insert(8102099357864587376usize, 18374810107896688752usize)
                .is_none()
        );
        assert_eq!(tree.get(0usize), None);
        assert_eq!(tree.remove(0usize), None);
    }

    #[test]
    fn test_insert_returns_replaced_value() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        // Insert new key should return None
        assert_eq!(tree.insert("key1", 100), None);
        assert_eq!(tree.get("key1"), Some(&100));

        // Insert same key should return previous value
        assert_eq!(tree.insert("key1", 200), Some(100));
        assert_eq!(tree.get("key1"), Some(&200));

        // Insert same key again should return current value
        assert_eq!(tree.insert("key1", 300), Some(200));
        assert_eq!(tree.get("key1"), Some(&300));

        // Insert different key should return None
        assert_eq!(tree.insert("key2", 400), None);
        assert_eq!(tree.get("key2"), Some(&400));

        // Original key should still have latest value
        assert_eq!(tree.get("key1"), Some(&300));
    }

    #[test]
    fn test_delete() {
        // Insert a bunch of random keys and values into both a btree and our tree, then iterate
        // over the btree and delete the keys from our tree. Then, iterate over our tree and make
        // sure it's empty.
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let mut btree = BTreeMap::new();
        let count = 5_000;
        let mut rng = rng();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.random_range(0..u64::MAX);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            tree.insert_k(&rnd_key, rnd_val);
            btree.insert(rnd_val, rnd_val);
        }

        for (key, value) in btree.iter() {
            let key: ArrayKey<16> = (*key).into();
            let get_result = tree.get_k(&key);
            debug_assert_eq!(
                get_result.cloned(),
                Some(*value),
                "Key with prefix {:?} not found in tree; it should be",
                key.to_partial(0).to_slice()
            );
            let result = tree.remove_k(&key);
            debug_assert_eq!(result, Some(*value));
        }
    }
    // Compare the results of a range query on an AdaptiveRadixTree and a BTreeMap, because we can
    // safely assume the latter exhibits correct behavior.
    fn test_range_matches<'a, KeyType: KeyTrait, ValueType: PartialEq + Debug + 'a>(
        art_range: tree::Range<'a, KeyType, ValueType>,
        btree_range: btree_map::Range<'a, u64, ValueType>,
    ) {
        // collect both into vectors then compare
        let art_values = art_range.map(|(_, v)| v).collect::<Vec<_>>();
        let btree_values = btree_range.map(|(_, v)| v).collect::<Vec<_>>();

        debug_assert_eq!(art_values.len(), btree_values.len());
        debug_assert_eq!(art_values, btree_values);
    }

    #[test]
    fn test_range() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let count = 10000;
        let mut rng = rng();
        let mut keys_inserted = BTreeMap::new();
        for i in 0..count {
            let _value = i;
            let rnd_val = rng.random_range(0..count);
            let rnd_key: ArrayKey<16> = rnd_val.into();
            if tree.get_k(&rnd_key).is_none() && tree.insert_k(&rnd_key, rnd_val).is_none() {
                let result = tree.get_k(&rnd_key);
                assert!(result.is_some());
                assert_eq!(*result.unwrap(), rnd_val);
                keys_inserted.insert(rnd_val, rnd_val);
            }
        }

        // Test for range with unbounded start and exclusive end
        let end_key: ArrayKey<16> = 100u64.into();
        let t_r = tree.range(..end_key);
        let k_r = keys_inserted.range(..100);
        test_range_matches(t_r, k_r);

        // Test for range with unbounded start and inclusive end.
        let t_r = tree.range(..=end_key);
        let k_r = keys_inserted.range(..=100);
        test_range_matches(t_r, k_r);

        // Test for range with unbounded end and exclusive start
        let start_key: ArrayKey<16> = 100u64.into();
        let t_r = tree.range(start_key..);
        let k_r = keys_inserted.range(100..);
        test_range_matches(t_r, k_r);

        // Test for range with bounded start and end (exclusive)
        let end_key: ArrayKey<16> = 1000u64.into();
        let t_r = tree.range(start_key..end_key);
        let k_r = keys_inserted.range(100..1000);
        test_range_matches(t_r, k_r);

        // Test for range with bounded start and end (inclusive)
        let t_r = tree.range(start_key..=end_key);
        let k_r = keys_inserted.range(100..=1000);
        test_range_matches(t_r, k_r);
    }
}
