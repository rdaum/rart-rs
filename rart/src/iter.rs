//! Iterator implementation for RART.
//!
//! This module provides iteration capabilities for Adaptive Radix Trees, allowing
//! traversal of all key-value pairs in lexicographic order.
//!
//! The iterator is designed to be memory-efficient and performs lazy evaluation,
//! only visiting nodes as needed during iteration.

use std::collections::Bound;

use crate::keys::KeyTrait;
use crate::node::{DefaultNode, Node, NodeIter};
use crate::partials::Partial;

type IterEntry<'a, P, V> = (u8, &'a DefaultNode<P, V>);

enum IterFrameIter<'a, P: Partial, V> {
    Plain(NodeIter<'a, P, V>),
    Leading {
        first: Option<IterEntry<'a, P, V>>,
        rest: NodeIter<'a, P, V>,
    },
}

impl<'a, P: Partial, V> Iterator for IterFrameIter<'a, P, V> {
    type Item = IterEntry<'a, P, V>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterFrameIter::Plain(iter) => iter.next(),
            IterFrameIter::Leading { first, rest } => first.take().or_else(|| rest.next()),
        }
    }
}

/// A lending borrowed view over a reconstructed ART key.
///
/// This borrows the segment list container itself from the traversal scratch
/// state, so it is only valid for the duration of the callback invocation that
/// receives it.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LendingKeyView<'tree, 'view> {
    segments: &'view [&'tree [u8]],
    len: usize,
}

impl<'tree, 'view> LendingKeyView<'tree, 'view> {
    pub(crate) fn new(segments: &'view [&'tree [u8]], len: usize) -> Self {
        Self { segments, len }
    }

    pub fn segments(&self) -> &[&'tree [u8]] {
        self.segments
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.segments
            .iter()
            .flat_map(|segment| segment.iter().copied())
    }

    pub fn write_into(&self, dst: &mut Vec<u8>) {
        dst.reserve(self.len);
        for segment in self.segments {
            dst.extend_from_slice(segment);
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.len);
        self.write_into(&mut out);
        out
    }

    pub fn to_key<K: KeyTrait>(&self) -> K {
        let key = self.to_vec();
        K::new_from_slice(&key)
    }

    pub fn eq_slice(&self, slice: &[u8]) -> bool {
        if self.len != slice.len() {
            return false;
        }

        let mut offset = 0usize;
        for segment in self.segments {
            let end = offset + segment.len();
            if slice[offset..end] != **segment {
                return false;
            }
            offset = end;
        }
        true
    }

    pub fn cmp_slice(&self, slice: &[u8]) -> std::cmp::Ordering {
        let mut offset = 0usize;
        for segment in self.segments {
            let remaining = &slice[offset..];
            let common = segment.len().min(remaining.len());
            match segment[..common].cmp(&remaining[..common]) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }

            if segment.len() != common {
                return std::cmp::Ordering::Greater;
            }
            if remaining.len() != common {
                return std::cmp::Ordering::Less;
            }
            offset += common;
        }

        self.len.cmp(&slice.len())
    }
}

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

/// Iterator over stored keys that are prefixes of a probe key.
///
/// This iterator follows only the path described by the probe key and yields
/// matching stored keys from shortest to longest.
pub struct PrefixMatchIter<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> {
    cur_node: Option<&'a DefaultNode<P, V>>,
    probe: K,
    cur_key: Vec<u8>,
    depth: usize,
}

struct IterInner<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> {
    node_iter_stack: Vec<(usize, IterFrameIter<'a, P, V>)>,

    // Pushed and popped with prefix portions as we descend the tree.
    // We materialize `K` only when yielding, which avoids repeated owned-key
    // rebuilds for heap-backed key types during traversal.
    cur_key: Vec<u8>,

    // For seekable iteration: skip keys based on start bound
    start_bound: Option<Bound<K>>,
}

pub(crate) struct LendingIterInner<'a, P: Partial + 'a, V> {
    node_iter_stack: Vec<(usize, usize, IterFrameIter<'a, P, V>)>,
    cur_segments: Vec<&'a [u8]>,
    cur_len: usize,
    end_bound: Option<(Vec<u8>, bool)>,
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> IterInner<'a, K, P, V> {
    #[inline]
    fn key_order(lhs: &K, rhs: &K) -> std::cmp::Ordering {
        let lhs_len = lhs.length_at(0);
        let rhs_len = rhs.length_at(0);
        let common = lhs_len.min(rhs_len);
        for i in 0..common {
            match lhs.at(i).cmp(&rhs.at(i)) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }
        }
        lhs_len.cmp(&rhs_len)
    }

