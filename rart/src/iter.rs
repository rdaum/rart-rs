//! Iterator implementation for RART.
//!
//! This module provides iteration capabilities for Adaptive Radix Trees, allowing
//! traversal of all key-value pairs in lexicographic order.
//!
//! The iterator is designed to be memory-efficient and performs lazy evaluation,
//! only visiting nodes as needed during iteration.


use crate::keys::KeyTrait;
use crate::node::{DefaultNode, LeafData, Node};
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

/// Fast iterator that uses the linked list of leaf nodes.
///
/// This iterator traverses the linked list of leaves directly, providing O(1) per-element
/// iteration performance instead of tree traversal.
pub struct LinkedIterator<'a, K: KeyTrait, V> {
    current: Option<*mut LeafData<V>>,
    _phantom: std::marker::PhantomData<(&'a K, &'a V)>,
}

impl<'a, K: KeyTrait, V> LinkedIterator<'a, K, V> {
    /// Create a new LinkedIterator starting from the given leaf.
    ///
    /// Safety: The caller must ensure that the pointer is valid for the lifetime 'a
    /// and that the linked list is properly maintained.
    pub(crate) unsafe fn new(start: Option<*mut LeafData<V>>) -> Self {
        Self {
            current: start,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<'a, K: KeyTrait, V> Iterator for LinkedIterator<'a, K, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;

        unsafe {
            let leaf_data = &*current;
            let value = &leaf_data.value;

            // Reconstruct the key from stored bytes
            let key = K::new_from_slice(&leaf_data.key_bytes);

            // Move to next leaf
            self.current = leaf_data.next;

            Some((key, value))
        }
    }
}
