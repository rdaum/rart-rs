//! Iterator implementation for RART.
//!
//! This module provides iteration capabilities for Adaptive Radix Trees, allowing
//! traversal of all key-value pairs in lexicographic order.
//!
//! The iterator is designed to be memory-efficient and performs lazy evaluation,
//! only visiting nodes as needed during iteration.

use std::collections::Bound;

use crate::keys::KeyTrait;
use crate::node::{DefaultNode, Node};
use crate::partials::Partial;

type IterEntry<'a, P, V> = (u8, &'a DefaultNode<P, V>);
type NodeIterator<'a, P, V> = dyn Iterator<Item = IterEntry<'a, P, V>> + 'a;

/// Iterator over all key-value pairs in an Adaptive Radix Tree.
///
/// This iterator traverses the tree in lexicographic order of the keys,
/// yielding `(Key, &Value)` pairs. The iteration is performed lazily,
/// visiting nodes only as needed.
///
/// ## Examples
///
/// ```rust
/// use rart::{AdaptiveRadixTree, ArrayKey};
///
/// let mut tree = AdaptiveRadixTree::<ArrayKey<16>, i32>::new();
/// tree.insert("apple", 1);
/// tree.insert("banana", 2);
/// tree.insert("cherry", 3);
///
/// // Iterate in lexicographic order
/// let items: Vec<_> = tree.iter().collect();
/// // Items will be ordered: apple, banana, cherry
/// ```
pub struct Iter<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> {
    inner: Box<dyn Iterator<Item = (K, &'a V)> + 'a>,
    _marker: std::marker::PhantomData<(K, P)>,
}

struct IterInner<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> {
    node_iter_stack: Vec<(usize, Box<NodeIterator<'a, P, V>>)>,

    // Pushed and popped with prefix portions as we descend the tree,
    cur_key: K,

    // For seekable iteration: skip keys based on start bound
    start_bound: Option<Bound<K>>,
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> IterInner<'a, K, P, V> {
    pub fn new(node: &'a DefaultNode<P, V>) -> Self {
        let node_iter_stack = vec![(
            node.prefix.len(), /* initial tree depth*/
            node.iter(),       /* root node iter*/
        )];

        Self {
            node_iter_stack,
            cur_key: K::new_from_partial(&node.prefix),
            start_bound: None,
        }
    }

    pub fn new_with_start_bound(node: &'a DefaultNode<P, V>, start_bound: Bound<K>) -> Self {
        let seek_key = match &start_bound {
            Bound::Included(key) | Bound::Excluded(key) => Some(key),
            Bound::Unbounded => None,
        };

        if let Some(seek_key) = seek_key {
            // Build the positioned iterator stack by navigating to the right starting point
            let positioned_stack = Self::build_positioned_stack(node, seek_key, 0);

            // If navigation returns empty, it means this entire tree should be skipped
            // But we still need to return a valid iterator for correctness
            let final_stack = if positioned_stack.is_empty() {
                vec![] // Empty iterator - no results
            } else {
                positioned_stack
            };

            return Self {
                node_iter_stack: final_stack,
                cur_key: K::new_from_partial(&node.prefix),
                start_bound: Some(start_bound.clone()), // Still filter to handle exact boundary cases
            };
        }

        // No seek key means unbounded start, use regular iteration
        let node_iter_stack = vec![(node.prefix.len(), node.iter())];

        Self {
            node_iter_stack,
            cur_key: K::new_from_partial(&node.prefix),
            start_bound: None,
        }
    }

    /// Build positioned iterator stack with O(log N) navigation to starting position
    fn build_positioned_stack(
        node: &'a DefaultNode<P, V>,
        seek_key: &K,
        depth: usize,
    ) -> Vec<(usize, Box<NodeIterator<'a, P, V>>)> {
        // Compare node prefix against seek key segment at this depth.
        let prefix_common = node.prefix.prefix_length_key(seek_key, depth);
        if prefix_common != node.prefix.len() {
            let seek_remaining = seek_key.length_at(depth);
            if prefix_common >= seek_remaining {
                // Seek key is a prefix of this subtree's prefix; whole subtree can be included.
                return vec![(node.prefix.len(), node.iter())];
            }

            let node_byte = node.prefix.at(prefix_common);
            let seek_byte = seek_key.at(depth + prefix_common);

            if node_byte < seek_byte {
                // Entire subtree is below the seek key.
                return vec![];
            }

            // Subtree prefix is above seek key; include subtree from beginning.
            return vec![(node.prefix.len(), node.iter())];
        }

        // Prefix fully matches. If seek key is exhausted at this node, include whole subtree.
        if seek_key.length_at(depth) == node.prefix.len() {
            return vec![(node.prefix.len(), node.iter())];
        }

        // Choose the first child with key-byte >= target.
        let target_depth = depth + node.prefix.len();
        let target_byte = seek_key.at(target_depth);
        let mut iter = node.iter();
        while let Some((k, child)) = iter.next() {
            if k < target_byte {
                continue;
            }

            let positioned_iter: Box<NodeIterator<'a, P, V>> =
                Box::new(std::iter::once((k, child)).chain(iter));
            return vec![(node.prefix.len(), positioned_iter)];
        }

        // No child can satisfy the start bound.
        vec![]
    }
}

impl<'a, K: KeyTrait<PartialType = P> + 'a, P: Partial + 'a, V> Iter<'a, K, P, V> {
    pub(crate) fn new(node: Option<&'a DefaultNode<P, V>>) -> Self {
        let Some(root_node) = node else {
            return Self {
                inner: Box::new(std::iter::empty()),
                _marker: Default::default(),
            };
        };

        // If root is a leaf, we can just return it.
        if root_node.is_leaf() {
            let root_key = K::new_from_partial(&root_node.prefix);
            let root_value = root_node
                .value()
                .expect("corruption: missing data at leaf node during iteration");
            return Self {
                inner: Box::new(std::iter::once((root_key, root_value))),
                _marker: Default::default(),
            };
        }

        Self {
            inner: Box::new(IterInner::<K, P, V>::new(root_node)),
            _marker: Default::default(),
        }
    }