    fn from_node_and_key(node: &'a DefaultNode<P, V>, cur_key: K) -> Self {
        let node_iter_stack = vec![(
            cur_key.length_at(0),              /* initial absolute tree depth */
            IterFrameIter::Plain(node.iter()), /* root node iter */
        )];
        Self {
            node_iter_stack,
            cur_key: cur_key.as_ref().to_vec(),
            start_bound: None,
        }
    }

    pub fn new(node: &'a DefaultNode<P, V>) -> Self {
        Self::from_node_and_key(node, K::new_from_partial(&node.prefix))
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
                cur_key: node.prefix.as_ref().to_vec(),
                start_bound: Some(start_bound.clone()),
            };
        }

        // No seek key means unbounded start, use regular iteration
        let node_iter_stack = vec![(node.prefix.len(), IterFrameIter::Plain(node.iter()))];

        Self {
            node_iter_stack,
            cur_key: node.prefix.as_ref().to_vec(),
            start_bound: None,
        }
    }

    /// Build positioned iterator stack with O(log N) navigation to starting position
    fn build_positioned_stack(
        node: &'a DefaultNode<P, V>,
        seek_key: &K,
        depth: usize,
    ) -> Vec<(usize, IterFrameIter<'a, P, V>)> {
        // Compare node prefix against seek key segment at this depth.
        let prefix_common = node.prefix.prefix_length_key(seek_key, depth);
        if prefix_common != node.prefix.len() {
            let seek_remaining = seek_key.length_at(depth);
            if prefix_common >= seek_remaining {
                // Seek key is a prefix of this subtree's prefix; whole subtree can be included.
                return vec![(node.prefix.len(), IterFrameIter::Plain(node.iter()))];
            }

            let node_byte = node.prefix.at(prefix_common);
            let seek_byte = seek_key.at(depth + prefix_common);

            if node_byte < seek_byte {
                // Entire subtree is below the seek key.
                return vec![];
            }

            // Subtree prefix is above seek key; include subtree from beginning.
            return vec![(node.prefix.len(), IterFrameIter::Plain(node.iter()))];
        }

        // Prefix fully matches. If seek key is exhausted at this node, include whole subtree.
        if seek_key.length_at(depth) == node.prefix.len() {
            return vec![(node.prefix.len(), IterFrameIter::Plain(node.iter()))];
        }

        // Choose the first child with key-byte >= target.
        let target_depth = depth + node.prefix.len();
        let target_byte = seek_key.at(target_depth);
        let mut iter = node.iter();
        while let Some((k, child)) = iter.next() {
            if k < target_byte {
                continue;
            }

            let positioned_iter = IterFrameIter::Leading {
                first: Some((k, child)),
                rest: iter,
            };
            return vec![(node.prefix.len(), positioned_iter)];
        }

        // No child can satisfy the start bound.
        vec![]
    }
}

