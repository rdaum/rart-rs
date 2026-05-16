//! Adaptive Radix Tree implementation.
//!
//! This module contains the main [`AdaptiveRadixTree`] implementation and related
//! functionality for the RART crate.

use std::cmp::Ordering;
use std::cmp::min;
use std::ops::RangeBounds;

use crate::VisitControl;
use crate::iter::{Iter, LendingIterInner, LendingKeyView, PrefixMatchIter, ValuesIter};
use crate::keys::KeyTrait;
use crate::node::{DefaultNode, Node};
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
    _phantom: std::marker::PhantomData<KeyType>,
}

type PrefixSubtreeView<'a, P, V> = (&'a DefaultNode<P, V>, Vec<&'a [u8]>, usize);

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
            _phantom: Default::default(),
        }
    }

    /// Build an Adaptive Radix Tree from already sorted key-value pairs.
    ///
    /// Duplicate keys must be adjacent because the input is sorted; the last value
    /// for a key wins. Panics if keys are not in nondecreasing order.
    pub fn bulk_load_sorted<I>(items: I) -> Self
    where
        I: IntoIterator<Item = (KeyType, ValueType)>,
    {
        let iter = items.into_iter();
        let (lower, _) = iter.size_hint();
        let mut unique: Vec<(KeyType, Option<ValueType>)> = Vec::with_capacity(lower);

        for (key, value) in iter {
            if let Some((last_key, last_value)) = unique.last_mut() {
                match Ord::cmp(&*last_key, &key) {
                    Ordering::Greater => panic!("bulk_load_sorted input is not sorted"),
                    Ordering::Equal => {
                        *last_value = Some(value);
                        continue;
                    }
                    Ordering::Less => {}
                }
            }

            unique.push((key, Some(value)));
        }

        Self::from_unique_sorted_items(unique)
    }

    /// Build an Adaptive Radix Tree from strictly sorted, unique key-value pairs.
    ///
    /// This is the fastest bulk-load entry point: it assumes the caller has
    /// already sorted and deduplicated the input. Debug builds check that the
    /// precondition holds; release builds skip that validation.
    pub fn bulk_load_sorted_unique<I>(items: I) -> Self
    where
        I: IntoIterator<Item = (KeyType, ValueType)>,
    {
        let iter = items.into_iter();
        let (lower, _) = iter.size_hint();
        let mut items = Vec::with_capacity(lower);
        for (key, value) in iter {
            items.push((key, Some(value)));
        }

        debug_assert!(
            items.windows(2).all(|window| window[0].0 < window[1].0),
            "bulk_load_sorted_unique input is not strictly sorted and unique"
        );

        Self::from_unique_sorted_items(items)
    }

    /// Build an Adaptive Radix Tree from indexed, strictly sorted, unique keys.
    ///
    /// This avoids staging keys and values inside the builder. `key_at` must
    /// provide random access to keys sorted in strict ascending order, and
    /// `take_value_at` is called exactly once for each index whose value is
    /// moved into the tree. Debug builds check the key ordering precondition;
    /// release builds skip that validation.
    pub fn bulk_load_sorted_unique_by_index<'a, KF, VF>(
        len: usize,
        key_at: KF,
        mut take_value_at: VF,
    ) -> Self
    where
        KeyType: 'a,
        KF: Fn(usize) -> &'a KeyType,
        VF: FnMut(usize) -> ValueType,
    {
        if len == 0 {
            return Self::new();
        }

        debug_assert!(
            (1..len).all(|index| key_at(index - 1) < key_at(index)),
            "bulk_load_sorted_unique_by_index input is not strictly sorted and unique"
        );

        Self::from_root(Self::build_bulk_node_by_index(
            0,
            len,
            0,
            &key_at,
            &mut take_value_at,
        ))
    }

    /// Create a new Adaptive Radix Tree with the given root node.
    /// This is primarily used for internal conversions.
    pub(crate) fn from_root(root: DefaultNode<KeyType::PartialType, ValueType>) -> Self {
        Self {
            root: Some(root),
            _phantom: Default::default(),
        }
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

    /// Return the deepest key/value pair whose key is a prefix of `key`.
    ///
    /// This differs from [`Self::get`] by allowing partial matches.
    #[inline]
    pub fn longest_prefix_match<Key>(&self, key: Key) -> Option<(KeyType, &ValueType)>
    where
        Key: Into<KeyType>,
    {
        self.longest_prefix_match_k(&key.into())
    }

    /// Return the deepest key/value pair whose key is a prefix of `key`.
    #[inline]
    pub fn longest_prefix_match_k(&self, key: &KeyType) -> Option<(KeyType, &ValueType)> {
        AdaptiveRadixTree::longest_prefix_match_iterate(self.root.as_ref()?, key)
    }

    /// Invoke `on_match` with the deepest key/value pair whose key is a prefix of `key`,
    /// using a lending borrowed key view for the matched key.
    #[inline]
    pub fn with_longest_prefix_match_view<Key, F>(&self, key: Key, on_match: F) -> bool
    where
        Key: Into<KeyType>,
        F: for<'view> FnOnce(LendingKeyView<'_, 'view>, &ValueType),
    {
        self.with_longest_prefix_match_view_k(&key.into(), on_match)
    }

    /// Invoke `on_match` with the deepest key/value pair whose key is a prefix of `key`,
    /// using a lending borrowed key view for the matched key.
    #[inline]
    pub fn with_longest_prefix_match_view_k<F>(&self, key: &KeyType, on_match: F) -> bool
    where
        F: for<'view> FnOnce(LendingKeyView<'_, 'view>, &ValueType),
    {
        let Some(root) = self.root.as_ref() else {
            return false;
        };
        AdaptiveRadixTree::longest_prefix_match_lending(root, key, on_match)
    }

    /// Iterate over stored key/value pairs whose keys are prefixes of `key`.
    ///
    /// Matches are yielded from shortest to longest. This differs from
    /// [`Self::prefix_iter`], which yields entries below a supplied prefix.
    #[inline]
    pub fn prefix_match_iter<Key>(
        &self,
        key: Key,
    ) -> PrefixMatchIter<'_, KeyType, KeyType::PartialType, ValueType>
    where
        Key: Into<KeyType>,
    {
        PrefixMatchIter::new(self.root.as_ref(), key.into())
    }

    /// Iterate over stored key/value pairs whose keys are prefixes of `key`.
    ///
    /// Matches are yielded from shortest to longest. This differs from
    /// [`Self::prefix_iter_k`], which yields entries below a supplied prefix.
    #[inline]
    pub fn prefix_match_iter_k(
        &self,
        key: &KeyType,
    ) -> PrefixMatchIter<'_, KeyType, KeyType::PartialType, ValueType> {
        PrefixMatchIter::new(self.root.as_ref(), key.clone())
    }

    /// Visit stored key/value pairs whose keys are prefixes of `key`.
    ///
    /// Matches are visited from shortest to longest. The key slice passed to
    /// the callback borrows from the supplied probe key, so the tree does not
    /// rebuild owned keys or maintain per-match key scratch state.
    #[inline]
    pub fn prefix_match_for_each<Key, F>(&self, key: Key, on_match: F)
    where
        Key: Into<KeyType>,
        F: FnMut(&[u8], &ValueType),
    {
        self.prefix_match_for_each_k(&key.into(), on_match)
    }

    /// Visit stored key/value pairs whose keys are prefixes of `key`.
    ///
    /// Matches are visited from shortest to longest. The key slice passed to
    /// the callback borrows from the supplied probe key, so the tree does not
    /// rebuild owned keys or maintain per-match key scratch state.
    #[inline]
    pub fn prefix_match_for_each_k<F>(&self, key: &KeyType, on_match: F)
    where
        F: FnMut(&[u8], &ValueType),
    {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        AdaptiveRadixTree::prefix_match_for_each_impl(root, key, on_match);
    }

    /// Iterate over all entries whose keys start with `prefix`.
    #[inline]
    pub fn prefix_iter<Key>(
        &self,
        prefix: Key,
    ) -> Iter<'_, KeyType, KeyType::PartialType, ValueType>
    where
        Key: Into<KeyType>,
    {
        self.prefix_iter_k(&prefix.into())
    }

    /// Iterate over all entries whose keys start with `prefix`.
    pub fn prefix_iter_k(
        &self,
        prefix: &KeyType,
    ) -> Iter<'_, KeyType, KeyType::PartialType, ValueType> {
        let Some(root) = self.root.as_ref() else {
            return Iter::new(None);
        };
        let Some((subtree_root, subtree_root_key)) =
            AdaptiveRadixTree::find_prefix_subtree(root, prefix)
        else {
            return Iter::new(None);
        };
        Iter::new_with_prefix(Some(subtree_root), subtree_root_key)
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
        let Some(root) = &mut self.root else {
            self.root = Some(DefaultNode::new_leaf(key.to_partial(0), value));
            return None;
        };

        AdaptiveRadixTree::insert_recurse(root, key, value, 0)
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
        let root = self.root.as_mut()?;

        // Don't bother doing anything if there's no prefix match on the root at all.
        let prefix_common_match = root.prefix.prefix_length_key(key, 0);
        if prefix_common_match != root.prefix.len() {
            return None;
        }

        if root.prefix.len() == key.length_at(0) {
            if root.is_leaf() {
                let stolen = self.root.take().unwrap();
                let leaf = stolen
                    .value
                    .expect("corruption: missing value at leaf root");
                return Some(leaf);
            }

            let removed = root.value.take();
            if root.num_children() == 0 {
                self.root = None;
            }
            return removed;
        }

        let result = AdaptiveRadixTree::remove_recurse(root, key, prefix_common_match);

        // Prune root out if it's now empty.
        if root.is_inner() && root.num_children() == 0 && root.value().is_none() {
            self.root = None;
        }
        result
    }

    /// Create an iterator over all key-value pairs in the tree.
    ///
    /// The iterator yields items in lexicographic order of the keys.
    pub fn iter(&self) -> Iter<'_, KeyType, KeyType::PartialType, ValueType> {
        Iter::new(self.root.as_ref())
    }

    /// Visit all key-value pairs using a lending borrowed key view.
    pub fn for_each_view<F>(&self, on_each: F)
    where
        F: for<'view> FnMut(LendingKeyView<'_, 'view>, &ValueType),
    {
        LendingIterInner::for_each(self.root.as_ref(), on_each);
    }

    /// Create an iterator over only the values in the tree.
    ///
    /// This iterator skips key reconstruction entirely and only yields values.
    /// It's more efficient when you don't need the keys.
    pub fn values_iter(&self) -> ValuesIter<'_, KeyType::PartialType, ValueType> {
        ValuesIter::new(self.root.as_ref())
    }

    /// Intersect two trees using ART-native node traversal.
    ///
    /// This avoids full key-stream materialization and instead walks both tries in lockstep,
    /// pruning mismatched prefixes early.
    pub fn intersect_with<'a, F>(&'a self, other: &'a Self, mut on_match: F)
    where
        F: FnMut(KeyType, &'a ValueType, &'a ValueType),
    {
        let (Some(left_root), Some(right_root)) = (self.root.as_ref(), other.root.as_ref()) else {
            return;
        };

        let mut key_buf = Vec::with_capacity(KeyType::MAXIMUM_SIZE.unwrap_or(64));
        Self::intersect_nodes(left_root, 0, right_root, 0, &mut key_buf, &mut on_match);
    }

    /// Intersect two trees using ART-native traversal and yield lending key views.
    pub fn intersect_lending_with<'a, F>(&'a self, other: &'a Self, mut on_match: F)
    where
        F: for<'view> FnMut(LendingKeyView<'a, 'view>, &'a ValueType, &'a ValueType),
    {
        let (Some(left_root), Some(right_root)) = (self.root.as_ref(), other.root.as_ref()) else {
            return;
        };

        let mut segments = Vec::new();
        let mut key_len = 0usize;
        Self::intersect_nodes_lending(
            left_root,
            0,
            right_root,
            0,
            &mut segments,
            &mut key_len,
            &mut on_match,
        );
    }

    /// Intersect two trees and invoke a callback with value pairs only.
    ///
    /// This avoids key materialization and is useful when only the joined values are needed.
    pub fn intersect_values_with<'a, F>(&'a self, other: &'a Self, mut on_match: F)
    where
        F: FnMut(&'a ValueType, &'a ValueType),
    {
        let (Some(left_root), Some(right_root)) = (self.root.as_ref(), other.root.as_ref()) else {
            return;
        };

        Self::intersect_nodes_values(left_root, 0, right_root, 0, &mut on_match);
    }

    /// Count the number of keys that exist in both trees.
    pub fn intersect_count(&self, other: &Self) -> usize {
        let mut count = 0usize;
        self.intersect_values_with(other, |_left_value, _right_value| {
            count += 1;
        });
        count
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

        // Use optimized O(log n) iteration for start bound
        match start_bound {
            std::collections::Bound::Unbounded => {
                // No start bound, use regular iterator
                let iter = self.iter();
                Range::for_iter(iter, end_bound)
            }
            _ => {
                // Use optimized start bound iteration
                let optimized_iter = Iter::new_with_start_bound(self.root.as_ref(), start_bound);
                Range::for_iter(optimized_iter, end_bound)
            }
        }
    }

    /// Visit all entries whose keys start with `prefix` using a lending borrowed key view.
    pub fn prefix_for_each_view<Key, F>(&self, prefix: Key, on_each: F)
    where
        Key: Into<KeyType>,
        F: for<'view> FnMut(LendingKeyView<'_, 'view>, &ValueType),
    {
        self.prefix_for_each_view_k(&prefix.into(), on_each)
    }

    /// Visit all entries whose keys start with `prefix` using a lending borrowed key view.
    pub fn prefix_for_each_view_k<F>(&self, prefix: &KeyType, on_each: F)
    where
        F: for<'view> FnMut(LendingKeyView<'_, 'view>, &ValueType),
    {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        let Some((subtree_root, subtree_root_segments, subtree_root_len)) =
            AdaptiveRadixTree::find_prefix_subtree_view(root, prefix)
        else {
            return;
        };
        LendingIterInner::for_each_with_prefix(
            Some(subtree_root),
            subtree_root_segments,
            subtree_root_len,
            on_each,
        );
    }

    /// Visit only values whose keys start with `prefix`.
    ///
    /// This avoids owned key reconstruction and lending key-view construction for
    /// each visited entry.
    #[inline]
    pub fn prefix_values_for_each<Key, F>(&self, prefix: Key, on_each: F)
    where
        Key: Into<KeyType>,
        F: FnMut(&ValueType),
    {
        self.prefix_values_for_each_k(&prefix.into(), on_each)
    }

    /// Visit only values whose keys start with `prefix`.
    ///
    /// This avoids owned key reconstruction and lending key-view construction for
    /// each visited entry.
    pub fn prefix_values_for_each_k<F>(&self, prefix: &KeyType, mut on_each: F)
    where
        F: FnMut(&ValueType),
    {
        let result: Result<(), std::convert::Infallible> =
            self.try_prefix_values_for_each_k(prefix, |value| {
                on_each(value);
                Ok(VisitControl::Continue)
            });
        match result {
            Ok(()) => {}
            Err(never) => match never {},
        }
    }

    /// Fallibly visit only values whose keys start with `prefix`.
    ///
    /// Returning [`VisitControl::Stop`] stops traversal immediately. Returning
    /// `Err` propagates that error without visiting more entries.
    #[inline]
    pub fn try_prefix_values_for_each<Key, E, F>(&self, prefix: Key, on_each: F) -> Result<(), E>
    where
        Key: Into<KeyType>,
        F: FnMut(&ValueType) -> Result<VisitControl, E>,
    {
        self.try_prefix_values_for_each_k(&prefix.into(), on_each)
    }

    /// Fallibly visit only values whose keys start with `prefix`.
    ///
    /// Returning [`VisitControl::Stop`] stops traversal immediately. Returning
    /// `Err` propagates that error without visiting more entries.
    pub fn try_prefix_values_for_each_k<E, F>(
        &self,
        prefix: &KeyType,
        mut on_each: F,
    ) -> Result<(), E>
    where
        F: FnMut(&ValueType) -> Result<VisitControl, E>,
    {
        let Some(root) = self.root.as_ref() else {
            return Ok(());
        };
        let Some(subtree_root) = AdaptiveRadixTree::find_prefix_subtree_node(root, prefix) else {
            return Ok(());
        };

        for value in ValuesIter::new(Some(subtree_root)) {
            if on_each(value)? == VisitControl::Stop {
                break;
            }
        }

        Ok(())
    }

    /// Visit key-value pairs within a specified range using a lending borrowed key view.
    pub fn for_each_range_view<R, F>(&self, range: R, on_each: F)
    where
        R: RangeBounds<KeyType>,
        F: for<'view> FnMut(LendingKeyView<'_, 'view>, &ValueType),
    {
        let Some(_) = &self.root else {
            return;
        };

        let start_bound = range.start_bound().cloned();
        let end_bound = range.end_bound().cloned();
        LendingIterInner::for_each_with_bounds(self.root.as_ref(), start_bound, end_bound, on_each);
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
    fn from_unique_sorted_items(mut items: Vec<(KeyType, Option<ValueType>)>) -> Self {
        if items.is_empty() {
            return Self::new();
        }

        Self::from_root(Self::build_bulk_node(&mut items, 0))
    }

    fn build_bulk_node(
        items: &mut [(KeyType, Option<ValueType>)],
        depth: usize,
    ) -> DefaultNode<KeyType::PartialType, ValueType> {
        debug_assert!(!items.is_empty());

        if items.len() == 1 {
            let (key, value) = &mut items[0];
            return DefaultNode::new_leaf(
                key.to_partial(depth),
                value.take().expect("bulk-load value already consumed"),
            );
        }

        let prefix_len = Self::common_prefix_len(&items[0].0, &items[items.len() - 1].0, depth);
        let node_depth = depth + prefix_len;
        let prefix = items[0].0.to_partial(depth).partial_before(prefix_len);
        let has_value = items[0].0.length_at(depth) == prefix_len;
        let first_child = usize::from(has_value);
        let child_count = Self::child_group_count(&items[first_child..], node_depth);

        if child_count == 0 {
            debug_assert!(has_value);
            return DefaultNode::new_leaf(
                prefix,
                items[0].1.take().expect("bulk-load value already consumed"),
            );
        }

        let mut node = match child_count {
            0..=4 => DefaultNode::new_4(prefix),
            5..=16 => DefaultNode::new_16(prefix),
            17..=48 => DefaultNode::new_48(prefix),
            _ => DefaultNode::new_256(prefix),
        };

        if has_value {
            node.value = Some(items[0].1.take().expect("bulk-load value already consumed"));
        }

        let mut start = first_child;
        while start < items.len() {
            debug_assert!(items[start].0.length_at(0) > node_depth);
            let edge = items[start].0.at(node_depth);
            let mut end = start + 1;
            while end < items.len() && items[end].0.at(node_depth) == edge {
                end += 1;
            }

            let child = Self::build_bulk_node(&mut items[start..end], node_depth);
            node.add_child_sorted_unchecked(edge, child);
            start = end;
        }

        node
    }

    fn build_bulk_node_by_index<'a, KF, VF>(
        start: usize,
        end: usize,
        depth: usize,
        key_at: &KF,
        take_value_at: &mut VF,
    ) -> DefaultNode<KeyType::PartialType, ValueType>
    where
        KeyType: 'a,
        KF: Fn(usize) -> &'a KeyType,
        VF: FnMut(usize) -> ValueType,
    {
        debug_assert!(start < end);

        if end - start == 1 {
            let key = key_at(start);
            return DefaultNode::new_leaf(key.to_partial(depth), take_value_at(start));
        }

        let first_key = key_at(start);
        let prefix_len = Self::common_prefix_len(first_key, key_at(end - 1), depth);
        let node_depth = depth + prefix_len;
        let prefix = first_key.to_partial(depth).partial_before(prefix_len);
        let has_value = first_key.length_at(depth) == prefix_len;
        let first_child = start + usize::from(has_value);
        let child_count = Self::child_group_count_by_index(first_child, end, node_depth, key_at);

        if child_count == 0 {
            debug_assert!(has_value);
            return DefaultNode::new_leaf(prefix, take_value_at(start));
        }

        let mut node = match child_count {
            0..=4 => DefaultNode::new_4(prefix),
            5..=16 => DefaultNode::new_16(prefix),
            17..=48 => DefaultNode::new_48(prefix),
            _ => DefaultNode::new_256(prefix),
        };

        if has_value {
            node.value = Some(take_value_at(start));
        }

        let mut child_start = first_child;
        while child_start < end {
            let child_key = key_at(child_start);
            debug_assert!(child_key.length_at(0) > node_depth);
            let edge = child_key.at(node_depth);
            let mut child_end = child_start + 1;
            while child_end < end && key_at(child_end).at(node_depth) == edge {
                child_end += 1;
            }

            let child = Self::build_bulk_node_by_index(
                child_start,
                child_end,
                node_depth,
                key_at,
                take_value_at,
            );
            node.add_child_sorted_unchecked(edge, child);
            child_start = child_end;
        }

        node
    }

    fn common_prefix_len(left: &KeyType, right: &KeyType, depth: usize) -> usize {
        let common_len = left.length_at(depth).min(right.length_at(depth));
        let mut matched = 0;
        while matched < common_len && left.at(depth + matched) == right.at(depth + matched) {
            matched += 1;
        }
        matched
    }

    fn child_group_count(items: &[(KeyType, Option<ValueType>)], depth: usize) -> usize {
        let mut count = 0;
        let mut previous = None;
        for (key, _) in items {
            debug_assert!(key.length_at(0) > depth);
            let edge = key.at(depth);
            if previous != Some(edge) {
                count += 1;
                previous = Some(edge);
            }
        }
        count
    }

    fn child_group_count_by_index<'a, KF>(
        start: usize,
        end: usize,
        depth: usize,
        key_at: &KF,
    ) -> usize
    where
        KeyType: 'a,
        KF: Fn(usize) -> &'a KeyType,
    {
        let mut count = 0;
        let mut previous = None;
        for index in start..end {
            let key = key_at(index);
            debug_assert!(key.length_at(0) > depth);
            let edge = key.at(depth);
            if previous != Some(edge) {
                count += 1;
                previous = Some(edge);
            }
        }
        count
    }

    fn get_iterate<'a>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) -> Option<&'a ValueType> {
        let mut cur_node = cur_node;
        let key_bytes = key.as_ref();
        let mut depth = 0;
        loop {
            let prefix_len = cur_node.prefix.len();
            let remaining_len = key_bytes.len() - depth;
            let prefix_common_match = cur_node.prefix.prefix_length_slice(&key_bytes[depth..]);
            if prefix_common_match != prefix_len {
                return None;
            }

            if prefix_len == remaining_len {
                return cur_node.value();
            }
            let k = key_bytes[depth + prefix_len];
            depth += prefix_len;
            cur_node = cur_node.seek_child(k)?
        }
    }

    fn longest_prefix_match_iterate<'a>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) -> Option<(KeyType, &'a ValueType)> {
        let mut cur_node = cur_node;
        let mut cur_key = cur_node.prefix.as_ref().to_vec();
        let mut best_match = None;
        let mut depth = 0;

        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
            if prefix_common_match != cur_node.prefix.len() {
                return best_match;
            }

            if let Some(value) = cur_node.value() {
                best_match = Some((KeyType::new_from_slice(&cur_key), value));
            }

            if cur_node.prefix.len() == key.length_at(depth) {
                return best_match;
            }

            let k = key.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();

            let Some(child) = cur_node.seek_child(k) else {
                return best_match;
            };
            cur_node = child;
            cur_key.extend_from_slice(cur_node.prefix.as_ref());
        }
    }

    fn longest_prefix_match_lending<'a, F>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        on_match: F,
    ) -> bool
    where
        F: for<'view> FnOnce(LendingKeyView<'a, 'view>, &'a ValueType),
    {
        let mut cur_node = cur_node;
        let mut cur_segments = if cur_node.prefix.is_empty() {
            Vec::new()
        } else {
            vec![cur_node.prefix.as_ref()]
        };
        let mut cur_len = cur_node.prefix.len();
        let mut best_match = None::<(usize, usize, &'a ValueType)>;
        let mut depth = 0;

        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
            if prefix_common_match != cur_node.prefix.len() {
                break;
            }

            if let Some(value) = cur_node.value() {
                best_match = Some((cur_segments.len(), cur_len, value));
            }

            if cur_node.prefix.len() == key.length_at(depth) {
                break;
            }

            let k = key.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();

            let Some(child) = cur_node.seek_child(k) else {
                break;
            };
            cur_node = child;
            let segment = cur_node.prefix.as_ref();
            if !segment.is_empty() {
                cur_segments.push(segment);
                cur_len += segment.len();
            }
        }

        if let Some((best_segment_count, best_len, value)) = best_match {
            on_match(
                LendingKeyView::new(&cur_segments[..best_segment_count], best_len),
                value,
            );
            return true;
        }

        false
    }

    fn prefix_match_for_each_impl<'a, F>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        mut on_match: F,
    ) where
        F: FnMut(&[u8], &'a ValueType),
    {
        let mut cur_node = cur_node;
        let mut depth = 0;
        let key_bytes = key.as_ref();

        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(key, depth);
            if prefix_common_match != cur_node.prefix.len() {
                return;
            }

            let matched_len = depth + cur_node.prefix.len();
            if let Some(value) = cur_node.value() {
                on_match(&key_bytes[..matched_len], value);
            }

            if cur_node.prefix.len() == key.length_at(depth) {
                return;
            }

            let k = key.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();

            let Some(child) = cur_node.seek_child(k) else {
                return;
            };
            cur_node = child;
        }
    }

    fn find_prefix_subtree<'a>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
        prefix: &KeyType,
    ) -> Option<(&'a DefaultNode<KeyType::PartialType, ValueType>, KeyType)> {
        let mut cur_node = cur_node;
        let mut cur_key = cur_node.prefix.as_ref().to_vec();
        let mut depth = 0;

        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(prefix, depth);
            if prefix_common_match != cur_node.prefix.len() {
                if prefix_common_match == prefix.length_at(depth) {
                    return Some((cur_node, KeyType::new_from_slice(&cur_key)));
                }
                return None;
            }

            if cur_node.prefix.len() == prefix.length_at(depth) {
                return Some((cur_node, KeyType::new_from_slice(&cur_key)));
            }

            let k = prefix.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();

            let child = cur_node.seek_child(k)?;
            cur_node = child;
            cur_key.extend_from_slice(cur_node.prefix.as_ref());
        }
    }

    fn find_prefix_subtree_node<'a>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
        prefix: &KeyType,
    ) -> Option<&'a DefaultNode<KeyType::PartialType, ValueType>> {
        let mut cur_node = cur_node;
        let mut depth = 0;

        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(prefix, depth);
            if prefix_common_match != cur_node.prefix.len() {
                if prefix_common_match == prefix.length_at(depth) {
                    return Some(cur_node);
                }
                return None;
            }

            if cur_node.prefix.len() == prefix.length_at(depth) {
                return Some(cur_node);
            }

            let k = prefix.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();

            cur_node = cur_node.seek_child(k)?;
        }
    }

    fn find_prefix_subtree_view<'a>(
        cur_node: &'a DefaultNode<KeyType::PartialType, ValueType>,
        prefix: &KeyType,
    ) -> Option<PrefixSubtreeView<'a, KeyType::PartialType, ValueType>> {
        let mut cur_node = cur_node;
        let mut cur_segments = if cur_node.prefix.is_empty() {
            Vec::new()
        } else {
            vec![cur_node.prefix.as_ref()]
        };
        let mut cur_len = cur_node.prefix.len();
        let mut depth = 0;

        loop {
            let prefix_common_match = cur_node.prefix.prefix_length_key(prefix, depth);
            if prefix_common_match != cur_node.prefix.len() {
                if prefix_common_match == prefix.length_at(depth) {
                    return Some((cur_node, cur_segments, cur_len));
                }
                return None;
            }

            if cur_node.prefix.len() == prefix.length_at(depth) {
                return Some((cur_node, cur_segments, cur_len));
            }

            let k = prefix.at(depth + cur_node.prefix.len());
            depth += cur_node.prefix.len();

            let child = cur_node.seek_child(k)?;
            cur_node = child;
            let segment = cur_node.prefix.as_ref();
            if !segment.is_empty() {
                cur_segments.push(segment);
                cur_len += segment.len();
            }
        }
    }

    /// Recursively intersect two nodes, supporting different prefix-compression boundaries
    /// through in-prefix offsets.
    fn intersect_nodes<'a, F>(
        left: &'a DefaultNode<KeyType::PartialType, ValueType>,
        mut left_offset: usize,
        right: &'a DefaultNode<KeyType::PartialType, ValueType>,
        mut right_offset: usize,
        key_buf: &mut Vec<u8>,
        on_match: &mut F,
    ) where
        F: FnMut(KeyType, &'a ValueType, &'a ValueType),
    {
        let restore_len = key_buf.len();
        let left_prefix = left.prefix.as_ref();
        let right_prefix = right.prefix.as_ref();

        while left_offset < left_prefix.len() && right_offset < right_prefix.len() {
            let left_byte = left_prefix[left_offset];
            let right_byte = right_prefix[right_offset];
            if left_byte != right_byte {
                key_buf.truncate(restore_len);
                return;
            }
            key_buf.push(left_byte);
            left_offset += 1;
            right_offset += 1;
        }

        // The remaining byte in the longer prefix must transition through a matching
        // child edge in the shorter side to continue.
        if left_offset < left_prefix.len() {
            if !right.is_inner() {
                key_buf.truncate(restore_len);
                return;
            }

            let edge = left_prefix[left_offset];
            let Some(right_child) = right.seek_child(edge) else {
                key_buf.truncate(restore_len);
                return;
            };

            key_buf.push(edge);
            Self::intersect_nodes(left, left_offset + 1, right_child, 1, key_buf, on_match);
            key_buf.truncate(restore_len);
            return;
        }

        if right_offset < right_prefix.len() {
            if !left.is_inner() {
                key_buf.truncate(restore_len);
                return;
            }

            let edge = right_prefix[right_offset];
            let Some(left_child) = left.seek_child(edge) else {
                key_buf.truncate(restore_len);
                return;
            };

            key_buf.push(edge);
            Self::intersect_nodes(left_child, 1, right, right_offset + 1, key_buf, on_match);
            key_buf.truncate(restore_len);
            return;
        }

        if let (Some(left_value), Some(right_value)) = (left.value(), right.value()) {
            on_match(
                KeyType::new_from_slice(key_buf.as_slice()),
                left_value,
                right_value,
            );
        }

        if left.is_inner() && right.is_inner() {
            if left.num_children() <= right.num_children() {
                for (edge, left_child) in left.iter() {
                    if let Some(right_child) = right.seek_child(edge) {
                        Self::intersect_nodes(left_child, 0, right_child, 0, key_buf, on_match);
                    }
                }
            } else {
                for (edge, right_child) in right.iter() {
                    if let Some(left_child) = left.seek_child(edge) {
                        Self::intersect_nodes(left_child, 0, right_child, 0, key_buf, on_match);
                    }
                }
            }
        }

        key_buf.truncate(restore_len);
    }

    fn intersect_nodes_lending<'a, F>(
        left: &'a DefaultNode<KeyType::PartialType, ValueType>,
        mut left_offset: usize,
        right: &'a DefaultNode<KeyType::PartialType, ValueType>,
        mut right_offset: usize,
        key_segments: &mut Vec<&'a [u8]>,
        key_len: &mut usize,
        on_match: &mut F,
    ) where
        F: for<'view> FnMut(LendingKeyView<'a, 'view>, &'a ValueType, &'a ValueType),
    {
        let restore_segments = key_segments.len();
        let restore_len = *key_len;
        let left_prefix = left.prefix.as_ref();
        let right_prefix = right.prefix.as_ref();
        let matched_left_start = left_offset;

        while left_offset < left_prefix.len() && right_offset < right_prefix.len() {
            if left_prefix[left_offset] != right_prefix[right_offset] {
                key_segments.truncate(restore_segments);
                *key_len = restore_len;
                return;
            }
            left_offset += 1;
            right_offset += 1;
        }

        if left_offset > matched_left_start {
            let matched = &left_prefix[matched_left_start..left_offset];
            key_segments.push(matched);
            *key_len += matched.len();
        }

        if left_offset < left_prefix.len() {
            if !right.is_inner() {
                key_segments.truncate(restore_segments);
                *key_len = restore_len;
                return;
            }

            let edge = left_prefix[left_offset];
            let Some(right_child) = right.seek_child(edge) else {
                key_segments.truncate(restore_segments);
                *key_len = restore_len;
                return;
            };

            let edge_segment = &left_prefix[left_offset..left_offset + 1];
            key_segments.push(edge_segment);
            *key_len += 1;
            Self::intersect_nodes_lending(
                left,
                left_offset + 1,
                right_child,
                1,
                key_segments,
                key_len,
                on_match,
            );
            key_segments.truncate(restore_segments);
            *key_len = restore_len;
            return;
        }

        if right_offset < right_prefix.len() {
            if !left.is_inner() {
                key_segments.truncate(restore_segments);
                *key_len = restore_len;
                return;
            }

            let edge = right_prefix[right_offset];
            let Some(left_child) = left.seek_child(edge) else {
                key_segments.truncate(restore_segments);
                *key_len = restore_len;
                return;
            };

            let edge_segment = &right_prefix[right_offset..right_offset + 1];
            key_segments.push(edge_segment);
            *key_len += 1;
            Self::intersect_nodes_lending(
                left_child,
                1,
                right,
                right_offset + 1,
                key_segments,
                key_len,
                on_match,
            );
            key_segments.truncate(restore_segments);
            *key_len = restore_len;
            return;
        }

        if let (Some(left_value), Some(right_value)) = (left.value(), right.value()) {
            on_match(
                LendingKeyView::new(key_segments, *key_len),
                left_value,
                right_value,
            );
        }

        if left.is_inner() && right.is_inner() {
            if left.num_children() <= right.num_children() {
                for (edge, left_child) in left.iter() {
                    if let Some(right_child) = right.seek_child(edge) {
                        Self::intersect_nodes_lending(
                            left_child,
                            0,
                            right_child,
                            0,
                            key_segments,
                            key_len,
                            on_match,
                        );
                    }
                }
            } else {
                for (edge, right_child) in right.iter() {
                    if let Some(left_child) = left.seek_child(edge) {
                        Self::intersect_nodes_lending(
                            left_child,
                            0,
                            right_child,
                            0,
                            key_segments,
                            key_len,
                            on_match,
                        );
                    }
                }
            }
        }

        key_segments.truncate(restore_segments);
        *key_len = restore_len;
    }

    /// Recursively intersect two nodes and emit only value pairs (no key reconstruction).
    fn intersect_nodes_values<'a, F>(
        left: &'a DefaultNode<KeyType::PartialType, ValueType>,
        mut left_offset: usize,
        right: &'a DefaultNode<KeyType::PartialType, ValueType>,
        mut right_offset: usize,
        on_match: &mut F,
    ) where
        F: FnMut(&'a ValueType, &'a ValueType),
    {
        let left_prefix = left.prefix.as_ref();
        let right_prefix = right.prefix.as_ref();

        while left_offset < left_prefix.len() && right_offset < right_prefix.len() {
            if left_prefix[left_offset] != right_prefix[right_offset] {
                return;
            }
            left_offset += 1;
            right_offset += 1;
        }

        if left_offset < left_prefix.len() {
            if !right.is_inner() {
                return;
            }
            let edge = left_prefix[left_offset];
            let Some(right_child) = right.seek_child(edge) else {
                return;
            };
            Self::intersect_nodes_values(left, left_offset + 1, right_child, 1, on_match);
            return;
        }

        if right_offset < right_prefix.len() {
            if !left.is_inner() {
                return;
            }
            let edge = right_prefix[right_offset];
            let Some(left_child) = left.seek_child(edge) else {
                return;
            };
            Self::intersect_nodes_values(left_child, 1, right, right_offset + 1, on_match);
            return;
        }

        if let (Some(left_value), Some(right_value)) = (left.value(), right.value()) {
            on_match(left_value, right_value);
        }

        if left.is_inner() && right.is_inner() {
            if left.num_children() <= right.num_children() {
                for (edge, left_child) in left.iter() {
                    if let Some(right_child) = right.seek_child(edge) {
                        Self::intersect_nodes_values(left_child, 0, right_child, 0, on_match);
                    }
                }
            } else {
                for (edge, right_child) in right.iter() {
                    if let Some(left_child) = left.seek_child(edge) {
                        Self::intersect_nodes_values(left_child, 0, right_child, 0, on_match);
                    }
                }
            }
        }
    }

    fn get_iterate_mut<'a>(
        cur_node: &'a mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
    ) -> Option<&'a mut ValueType> {
        let mut cur_node = cur_node;
        let key_bytes = key.as_ref();
        let mut depth = 0;
        loop {
            let prefix_len = cur_node.prefix.len();
            let remaining_len = key_bytes.len() - depth;
            let prefix_common_match = cur_node.prefix.prefix_length_slice(&key_bytes[depth..]);
            if prefix_common_match != prefix_len {
                return None;
            }

            if prefix_len == remaining_len {
                return cur_node.value_mut();
            }

            let k = key_bytes[depth + prefix_len];
            depth += prefix_len;
            cur_node = cur_node.seek_child_mut(k)?;
        }
    }

    fn insert_recurse(
        cur_node: &mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        value: ValueType,
        depth: usize,
    ) -> Option<ValueType> {
        let longest_common_prefix = cur_node.prefix.prefix_length_key(key, depth);

        let is_prefix_match =
            min(cur_node.prefix.len(), key.length_at(depth)) == longest_common_prefix;

        // Prefix fully covers this node.
        // Either sets the value or replaces the old value already here.
        if is_prefix_match && cur_node.prefix.len() == key.length_at(depth) {
            if let Some(v) = cur_node.value_mut() {
                return Some(std::mem::replace(v, value));
            }
            cur_node.value = Some(value);
            return None;
        }

        if is_prefix_match && cur_node.prefix.len() > key.length_at(depth) {
            let new_prefix = cur_node.prefix.partial_after(longest_common_prefix);
            let old_node_prefix = std::mem::replace(&mut cur_node.prefix, new_prefix);
            let mut new_parent =
                DefaultNode::new_inner(old_node_prefix.partial_before(longest_common_prefix));
            new_parent.value = Some(value);
            let edge = old_node_prefix.at(longest_common_prefix);
            let replacement_current = std::mem::replace(cur_node, new_parent);
            cur_node.add_child(edge, replacement_current);
            return None;
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
            let new_leaf =
                DefaultNode::new_leaf(key.to_partial(depth + longest_common_prefix), value);

            // Add the old leaf node as a child of the new inner node.
            cur_node.add_child(k1, replacement_current);
            cur_node.add_child(k2, new_leaf);

            return None;
        }

        if cur_node.is_leaf() {
            // Existing key ends at this node but the inserted key continues. This node must start
            // acting as an inner node while retaining its terminal value.
            let edge = key.at(depth + longest_common_prefix);
            let new_leaf =
                DefaultNode::new_leaf(key.to_partial(depth + longest_common_prefix), value);
            cur_node.add_child(edge, new_leaf);
            return None;
        }

        // We must be an inner node, and either we need a new baby, or one of our children does, so
        // we'll hunt and see.
        let k = key.at(depth + longest_common_prefix);

        let Some(child) = cur_node.seek_child_mut(k) else {
            // We should not be a leaf at this point. If so, something bad has happened.
            debug_assert!(cur_node.is_inner());
            let new_leaf =
                DefaultNode::new_leaf(key.to_partial(depth + longest_common_prefix), value);
            cur_node.add_child(k, new_leaf);
            return None;
        };

        AdaptiveRadixTree::insert_recurse(child, key, value, depth + longest_common_prefix)
    }

    fn remove_recurse(
        parent_node: &mut DefaultNode<KeyType::PartialType, ValueType>,
        key: &KeyType,
        depth: usize,
    ) -> Option<ValueType> {
        // Seek the child that matches the key at this depth, which is the first character at the
        // depth we're at.
        let c = key.at(depth);
        let child_node = parent_node.seek_child_mut(c)?;

        let prefix_common_match = child_node.prefix.prefix_length_key(key, depth);
        if prefix_common_match != child_node.prefix.len() {
            return None;
        }

        if child_node.prefix.len() == key.length_at(depth) {
            if child_node.is_leaf() {
                let node = parent_node.delete_child(c).unwrap();
                let v = node
                    .value
                    .expect("corruption: missing value at deleted leaf");
                return Some(v);
            }

            let removed = child_node.value.take();
            if removed.is_some() && child_node.num_children() == 0 {
                let prefix = child_node.prefix.clone();
                let deleted = parent_node.delete_child(c).unwrap();
                debug_assert_eq!(prefix.to_slice(), deleted.prefix.to_slice());
            }
            return removed;
        }

        // If the child is a leaf, and the prefix matches the key, we can remove it from this parent
        // node. If the prefix does not match, then we have nothing to do here.
        if child_node.is_leaf() {
            if child_node.prefix.len() != (key.length_at(depth)) {
                return None;
            }
            let node = parent_node.delete_child(c).unwrap();
            let v = node
                .value
                .expect("corruption: missing value at deleted leaf");
            return Some(v);
        }

        // Otherwise, recurse down the branch in that direction.
        let result =
            AdaptiveRadixTree::remove_recurse(child_node, key, depth + child_node.prefix.len());

        // If after this our child we just recursed into no longer has children of its own, it can
        // be collapsed into us. In this way we can prune the tree as we go.
        if result.is_some()
            && child_node.is_inner()
            && child_node.num_children() == 0
            && child_node.value().is_none()
        {
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
        if node.is_leaf() {
            tree_stats.num_leaves += 1;
        } else {
            update_tree_stats(tree_stats, node);
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
    use std::collections::BTreeMap;
    use std::fmt::Debug;
    use std::ops::Bound::{Excluded, Included, Unbounded};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use proptest::prelude::*;

    use crate::VisitControl;
    use crate::keys::KeyTrait;
    use crate::keys::array_key::ArrayKey;
    use crate::keys::vector_key::VectorKey;
    use crate::partials::array_partial::ArrPartial;
    use crate::tree::AdaptiveRadixTree;

    fn collect_items(tree: &AdaptiveRadixTree<ArrayKey<16>, u64>) -> Vec<(Vec<u8>, u64)> {
        tree.iter()
            .map(|(key, value)| (key.as_ref().to_vec(), *value))
            .collect()
    }

    #[test]
    fn bulk_load_matches_incremental_insert_for_sorted_numeric_keys() {
        let items: Vec<_> = (0..4096u64)
            .map(|value| (ArrayKey::from(value), value))
            .collect();
        let mut incremental = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        for &(key, value) in &items {
            incremental.insert_k(&key, value);
        }

        let bulk = AdaptiveRadixTree::<ArrayKey<16>, u64>::bulk_load_sorted_unique(items);

        assert_eq!(collect_items(&bulk), collect_items(&incremental));
        for raw_key in [0, 1, 255, 1024, 4095] {
            let key = ArrayKey::from(raw_key);
            assert_eq!(bulk.get_k(&key), Some(&raw_key));
        }
    }

    #[test]
    fn bulk_load_sorted_handles_prefix_keys_and_empty_key() {
        let items = vec![
            (ArrayKey::new_from_slice(b""), 0),
            (ArrayKey::new_from_slice(b"a"), 1),
            (ArrayKey::new_from_slice(b"ab"), 2),
            (ArrayKey::new_from_slice(b"abc"), 3),
            (ArrayKey::new_from_slice(b"b"), 4),
        ];
        let mut incremental = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        for &(key, value) in &items {
            incremental.insert_k(&key, value);
        }

        let bulk = AdaptiveRadixTree::<ArrayKey<16>, u64>::bulk_load_sorted(items);

        assert_eq!(collect_items(&bulk), collect_items(&incremental));
        assert_eq!(bulk.get_k(&ArrayKey::new_from_slice(b"")), Some(&0));
        assert_eq!(bulk.get_k(&ArrayKey::new_from_slice(b"abc")), Some(&3));
    }

    #[test]
    fn bulk_load_sorted_unique_matches_incremental_insert() {
        let items = vec![
            (ArrayKey::new_from_slice(b"a"), 1),
            (ArrayKey::new_from_slice(b"ab"), 2),
            (ArrayKey::new_from_slice(b"b"), 3),
            (ArrayKey::new_from_slice(b"ba"), 4),
        ];
        let mut incremental = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        for &(key, value) in &items {
            incremental.insert_k(&key, value);
        }

        let bulk = AdaptiveRadixTree::<ArrayKey<16>, u64>::bulk_load_sorted_unique(items);

        assert_eq!(collect_items(&bulk), collect_items(&incremental));
    }

    #[test]
    fn bulk_load_sorted_unique_by_index_matches_incremental_insert() {
        let keys = [
            ArrayKey::new_from_slice(b""),
            ArrayKey::new_from_slice(b"a"),
            ArrayKey::new_from_slice(b"ab"),
            ArrayKey::new_from_slice(b"b"),
            ArrayKey::new_from_slice(b"ba"),
        ];
        let mut values: Vec<_> = (0..keys.len()).map(|value| Some(value as u64)).collect();
        let mut incremental = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        for (index, key) in keys.iter().enumerate() {
            incremental.insert_k(key, index as u64);
        }

        let bulk = AdaptiveRadixTree::<ArrayKey<16>, u64>::bulk_load_sorted_unique_by_index(
            keys.len(),
            |index| &keys[index],
            |index| values[index].take().expect("value should be taken once"),
        );

        assert_eq!(collect_items(&bulk), collect_items(&incremental));
        assert!(values.iter().all(Option::is_none));
    }

    #[test]
    fn bulk_load_sorted_keeps_last_adjacent_duplicate_value() {
        let items = vec![
            (ArrayKey::new_from_slice(b"a"), 1),
            (ArrayKey::new_from_slice(b"a"), 3),
            (ArrayKey::new_from_slice(b"b"), 2),
        ];

        let tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::bulk_load_sorted(items);

        assert_eq!(tree.get_k(&ArrayKey::new_from_slice(b"a")), Some(&3));
        assert_eq!(tree.get_k(&ArrayKey::new_from_slice(b"b")), Some(&2));
    }

    #[test]
    #[should_panic(expected = "bulk_load_sorted input is not sorted")]
    fn bulk_load_sorted_rejects_unsorted_input() {
        let items = vec![
            (ArrayKey::new_from_slice(b"b"), 1),
            (ArrayKey::new_from_slice(b"a"), 2),
        ];

        let _ = AdaptiveRadixTree::<ArrayKey<16>, u64>::bulk_load_sorted(items);
    }

    #[test]
    fn values_iter_includes_root_value() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        tree.insert("a", 1);
        tree.insert("ab", 2);
        tree.insert("abc", 3);

        let values: Vec<_> = tree.values_iter().copied().collect();
        assert_eq!(values, vec![1, 2, 3]);
    }

    static PANIC_ON_FOUR_CMP: AtomicBool = AtomicBool::new(false);
    static PANIC_ON_BELOW_M_CMP: AtomicBool = AtomicBool::new(false);
    static PANIC_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Clone, Eq, PartialEq, Debug)]
    struct PanickyRangeKey(ArrayKey<16>);

    impl PanickyRangeKey {
        fn as_u64(&self) -> u64 {
            self.0.to_be_u64()
        }
    }

    impl AsRef<[u8]> for PanickyRangeKey {
        fn as_ref(&self) -> &[u8] {
            self.0.as_ref()
        }
    }

    impl PartialOrd for PanickyRangeKey {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for PanickyRangeKey {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            if PANIC_ON_FOUR_CMP.load(Ordering::Relaxed)
                && (self.as_u64() == 4 || other.as_u64() == 4)
            {
                panic!("range compared past first out-of-range key");
            }
            if PANIC_ON_BELOW_M_CMP.load(Ordering::Relaxed) {
                let lhs = self.as_ref().first().copied().unwrap_or_default();
                let rhs = other.as_ref().first().copied().unwrap_or_default();
                if lhs < b'm' || rhs < b'm' {
                    panic!("range start seek compared a key below start prefix");
                }
            }
            self.0.cmp(&other.0)
        }
    }

    impl From<u64> for PanickyRangeKey {
        fn from(value: u64) -> Self {
            Self(value.into())
        }
    }

    impl From<&str> for PanickyRangeKey {
        fn from(value: &str) -> Self {
            Self(value.into())
        }
    }

    impl From<PanickyRangeKey> for ArrPartial<16> {
        fn from(value: PanickyRangeKey) -> Self {
            value.0.to_partial(0)
        }
    }

    impl KeyTrait for PanickyRangeKey {
        type PartialType = ArrPartial<16>;
        const MAXIMUM_SIZE: Option<usize> = Some(16);

        fn new_from_slice(slice: &[u8]) -> Self {
            Self(ArrayKey::new_from_slice(slice))
        }

        fn new_from_partial(partial: &Self::PartialType) -> Self {
            Self(ArrayKey::new_from_partial(partial))
        }

        fn extend_from_partial(&self, partial: &Self::PartialType) -> Self {
            Self(self.0.extend_from_partial(partial))
        }

        fn truncate(&self, at_depth: usize) -> Self {
            Self(self.0.truncate(at_depth))
        }

        fn at(&self, pos: usize) -> u8 {
            self.0.at(pos)
        }

        fn length_at(&self, at_depth: usize) -> usize {
            self.0.length_at(at_depth)
        }

        fn to_partial(&self, at_depth: usize) -> Self::PartialType {
            self.0.to_partial(at_depth)
        }

        fn matches_slice(&self, slice: &[u8]) -> bool {
            self.0.matches_slice(slice)
        }
    }

    #[derive(Clone, Debug)]
    enum TreeOp {
        Get { key: u8 },
        Insert { key: u8, value: u16 },
        Update { key: u8, value: u16 },
        Remove { key: u8 },
    }

    fn tree_op_strategy() -> impl Strategy<Value = TreeOp> {
        prop_oneof![
            any::<u8>().prop_map(|key| TreeOp::Get { key }),
            (any::<u8>(), any::<u16>()).prop_map(|(key, value)| TreeOp::Insert { key, value }),
            (any::<u8>(), any::<u16>()).prop_map(|(key, value)| TreeOp::Update { key, value }),
            any::<u8>().prop_map(|key| TreeOp::Remove { key }),
        ]
    }

    #[derive(Clone, Debug)]
    enum RangeQuery {
        All,
        From {
            start: u8,
            inclusive: bool,
        },
        To {
            end: u8,
            inclusive: bool,
        },
        Between {
            start: u8,
            end: u8,
            start_inclusive: bool,
            end_inclusive: bool,
        },
    }

    fn range_query_strategy() -> impl Strategy<Value = RangeQuery> {
        prop_oneof![
            Just(RangeQuery::All),
            (any::<u8>(), any::<bool>())
                .prop_map(|(start, inclusive)| RangeQuery::From { start, inclusive }),
            (any::<u8>(), any::<bool>())
                .prop_map(|(end, inclusive)| RangeQuery::To { end, inclusive }),
            (any::<u8>(), any::<u8>(), any::<bool>(), any::<bool>()).prop_map(
                |(a, b, start_inclusive, end_inclusive)| {
                    let (start, end) = if a <= b { (a, b) } else { (b, a) };
                    let (start_inclusive, end_inclusive) = if start == end {
                        (true, true)
                    } else {
                        (start_inclusive, end_inclusive)
                    };
                    RangeQuery::Between {
                        start,
                        end,
                        start_inclusive,
                        end_inclusive,
                    }
                }
            ),
        ]
    }

    fn ascii_key_strategy() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec(b'a'..=b'd', 1..=6)
    }

    fn trim_array_key_bytes(bytes: &[u8]) -> Vec<u8> {
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        bytes[..end].to_vec()
    }

    fn collect_art_u8_items(tree: &AdaptiveRadixTree<ArrayKey<16>, u16>) -> Vec<(u8, u16)> {
        tree.iter()
            .map(|(key, value)| (key.to_be_u64() as u8, *value))
            .collect()
    }

    fn collect_art_range_u8_items(
        tree: &AdaptiveRadixTree<ArrayKey<16>, u16>,
        query: &RangeQuery,
    ) -> Vec<(u8, u16)> {
        match *query {
            RangeQuery::All => tree
                .range(..)
                .map(|(key, value)| (key.to_be_u64() as u8, *value))
                .collect(),
            RangeQuery::From { start, inclusive } => {
                let start_key: ArrayKey<16> = start.into();
                if inclusive {
                    tree.range((Included(start_key), Unbounded))
                } else {
                    tree.range((Excluded(start_key), Unbounded))
                }
                .map(|(key, value)| (key.to_be_u64() as u8, *value))
                .collect()
            }
            RangeQuery::To { end, inclusive } => {
                let end_key: ArrayKey<16> = end.into();
                if inclusive {
                    tree.range((Unbounded, Included(end_key)))
                } else {
                    tree.range((Unbounded, Excluded(end_key)))
                }
                .map(|(key, value)| (key.to_be_u64() as u8, *value))
                .collect()
            }
            RangeQuery::Between {
                start,
                end,
                start_inclusive,
                end_inclusive,
            } => {
                let start_key: ArrayKey<16> = start.into();
                let end_key: ArrayKey<16> = end.into();
                let start_bound = if start_inclusive {
                    Included(start_key)
                } else {
                    Excluded(start_key)
                };
                let end_bound = if end_inclusive {
                    Included(end_key)
                } else {
                    Excluded(end_key)
                };
                tree.range((start_bound, end_bound))
                    .map(|(key, value)| (key.to_be_u64() as u8, *value))
                    .collect()
            }
        }
    }

    fn collect_btree_range_u8_items(map: &BTreeMap<u8, u16>, query: &RangeQuery) -> Vec<(u8, u16)> {
        match *query {
            RangeQuery::All => map.range(..).map(|(key, value)| (*key, *value)).collect(),
            RangeQuery::From { start, inclusive } => if inclusive {
                map.range((Included(start), Unbounded))
            } else {
                map.range((Excluded(start), Unbounded))
            }
            .map(|(key, value)| (*key, *value))
            .collect(),
            RangeQuery::To { end, inclusive } => if inclusive {
                map.range((Unbounded, Included(end)))
            } else {
                map.range((Unbounded, Excluded(end)))
            }
            .map(|(key, value)| (*key, *value))
            .collect(),
            RangeQuery::Between {
                start,
                end,
                start_inclusive,
                end_inclusive,
            } => {
                let start_bound = if start_inclusive {
                    Included(start)
                } else {
                    Excluded(start)
                };
                let end_bound = if end_inclusive {
                    Included(end)
                } else {
                    Excluded(end)
                };
                map.range((start_bound, end_bound))
                    .map(|(key, value)| (*key, *value))
                    .collect()
            }
        }
    }

    proptest! {
        #[test]
        fn prop_map_operations_match_btreemap(
            ops in proptest::collection::vec(tree_op_strategy(), 0..128)
        ) {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u16>::new();
            let mut btree = BTreeMap::<u8, u16>::new();

            for op in ops {
                match op {
                    TreeOp::Get { key } => {
                        prop_assert_eq!(tree.get(key).copied(), btree.get(&key).copied());
                    }
                    TreeOp::Insert { key, value } => {
                        let art_old = tree.insert(key, value);
                        let btree_old = btree.insert(key, value);
                        prop_assert_eq!(art_old, btree_old);
                        prop_assert_eq!(tree.get(key).copied(), btree.get(&key).copied());
                    }
                    TreeOp::Update { key, value } => {
                        let art_present = if let Some(slot) = tree.get_mut(key) {
                            *slot = value;
                            true
                        } else {
                            false
                        };
                        let btree_present = if let Some(slot) = btree.get_mut(&key) {
                            *slot = value;
                            true
                        } else {
                            false
                        };
                        prop_assert_eq!(art_present, btree_present);
                        prop_assert_eq!(tree.get(key).copied(), btree.get(&key).copied());
                    }
                    TreeOp::Remove { key } => {
                        let art_removed = tree.remove(key);
                        let btree_removed = btree.remove(&key);
                        prop_assert_eq!(art_removed, btree_removed);
                        prop_assert_eq!(tree.get(key).copied(), btree.get(&key).copied());
                    }
                }
            }

            let art_items = collect_art_u8_items(&tree);
            let btree_items: Vec<_> = btree.iter().map(|(key, value)| (*key, *value)).collect();
            prop_assert_eq!(art_items, btree_items);
        }

        #[test]
        fn prop_range_queries_match_btreemap(
            entries in proptest::collection::vec((any::<u8>(), any::<u16>()), 0..96),
            queries in proptest::collection::vec(range_query_strategy(), 0..32)
        ) {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u16>::new();
            let mut btree = BTreeMap::<u8, u16>::new();

            for (key, value) in entries {
                tree.insert(key, value);
                btree.insert(key, value);
            }

            for query in &queries {
                let art_items = collect_art_range_u8_items(&tree, query);
                let btree_items = collect_btree_range_u8_items(&btree, query);
                prop_assert_eq!(art_items, btree_items);
            }
        }

        #[test]
        fn prop_prefix_queries_match_reference_model(
            entries in proptest::collection::vec((ascii_key_strategy(), any::<u8>()), 0..64),
            probes in proptest::collection::vec(ascii_key_strategy(), 0..32)
        ) {
            let mut tree = AdaptiveRadixTree::<ArrayKey<8>, u8>::new();
            let mut map = BTreeMap::<Vec<u8>, u8>::new();

            for (key, value) in entries {
                tree.insert_k(&ArrayKey::<8>::new_from_slice(&key), value);
                map.insert(key, value);
            }

            for probe in probes {
                let prefix = ArrayKey::<8>::new_from_slice(&probe);
                let got_prefix: Vec<_> = tree
                    .prefix_iter_k(&prefix)
                    .map(|(key, value)| (trim_array_key_bytes(key.as_ref()), *value))
                    .collect();
                let expected_prefix: Vec<_> = map
                    .iter()
                    .filter(|(key, _)| key.starts_with(&probe))
                    .map(|(key, value)| (key.clone(), *value))
                    .collect();
                prop_assert_eq!(got_prefix, expected_prefix);

                let got_longest = tree
                    .longest_prefix_match_k(&prefix)
                    .map(|(key, value)| (trim_array_key_bytes(key.as_ref()), *value));
                let expected_longest = map
                    .iter()
                    .filter(|(key, _)| probe.starts_with(key.as_slice()))
                    .max_by_key(|(key, _)| key.len())
                    .map(|(key, value)| (key.clone(), *value));
                prop_assert_eq!(got_longest, expected_longest);

                let got_prefix_matches: Vec<_> = tree
                    .prefix_match_iter_k(&prefix)
                    .map(|(key, value)| (trim_array_key_bytes(key.as_ref()), *value))
                    .collect();
                let expected_prefix_matches: Vec<_> = map
                    .iter()
                    .filter(|(key, _)| probe.starts_with(key.as_slice()))
                    .map(|(key, value)| (key.clone(), *value))
                    .collect();
                prop_assert_eq!(&got_prefix_matches, &expected_prefix_matches);

                let mut got_prefix_matches_with_callback = Vec::new();
                tree.prefix_match_for_each_k(&prefix, |key, value| {
                    got_prefix_matches_with_callback.push((key.to_vec(), *value));
                });
                prop_assert_eq!(
                    &got_prefix_matches_with_callback,
                    &expected_prefix_matches
                );
            }
        }

        #[test]
        fn prop_intersection_matches_reference_model(
            left_entries in proptest::collection::vec((ascii_key_strategy(), any::<u8>()), 0..64),
            right_entries in proptest::collection::vec((ascii_key_strategy(), any::<u8>()), 0..64)
        ) {
            let mut left = AdaptiveRadixTree::<ArrayKey<8>, u8>::new();
            let mut right = AdaptiveRadixTree::<ArrayKey<8>, u8>::new();
            let mut left_map = BTreeMap::<Vec<u8>, u8>::new();
            let mut right_map = BTreeMap::<Vec<u8>, u8>::new();

            for (key, value) in left_entries {
                left.insert_k(&ArrayKey::<8>::new_from_slice(&key), value);
                left_map.insert(key, value);
            }

            for (key, value) in right_entries {
                right.insert_k(&ArrayKey::<8>::new_from_slice(&key), value);
                right_map.insert(key, value);
            }

            let expected: Vec<_> = left_map
                .iter()
                .filter_map(|(key, left_value)| {
                    right_map
                        .get(key)
                        .map(|right_value| (key.clone(), *left_value, *right_value))
                })
                .collect();
            let expected_count = expected.len();

            let mut got = Vec::new();
            left.intersect_with(&right, |key, left_value, right_value| {
                got.push((trim_array_key_bytes(key.as_ref()), *left_value, *right_value));
            });
            got.sort();

            let mut got_values = Vec::new();
            left.intersect_values_with(&right, |left_value, right_value| {
                got_values.push((*left_value, *right_value));
            });
            got_values.sort();

            let mut expected_values: Vec<_> = expected
                .iter()
                .map(|(_, left_value, right_value)| (*left_value, *right_value))
                .collect();
            expected_values.sort();

            prop_assert_eq!(got, expected);
            prop_assert_eq!(got_values, expected_values);
            prop_assert_eq!(left.intersect_count(&right), expected_count);
        }

    }

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

    #[test]
    fn test_iter_one_regression() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        tree.insert(123, 456);
        let mut iter = tree.iter();
        let result = iter.next().expect("Expected an entry");
        assert_eq!(result.1, &456)
    }

    #[test]
    fn test_prefix_iter_returns_sorted_prefix_subset() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"alpha1"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"alpha2"), 2);
        tree.insert_k(&VectorKey::new_from_slice(b"alphabet"), 3);
        tree.insert_k(&VectorKey::new_from_slice(b"alpine"), 4);
        tree.insert_k(&VectorKey::new_from_slice(b"beta"), 5);

        let prefix = VectorKey::new_from_slice(b"alp");
        let got: Vec<(String, i32)> = tree
            .prefix_iter_k(&prefix)
            .map(|(k, v)| {
                (
                    String::from_utf8(k.as_ref().to_vec()).expect("key must be valid UTF-8"),
                    *v,
                )
            })
            .collect();

        assert_eq!(
            got,
            vec![
                ("alpha1".to_string(), 1),
                ("alpha2".to_string(), 2),
                ("alphabet".to_string(), 3),
                ("alpine".to_string(), 4),
            ]
        );
    }

    #[test]
    fn test_prefix_match_iter_returns_saved_prefixes() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        for (idx, key) in [
            b"".as_slice(),
            b"a".as_slice(),
            b"alpha".as_slice(),
            b"alphabet".as_slice(),
            b"alphabetical".as_slice(),
            b"apple".as_slice(),
        ]
        .iter()
        .enumerate()
        {
            tree.insert_k(&VectorKey::new_from_slice(key), idx as i32);
        }

        let got: Vec<Vec<u8>> = tree
            .prefix_match_iter(VectorKey::new_from_slice(b"alphabet"))
            .map(|(key, _)| key.as_ref().to_vec())
            .collect();

        assert_eq!(
            got,
            vec![
                b"".to_vec(),
                b"a".to_vec(),
                b"alpha".to_vec(),
                b"alphabet".to_vec(),
            ]
        );
    }

    #[test]
    fn test_prefix_match_for_each_returns_borrowed_probe_prefixes() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b""), 0);
        tree.insert_k(&VectorKey::new_from_slice(b"a"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"alpha"), 2);
        tree.insert_k(&VectorKey::new_from_slice(b"alphabet"), 3);
        tree.insert_k(&VectorKey::new_from_slice(b"alphabetical"), 4);

        let probe = VectorKey::new_from_slice(b"alphabet");
        let probe_base = probe.as_ref().as_ptr() as usize;
        let probe_end = probe_base + probe.as_ref().len();
        let mut got = Vec::new();

        tree.prefix_match_for_each_k(&probe, |key, value| {
            let ptr = key.as_ptr() as usize;
            assert!(ptr >= probe_base && ptr <= probe_end);
            got.push((key.to_vec(), *value));
        });

        assert_eq!(
            got,
            vec![
                (b"".to_vec(), 0),
                (b"a".to_vec(), 1),
                (b"alpha".to_vec(), 2),
                (b"alphabet".to_vec(), 3),
            ]
        );
    }

    #[test]
    fn test_prefix_match_iter_no_match() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"alpha"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"alphabet"), 2);

        let probe = VectorKey::new_from_slice(b"alpine");
        assert_eq!(tree.prefix_match_iter_k(&probe).count(), 0);
    }

    #[test]
    fn test_for_each_view_returns_sorted_entries() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"apple"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"banana"), 2);
        tree.insert_k(&VectorKey::new_from_slice(b"cherry"), 3);

        let mut got = Vec::new();
        tree.for_each_view(|k, v| {
            got.push((
                String::from_utf8(k.to_vec()).expect("key must be valid UTF-8"),
                *v,
            ));
        });

        assert_eq!(
            got,
            vec![
                ("apple".to_string(), 1),
                ("banana".to_string(), 2),
                ("cherry".to_string(), 3),
            ]
        );
    }

    #[test]
    fn test_prefix_for_each_view_returns_sorted_prefix_subset() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"alpha1"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"alpha2"), 2);
        tree.insert_k(&VectorKey::new_from_slice(b"alphabet"), 3);
        tree.insert_k(&VectorKey::new_from_slice(b"alpine"), 4);
        tree.insert_k(&VectorKey::new_from_slice(b"beta"), 5);

        let prefix = VectorKey::new_from_slice(b"alp");
        let mut got = Vec::new();
        tree.prefix_for_each_view_k(&prefix, |k, v| {
            got.push((
                String::from_utf8(k.to_vec()).expect("key must be valid UTF-8"),
                *v,
            ));
        });

        assert_eq!(
            got,
            vec![
                ("alpha1".to_string(), 1),
                ("alpha2".to_string(), 2),
                ("alphabet".to_string(), 3),
                ("alpine".to_string(), 4),
            ]
        );
    }

    #[test]
    fn prefix_values_for_each_visits_sorted_prefix_values() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"alpha1"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"alpha2"), 2);
        tree.insert_k(&VectorKey::new_from_slice(b"alphabet"), 3);
        tree.insert_k(&VectorKey::new_from_slice(b"alpine"), 4);
        tree.insert_k(&VectorKey::new_from_slice(b"beta"), 5);

        let mut got = Vec::new();
        tree.prefix_values_for_each_k(&VectorKey::new_from_slice(b"alp"), |value| {
            got.push(*value);
        });
        assert_eq!(got, vec![1, 2, 3, 4]);

        let mut no_match_count = 0;
        tree.prefix_values_for_each_k(&VectorKey::new_from_slice(b"zzz"), |_| {
            no_match_count += 1;
        });
        assert_eq!(no_match_count, 0);
    }

    #[test]
    fn try_prefix_values_for_each_stops_and_propagates_errors() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"alpha1"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"alpha2"), 2);
        tree.insert_k(&VectorKey::new_from_slice(b"alphabet"), 3);
        tree.insert_k(&VectorKey::new_from_slice(b"alpine"), 4);

        let mut stopped = Vec::new();
        let stop_result: Result<(), &str> =
            tree.try_prefix_values_for_each_k(&VectorKey::new_from_slice(b"alp"), |value| {
                stopped.push(*value);
                Ok(if *value == 2 {
                    VisitControl::Stop
                } else {
                    VisitControl::Continue
                })
            });
        assert_eq!(stop_result, Ok(()));
        assert_eq!(stopped, vec![1, 2]);

        let mut errored = Vec::new();
        let error_result =
            tree.try_prefix_values_for_each_k(&VectorKey::new_from_slice(b"alp"), |value| {
                errored.push(*value);
                if *value == 3 {
                    Err("boom")
                } else {
                    Ok(VisitControl::Continue)
                }
            });
        assert_eq!(error_result, Err("boom"));
        assert_eq!(errored, vec![1, 2, 3]);
    }

    #[test]
    fn test_prefix_iter_no_match() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"alpha"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"beta"), 2);

        let prefix = VectorKey::new_from_slice(b"zzz");
        assert_eq!(tree.prefix_iter_k(&prefix).count(), 0);
    }

    #[test]
    fn test_prefix_iter_short_prefix_regression() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(&[0x01, 0x02, b'a']), 1);
        tree.insert_k(&VectorKey::new_from_slice(&[0x01, 0x02, b'b']), 2);
        tree.insert_k(&VectorKey::new_from_slice(&[0x01, 0x03, b'c']), 3);

        let prefix = VectorKey::new_from_slice(&[0x01, 0x02]);
        let got: Vec<i32> = tree.prefix_iter_k(&prefix).map(|(_, v)| *v).collect();
        assert_eq!(got, vec![1, 2]);
    }

    #[test]
    fn test_for_each_range_view_returns_expected_subset() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"apple"), 1);
        tree.insert_k(&VectorKey::new_from_slice(b"banana"), 2);
        tree.insert_k(&VectorKey::new_from_slice(b"cherry"), 3);
        tree.insert_k(&VectorKey::new_from_slice(b"date"), 4);

        let start = VectorKey::new_from_slice(b"b");
        let end = VectorKey::new_from_slice(b"d");
        let mut got = Vec::new();
        tree.for_each_range_view(start..end, |k, v| {
            got.push((
                String::from_utf8(k.to_vec()).expect("key must be valid UTF-8"),
                *v,
            ));
        });

        assert_eq!(
            got,
            vec![("banana".to_string(), 2), ("cherry".to_string(), 3),]
        );
    }

    #[test]
    fn test_longest_prefix_match() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"cat"), 10);
        tree.insert_k(&VectorKey::new_from_slice(b"dog"), 20);

        let (matched_key, matched_value) = tree
            .longest_prefix_match(VectorKey::new_from_slice(b"catalog"))
            .expect("expected a prefix match");
        assert_eq!(matched_key.as_ref(), b"cat");
        assert_eq!(*matched_value, 10);

        let (matched_key, matched_value) = tree
            .longest_prefix_match(VectorKey::new_from_slice(b"dog"))
            .expect("expected exact match");
        assert_eq!(matched_key.as_ref(), b"dog");
        assert_eq!(*matched_value, 20);

        let (matched_key, matched_value) = tree
            .longest_prefix_match(VectorKey::new_from_slice(b"doge"))
            .expect("expected prefix match");
        assert_eq!(matched_key.as_ref(), b"dog");
        assert_eq!(*matched_value, 20);

        assert!(
            tree.longest_prefix_match(VectorKey::new_from_slice(b"do"))
                .is_none()
        );
        assert!(
            tree.longest_prefix_match(VectorKey::new_from_slice(b"zebra"))
                .is_none()
        );
    }

    #[test]
    fn test_with_longest_prefix_match_view() {
        let mut tree = AdaptiveRadixTree::<VectorKey, i32>::new();
        tree.insert_k(&VectorKey::new_from_slice(b"cat"), 10);
        tree.insert_k(&VectorKey::new_from_slice(b"dog"), 20);

        let mut seen = None;
        let found = tree.with_longest_prefix_match_view(
            VectorKey::new_from_slice(b"catalog"),
            |matched_key, matched_value| {
                seen = Some((matched_key.to_vec(), *matched_value));
            },
        );

        assert!(found);
        assert_eq!(seen, Some((b"cat".to_vec(), 10)));
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
    fn test_intersect_with_returns_common_keys_and_values() {
        let mut left = AdaptiveRadixTree::<ArrayKey<32>, i32>::new();
        let mut right = AdaptiveRadixTree::<ArrayKey<32>, i32>::new();

        for (k, v) in [
            ("a", 1),
            ("ab", 2),
            ("abc", 3),
            ("abd", 4),
            ("bzz", 5),
            ("cat", 6),
        ] {
            left.insert(k, v);
        }

        for (k, v) in [("ab", 20), ("abc", 30), ("bzz", 50), ("dog", 70)] {
            right.insert(k, v);
        }

        let mut seen = Vec::new();
        left.intersect_with(&right, |key, left_value, right_value| {
            seen.push((key, *left_value, *right_value));
        });

        let mut keys: Vec<String> = seen
            .iter()
            .map(|(k, _, _)| {
                let bytes = k.as_ref();
                let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
            })
            .collect();
        keys.sort();

        assert_eq!(keys, vec!["ab", "abc", "bzz"]);
        assert!(
            seen.iter()
                .any(|(k, lv, rv)| *k == "ab".into() && *lv == 2 && *rv == 20)
        );
        assert!(
            seen.iter()
                .any(|(k, lv, rv)| *k == "abc".into() && *lv == 3 && *rv == 30)
        );
        assert!(
            seen.iter()
                .any(|(k, lv, rv)| *k == "bzz".into() && *lv == 5 && *rv == 50)
        );
    }

    #[test]
    fn test_intersect_lending_with_returns_common_keys_and_values() {
        let mut left = AdaptiveRadixTree::<ArrayKey<32>, i32>::new();
        let mut right = AdaptiveRadixTree::<ArrayKey<32>, i32>::new();

        for (k, v) in [("a", 1), ("ab", 2), ("abc", 3), ("abd", 4), ("bzz", 5)] {
            left.insert(k, v);
        }

        for (k, v) in [("ab", 20), ("abc", 30), ("bzz", 50), ("dog", 70)] {
            right.insert(k, v);
        }

        let mut seen = Vec::new();
        left.intersect_lending_with(&right, |key, left_value, right_value| {
            seen.push((
                trim_array_key_bytes(&key.to_vec()),
                *left_value,
                *right_value,
            ));
        });
        seen.sort();

        assert_eq!(
            seen,
            vec![
                (b"ab".to_vec(), 2, 20),
                (b"abc".to_vec(), 3, 30),
                (b"bzz".to_vec(), 5, 50),
            ]
        );
    }

    #[test]
    fn test_intersect_with_empty_tree() {
        let mut left = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let right = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        left.insert("a", 1);
        left.insert("b", 2);

        let mut count = 0usize;
        left.intersect_with(&right, |_k, _lv, _rv| {
            count += 1;
        });
        assert_eq!(count, 0);
    }

    #[test]
    fn test_intersect_values_with_and_count() {
        let mut left = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
        let mut right = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();

        left.insert("aa", 1);
        left.insert("ab", 2);
        left.insert("ac", 3);

        right.insert("ab", 20);
        right.insert("ac", 30);
        right.insert("zz", 40);

        let mut pairs = Vec::new();
        left.intersect_values_with(&right, |lv, rv| pairs.push((*lv, *rv)));
        pairs.sort_unstable();

        assert_eq!(pairs, vec![(2, 20), (3, 30)]);
        assert_eq!(left.intersect_count(&right), 2);
    }

    #[test]
    fn test_range_stops_after_first_out_of_bounds_regression() {
        let _guard = PANIC_TEST_LOCK.lock().unwrap();
        PANIC_ON_FOUR_CMP.store(false, Ordering::Relaxed);
        PANIC_ON_BELOW_M_CMP.store(false, Ordering::Relaxed);
        let mut tree = AdaptiveRadixTree::<PanickyRangeKey, u64>::new();
        for i in 0..=4u64 {
            let key: PanickyRangeKey = i.into();
            tree.insert_k(&key, i);
        }

        let end: PanickyRangeKey = 2u64.into();
        PANIC_ON_FOUR_CMP.store(true, Ordering::Relaxed);
        let results: Vec<u64> = tree.range(..=end).map(|(_, v)| *v).collect();
        PANIC_ON_FOUR_CMP.store(false, Ordering::Relaxed);
        PANIC_ON_BELOW_M_CMP.store(false, Ordering::Relaxed);

        assert_eq!(results, vec![0, 1, 2]);
    }

    #[test]
    fn test_range_start_seek_regression() {
        let _guard = PANIC_TEST_LOCK.lock().unwrap();
        PANIC_ON_FOUR_CMP.store(false, Ordering::Relaxed);
        PANIC_ON_BELOW_M_CMP.store(false, Ordering::Relaxed);
        let mut tree = AdaptiveRadixTree::<PanickyRangeKey, u64>::new();
        for (i, c) in ('a'..='z').enumerate() {
            let key: PanickyRangeKey = format!("{c}key").as_str().into();
            tree.insert_k(&key, i as u64);
        }

        let start: PanickyRangeKey = "m".into();
        PANIC_ON_BELOW_M_CMP.store(true, Ordering::Relaxed);
        let collected: Vec<u64> = tree.range(start..).map(|(_, v)| *v).collect();
        PANIC_ON_BELOW_M_CMP.store(false, Ordering::Relaxed);
        PANIC_ON_FOUR_CMP.store(false, Ordering::Relaxed);

        let expected: Vec<u64> = (12..=25).collect();
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_range_to_inclusive_fuzz_regression() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        tree.insert(248u64, 7_800_515_995_666_006_788);
        tree.insert(2_678_072_818_765_473_061u64, 2_387_225_703_656_202_751);
        tree.insert(16_100_209_717_274_439_535u64, 8_027_225_910_236_114_799);
        tree.insert(6_196_794_136_686_718_831u64, 18_446_744_073_709_514_607);
        tree.insert(12_219_677_559_081_489_409u64, 4_683_546_028_065_928_715);

        let end: ArrayKey<16> = 67_478_703_180u64.into();
        let got: Vec<u64> = tree.range(..=end).map(|(_, v)| *v).collect();

        assert_eq!(got, vec![7_800_515_995_666_006_788]);
    }

    #[test]
    fn test_range_from_fuzz_regression() {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64>::new();
        let mut btree = BTreeMap::<u64, u64>::new();

        let pairs = [
            (3_124_419_705_906_079_527u64, 3_110_813_966_761_929_515u64),
            (18_446_505_647_410_981_675u64, 23_171_125_240_484_607u64),
            (14_251_014_049_101_104_581u64, 18_446_743_327_766_348_229u64),
            (2_882_303_757_842_906_925u64, 71_779_585_756_702_509u64),
            (12_297_829_382_473_187_410u64, 682u64),
        ];

        for (k, v) in pairs {
            tree.insert(k, v);
            btree.insert(k, v);
        }

        let start_raw = 5_931_894_175_636_062_208u64;
        let start_key: ArrayKey<16> = start_raw.into();

        let art_values: Vec<u64> = tree.range(start_key..).map(|(_, v)| *v).collect();
        let btree_values: Vec<u64> = btree.range(start_raw..).map(|(_, v)| *v).collect();

        assert_eq!(art_values, btree_values);
    }
}