    /// Create an iterator with a start bound for optimized range queries
    pub(crate) fn new_with_start_bound(
        node: Option<&'a DefaultNode<P, V>>,
        start_bound: Bound<K>,
    ) -> Self {
        let Some(root_node) = node else {
            return Self {
                inner: Box::new(std::iter::empty()),
                _marker: Default::default(),
            };
        };

        // If root is a leaf, check if it matches our start bound
        if root_node.is_leaf() {
            let root_key = K::new_from_partial(&root_node.prefix);
            let satisfies_start = match &start_bound {
                Bound::Included(start_key) => root_key.cmp(start_key) >= std::cmp::Ordering::Equal,
                Bound::Excluded(start_key) => root_key.cmp(start_key) > std::cmp::Ordering::Equal,
                Bound::Unbounded => true,
            };

            if satisfies_start {
                let root_value = root_node
                    .value()
                    .expect("corruption: missing data at leaf node during iteration");
                return Self {
                    inner: Box::new(std::iter::once((root_key, root_value))),
                    _marker: Default::default(),
                };
            } else {
                return Self {
                    inner: Box::new(std::iter::empty()),
                    _marker: Default::default(),
                };
            }
        }

        Self {
            inner: Box::new(IterInner::<K, P, V>::new_with_start_bound(
                root_node,
                start_bound,
            )),
            _marker: Default::default(),
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> Iterator for Iter<'a, K, P, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> Iterator for IterInner<'a, K, P, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Get working node iterator off the stack. If there is none, we're done.
            let (tree_depth, last_iter) = self.node_iter_stack.last_mut()?;
            let tree_depth = *tree_depth;

            // Pull the next node from the node iterator. If there's none, pop that iterator off
            // the stack, truncate our working key length back to the parent's depth, return to our
            // parent, and continue there.
            let Some((_k, node)) = last_iter.next() else {
                self.node_iter_stack.pop();
                // Get the parent-depth, and truncate our working key to that depth. If there is no
                // parent, no need to truncate, we'll be done in the next loop
                if let Some((parent_depth, _)) = self.node_iter_stack.last() {
                    self.cur_key = self.cur_key.truncate(*parent_depth);
                };
                continue;
            };

            // We're at a non-exhausted inner node, so go further down the tree by pushing node
            // iterator into the stack. We also extend our working key with this node's prefix.
            if node.is_inner() {
                self.node_iter_stack
                    .push((tree_depth + node.prefix.len(), node.iter()));
                self.cur_key = self.cur_key.extend_from_partial(&node.prefix);
                continue;
            }

            // We've got a value, so tack it onto our working key, and return it. If there's nothing
            // here, that's an issue, leaf nodes should always have values.
            let v = node
                .value()
                .expect("corruption: missing data at leaf node during iteration");
            let key = self.cur_key.extend_from_partial(&node.prefix);

            // Handle start bound filtering
            if let Some(ref start_bound) = self.start_bound
                && !match start_bound {
                    Bound::Included(start_key) => key.cmp(start_key) >= std::cmp::Ordering::Equal,
                    Bound::Excluded(start_key) => key.cmp(start_key) > std::cmp::Ordering::Equal,
                    Bound::Unbounded => true,
                }
            {
                continue; // Skip this key, it doesn't satisfy start bound
            }

            return Some((key, v));
        }
    }
}

/// Iterator over only the values in an Adaptive Radix Tree.
///
/// This iterator skips key reconstruction entirely, only yielding values.
/// It's useful for measuring the overhead of key reconstruction in iteration.
pub struct ValuesIter<'a, P: Partial + 'a, V> {
    node_iter_stack: Vec<Box<NodeIterator<'a, P, V>>>,
}

impl<'a, P: Partial + 'a, V> ValuesIter<'a, P, V> {
    pub(crate) fn new(node: Option<&'a DefaultNode<P, V>>) -> Self {
        let Some(root_node) = node else {
            return Self {
                node_iter_stack: Vec::new(),
            };
        };

        // If root is a leaf, we handle it in the iterator
        Self {
            node_iter_stack: vec![root_node.iter()],
        }
    }
}

impl<'a, P: Partial + 'a, V> Iterator for ValuesIter<'a, P, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Get working node iterator off the stack. If there is none, we're done.
            let last_iter = self.node_iter_stack.last_mut()?;

            // Pull the next node from the node iterator. If there's none, pop that iterator off
            // the stack and continue with the parent.
            let Some((_k, node)) = last_iter.next() else {
                self.node_iter_stack.pop();
                continue;
            };

            // We're at a non-exhausted inner node, so go further down the tree by pushing node
            // iterator into the stack.
            if node.is_inner() {
                self.node_iter_stack.push(node.iter());
                continue;
            }

            // We've got a value, return it directly without any key reconstruction.
            if let Some(value) = node.value() {
                return Some(value);
            }
        }
    }
}