impl<'a, K: KeyTrait<PartialType = P> + 'a, P: Partial + 'a, V> Iter<'a, K, P, V> {
    fn from_root_and_children(
        root_key: K,
        root_value: Option<&'a V>,
        children: IterInner<'a, K, P, V>,
    ) -> Self {
        let inner: Box<dyn Iterator<Item = (K, &'a V)> + 'a> = match root_value {
            Some(value) => Box::new(std::iter::once((root_key, value)).chain(children)),
            None => Box::new(children),
        };

        Self {
            inner,
            _marker: Default::default(),
        }
    }

    pub(crate) fn new(node: Option<&'a DefaultNode<P, V>>) -> Self {
        let Some(root_node) = node else {
            return Self {
                inner: Box::new(std::iter::empty()),
                _marker: Default::default(),
            };
        };

        let root_key = K::new_from_partial(&root_node.prefix);
        let root_value = root_node.value();

        if root_node.is_leaf() {
            return Self {
                inner: Box::new(std::iter::once((
                    root_key,
                    root_value.expect("corruption: missing data at leaf node during iteration"),
                ))),
                _marker: Default::default(),
            };
        }

        Self::from_root_and_children(root_key, root_value, IterInner::<K, P, V>::new(root_node))
    }

    /// Create an iterator from a subtree root with a fully-qualified key for that root node.
    pub(crate) fn new_with_prefix(node: Option<&'a DefaultNode<P, V>>, root_key: K) -> Self {
        let Some(root_node) = node else {
            return Self {
                inner: Box::new(std::iter::empty()),
                _marker: Default::default(),
            };
        };

        let root_value = root_node.value();

        if root_node.is_leaf() {
            return Self {
                inner: Box::new(std::iter::once((
                    root_key,
                    root_value.expect("corruption: missing data at leaf node during iteration"),
                ))),
                _marker: Default::default(),
            };
        }

        Self::from_root_and_children(
            root_key.clone(),
            root_value,
            IterInner::<K, P, V>::from_node_and_key(root_node, root_key),
        )
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

        let root_key = K::new_from_partial(&root_node.prefix);
        let root_value = root_node.value();
        let satisfies_start = match &start_bound {
            Bound::Included(start_key) => {
                IterInner::<K, P, V>::key_order(&root_key, start_key) >= std::cmp::Ordering::Equal
            }
            Bound::Excluded(start_key) => {
                IterInner::<K, P, V>::key_order(&root_key, start_key) > std::cmp::Ordering::Equal
            }
            Bound::Unbounded => true,
        };

        // If root is a leaf, check if it matches our start bound
        if root_node.is_leaf() {
            if satisfies_start {
                return Self {
                    inner: Box::new(std::iter::once((
                        root_key,
                        root_value.expect("corruption: missing data at leaf node during iteration"),
                    ))),
                    _marker: Default::default(),
                };
            }

            return Self {
                inner: Box::new(std::iter::empty()),
                _marker: Default::default(),
            };
        }

        let children = IterInner::<K, P, V>::new_with_start_bound(root_node, start_bound.clone());
        if satisfies_start {
            return Self::from_root_and_children(root_key, root_value, children);
        }

        Self {
            inner: Box::new(children),
            _marker: Default::default(),
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> PrefixMatchIter<'a, K, P, V> {
    pub(crate) fn new(node: Option<&'a DefaultNode<P, V>>, probe: K) -> Self {
        Self {
            cur_node: node,
            probe,
            cur_key: Vec::new(),
            depth: 0,
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial + 'a, V> Iterator
    for PrefixMatchIter<'a, K, P, V>
{
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cur_node = self.cur_node.take()?;
            let remaining_len = self.probe.length_at(self.depth);
            let prefix_len = cur_node.prefix.len();
            let prefix_common_match = cur_node.prefix.prefix_length_key(&self.probe, self.depth);

            if prefix_common_match != prefix_len {
                return None;
            }

            self.cur_key.extend_from_slice(cur_node.prefix.as_ref());
            self.depth += prefix_len;

            self.cur_node = if prefix_len == remaining_len {
                None
            } else {
                cur_node.seek_child(self.probe.at(self.depth))
            };

            if let Some(value) = cur_node.value() {
                return Some((K::new_from_slice(&self.cur_key), value));
            }
        }
    }
}

impl<'a, P: Partial + 'a, V> LendingIterInner<'a, P, V> {
    fn cmp_segments_to_slice(segments: &[&[u8]], len: usize, slice: &[u8]) -> std::cmp::Ordering {
        let mut offset = 0usize;
        for segment in segments {
            let remaining = &slice[offset..];
            let common = segment.len().min(remaining.len());
            match segment[..common].cmp(&remaining[..common]) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }

            if segment.len() != common {
                return std::cmp::Ordering::Greater;
            }
            if remaining.len() != common {
                return std::cmp::Ordering::Less;
            }
            offset += common;
        }

        len.cmp(&slice.len())
    }

    fn within_end_bound(&self) -> bool {
        let Some((end_key, inclusive)) = self.end_bound.as_ref() else {
            return true;
        };

        match Self::cmp_segments_to_slice(&self.cur_segments, self.cur_len, end_key) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Equal => *inclusive,
            std::cmp::Ordering::Greater => false,
        }
    }

    fn lending_view<'view>(&'view self) -> LendingKeyView<'a, 'view> {
        LendingKeyView::new(&self.cur_segments, self.cur_len)
    }

    fn visit_each<F>(&mut self, on_each: &mut F)
    where
        F: for<'view> FnMut(LendingKeyView<'a, 'view>, &'a V),
    {
        loop {
            let next = {
                let (segment_depth, key_len, last_iter) = match self.node_iter_stack.last_mut() {
                    Some(v) => v,
                    None => return,
                };
                let segment_depth = *segment_depth;
                let key_len = *key_len;
                self.cur_segments.truncate(segment_depth);
                self.cur_len = key_len;

                let Some((_k, node)) = last_iter.next() else {
                    self.node_iter_stack.pop();
                    if let Some((parent_segment_depth, parent_key_len, _)) =
                        self.node_iter_stack.last()
                    {
                        self.cur_segments.truncate(*parent_segment_depth);
                        self.cur_len = *parent_key_len;
                    }
                    continue;
                };

                let segment = node.prefix.as_ref();
                if !segment.is_empty() {
                    self.cur_segments.push(segment);
                    self.cur_len += segment.len();
                }

                let is_inner = node.is_inner();
                if is_inner {
                    self.node_iter_stack.push((
                        self.cur_segments.len(),
                        self.cur_len,
                        IterFrameIter::Plain(node.iter()),
                    ));
                }

                Some((segment, is_inner, node.value()))
            };

            let Some((segment, is_inner, value)) = next else {
                continue;
            };

            if let Some(value) = value {
                if !self.within_end_bound() {
                    self.node_iter_stack.clear();
                    self.cur_segments.clear();
                    self.cur_len = 0;
                    return;
                }
                on_each(self.lending_view(), value);
            }

            if !is_inner && !segment.is_empty() {
                self.cur_segments.pop();
                self.cur_len -= segment.len();
            }
        }
    }

    fn build_positioned_stack<K: KeyTrait<PartialType = P>>(
        node: &'a DefaultNode<P, V>,
        seek_key: &K,
        depth: usize,
    ) -> Vec<(usize, usize, IterFrameIter<'a, P, V>)> {
        let root_segment_depth = usize::from(!node.prefix.as_ref().is_empty());

        let prefix_common = node.prefix.prefix_length_key(seek_key, depth);
        if prefix_common != node.prefix.len() {
            let seek_remaining = seek_key.length_at(depth);
            if prefix_common >= seek_remaining {
                return vec![(
                    root_segment_depth,
                    node.prefix.len(),
                    IterFrameIter::Plain(node.iter()),
                )];
            }

            let node_byte = node.prefix.at(prefix_common);
            let seek_byte = seek_key.at(depth + prefix_common);

            if node_byte < seek_byte {
                return vec![];
            }

            return vec![(
                root_segment_depth,
                node.prefix.len(),
                IterFrameIter::Plain(node.iter()),
            )];
        }

        if seek_key.length_at(depth) == node.prefix.len() {
            return vec![(
                root_segment_depth,
                node.prefix.len(),
                IterFrameIter::Plain(node.iter()),
            )];
        }

        let target_depth = depth + node.prefix.len();
        let target_byte = seek_key.at(target_depth);
        let mut iter = node.iter();
        while let Some((k, child)) = iter.next() {
            if k < target_byte {
                continue;
            }

            let positioned_iter = IterFrameIter::Leading {
                first: Some((k, child)),
                rest: iter,
            };
            return vec![(root_segment_depth, node.prefix.len(), positioned_iter)];
        }

        vec![]
    }
}

#[allow(dead_code)]
impl<'a, P: Partial + 'a, V> LendingIterInner<'a, P, V> {
    pub(crate) fn for_each<F>(node: Option<&'a DefaultNode<P, V>>, mut on_each: F)
    where
        F: for<'view> FnMut(LendingKeyView<'a, 'view>, &'a V),
    {
        let Some(root_node) = node else {
            return;
        };

        let root_segments = if root_node.prefix.is_empty() {
            Vec::new()
        } else {
            vec![root_node.prefix.as_ref()]
        };
        let root_len = root_node.prefix.len();

        if let Some(value) = root_node.value() {
            on_each(LendingKeyView::new(&root_segments, root_len), value);
        }

        if root_node.is_inner() {
            let mut inner = Self {
                node_iter_stack: vec![(
                    root_segments.len(),
                    root_len,
                    IterFrameIter::Plain(root_node.iter()),
                )],
                cur_segments: root_segments,
                cur_len: root_len,
                end_bound: None,
            };
            inner.visit_each(&mut on_each);
        }
    }

    pub(crate) fn for_each_with_prefix<F>(
        node: Option<&'a DefaultNode<P, V>>,
        root_segments: Vec<&'a [u8]>,
        root_len: usize,
        mut on_each: F,
    ) where
        F: for<'view> FnMut(LendingKeyView<'a, 'view>, &'a V),
    {
        let Some(root_node) = node else {
            return;
        };

        if let Some(value) = root_node.value() {
            on_each(LendingKeyView::new(&root_segments, root_len), value);
        }

        if root_node.is_inner() {
            let mut inner = Self {
                node_iter_stack: vec![(
                    root_segments.len(),
                    root_len,
                    IterFrameIter::Plain(root_node.iter()),
                )],
                cur_segments: root_segments,
                cur_len: root_len,
                end_bound: None,
            };
            inner.visit_each(&mut on_each);
        }
    }

    pub(crate) fn for_each_with_bounds<K, F>(
        node: Option<&'a DefaultNode<P, V>>,
        start_bound: Bound<K>,
        end_bound: Bound<K>,
        mut on_each: F,
    ) where
        K: KeyTrait<PartialType = P>,
        F: for<'view> FnMut(LendingKeyView<'a, 'view>, &'a V),
    {
        let Some(root_node) = node else {
            return;
        };

        let end_bound_vec = match end_bound {
            Bound::Included(key) => Some((key.as_ref().to_vec(), true)),
            Bound::Excluded(key) => Some((key.as_ref().to_vec(), false)),
            Bound::Unbounded => None,
        };

        let root_segments = if root_node.prefix.is_empty() {
            Vec::new()
        } else {
            vec![root_node.prefix.as_ref()]
        };
        let root_len = root_node.prefix.len();
        let root_view = LendingKeyView::new(&root_segments, root_len);
        let satisfies_start = match &start_bound {
            Bound::Included(start_key) => {
                root_view.cmp_slice(start_key.as_ref()) >= std::cmp::Ordering::Equal
            }
            Bound::Excluded(start_key) => {
                root_view.cmp_slice(start_key.as_ref()) > std::cmp::Ordering::Equal
            }
            Bound::Unbounded => true,
        };
        let satisfies_end = match end_bound_vec.as_ref() {
            Some((end_key, inclusive)) => match root_view.cmp_slice(end_key) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => *inclusive,
                std::cmp::Ordering::Greater => false,
            },
            None => true,
        };

        if !satisfies_end {
            return;
        }

        if let Some(value) = root_node.value()
            && satisfies_start
        {
            on_each(root_view, value);
        }

        if !root_node.is_inner() {
            return;
        }

        let seek_key = match &start_bound {
            Bound::Included(key) | Bound::Excluded(key) => Some(key),
            Bound::Unbounded => None,
        };

        let mut inner = if let Some(seek_key) = seek_key {
            let positioned_stack = Self::build_positioned_stack(root_node, seek_key, 0);
            let final_stack = if positioned_stack.is_empty() {
                vec![]
            } else {
                positioned_stack
            };
            Self {
                node_iter_stack: final_stack,
                cur_segments: root_segments,
                cur_len: root_len,
                end_bound: end_bound_vec,
            }
        } else {
            Self {
                node_iter_stack: vec![(
                    root_segments.len(),
                    root_len,
                    IterFrameIter::Plain(root_node.iter()),
                )],
                cur_segments: root_segments,
                cur_len: root_len,
                end_bound: end_bound_vec,
            }
        };

        inner.visit_each(&mut on_each);
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
            self.cur_key.truncate(tree_depth);

            // Pull the next node from the node iterator. If there's none, pop that iterator off
            // the stack, truncate our working key length back to the parent's depth, return to our
            // parent, and continue there.
            let Some((_k, node)) = last_iter.next() else {
                self.node_iter_stack.pop();
                // Get the parent-depth, and truncate our working key to that depth. If there is no
                // parent, no need to truncate, we'll be done in the next loop
                if let Some((parent_depth, _)) = self.node_iter_stack.last() {
                    self.cur_key.truncate(*parent_depth);
                };
                continue;
            };

            self.cur_key.extend_from_slice(node.prefix.as_ref());

            let is_inner = node.is_inner();
            if is_inner {
                self.node_iter_stack.push((
                    tree_depth + node.prefix.len(),
                    IterFrameIter::Plain(node.iter()),
                ));
            }

            if let Some(v) = node.value() {
                let key = K::new_from_slice(&self.cur_key);
                // Handle start bound filtering. Once we yield a key that satisfies the start bound,
                // all subsequent keys will also satisfy it due to sorted iteration order.
                if let Some(start_bound) = self.start_bound.as_ref() {
                    let satisfies_start = match start_bound {
                        Bound::Included(start_key) => {
                            IterInner::<K, P, V>::key_order(&key, start_key)
                                >= std::cmp::Ordering::Equal
                        }
                        Bound::Excluded(start_key) => {
                            IterInner::<K, P, V>::key_order(&key, start_key)
                                > std::cmp::Ordering::Equal
                        }
                        Bound::Unbounded => true,
                    };
                    if !satisfies_start {
                        continue;
                    }
                    self.start_bound = None;
                }
                return Some((key, v));
            }

            if !is_inner {
                self.cur_key.truncate(tree_depth);
            }
            continue;
        }
    }
}

/// Iterator over only the values in an Adaptive Radix Tree.
///
/// This iterator skips key reconstruction entirely, only yielding values.
/// It's useful for measuring the overhead of key reconstruction in iteration.
pub struct ValuesIter<'a, P: Partial + 'a, V> {
    root_value: Option<&'a V>,
    node_iter_stack: Vec<NodeIter<'a, P, V>>,
}

impl<'a, P: Partial + 'a, V> ValuesIter<'a, P, V> {
    pub(crate) fn new(node: Option<&'a DefaultNode<P, V>>) -> Self {
        let Some(root_node) = node else {
            return Self {
                root_value: None,
                node_iter_stack: Vec::new(),
            };
        };

        Self {
            root_value: root_node.value(),
            node_iter_stack: vec![root_node.iter()],
        }
    }
}

impl<'a, P: Partial + 'a, V> Iterator for ValuesIter<'a, P, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.root_value.take() {
            return Some(value);
        }

        loop {
            // Get working node iterator off the stack. If there is none, we're done.
            let last_iter = self.node_iter_stack.last_mut()?;

            // Pull the next node from the node iterator. If there's none, pop that iterator off
            // the stack and continue with the parent.
            let Some((_k, node)) = last_iter.next() else {
                self.node_iter_stack.pop();
                continue;
            };

            if node.is_inner() {
                self.node_iter_stack.push(node.iter());
            }

            if let Some(value) = node.value() {
                return Some(value);
            }
        }
    }
}
