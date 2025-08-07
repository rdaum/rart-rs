//! Range query implementation for RART.
//!
//! This module provides efficient range iteration over Adaptive Radix Trees,
//! allowing traversal of key-value pairs within specified bounds.
//!
//! Range queries leverage the tree's trie structure to efficiently skip
//! subtrees that fall outside the requested range, providing O(log n)
//! navigation to the start of the range.

use std::collections::Bound;

use crate::keys::KeyTrait;
use crate::node::LeafData;
use crate::partials::Partial;

enum InnerResult<'a, K, V> {
    Iter(Option<(K, &'a V)>),
}

struct RangeInner<'a, K: KeyTrait + 'a, V> {
    current: Option<*mut LeafData<V>>,
    end: Bound<K>,
    _phantom: std::marker::PhantomData<&'a V>,
}

struct RangeInnerNone {}

struct RangeInnerWithBounds<'a, K: KeyTrait + 'a, V> {
    current: Option<*mut LeafData<V>>,
    start: Bound<K>,
    end: Bound<K>,
    _phantom: std::marker::PhantomData<&'a V>,
}

trait RangeInnerTrait<'a, K: KeyTrait + 'a, V> {
    fn next(&mut self) -> InnerResult<'a, K, V>;
}

/// Iterator over key-value pairs within a specified range in an Adaptive Radix Tree.
///
/// This iterator efficiently navigates to the start of the range and then iterates
/// through all key-value pairs that fall within the specified bounds. The iteration
/// leverages the tree's trie structure to skip entire subtrees that fall outside
/// the range, providing O(log n) navigation to the start.
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
/// tree.insert("date", 4);
///
/// // Get all keys from "b" to "d" (exclusive)
/// let start: ArrayKey<16> = "b".into();
/// let end: ArrayKey<16> = "d".into();
/// let range_items: Vec<_> = tree.range(start..end).collect();
/// // Contains: banana, cherry
/// debug_assert_eq!(range_items.len(), 2);
/// ```
pub struct Range<'a, K: KeyTrait + 'a, V> {
    inner: Box<dyn RangeInnerTrait<'a, K, V> + 'a>,
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInnerNone {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        InnerResult::Iter(None)
    }
}

impl<'a, K: KeyTrait, V> RangeInner<'a, K, V> {
    pub fn new(current: Option<*mut LeafData<V>>, end: Bound<K>) -> Self {
        Self {
            current,
            end,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<'a, K: KeyTrait, V> RangeInnerWithBounds<'a, K, V> {
    pub fn new(current: Option<*mut LeafData<V>>, start: Bound<K>, end: Bound<K>) -> Self {
        Self {
            current,
            start,
            end,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInner<'a, K, V> {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        let current = match self.current {
            Some(ptr) => ptr,
            None => return InnerResult::Iter(None),
        };

        unsafe {
            let leaf_data = &*current;
            let key = K::new_from_slice(&leaf_data.key_bytes);
            let value = &leaf_data.value;

            // Check end bound
            let satisfies_end = match &self.end {
                Bound::Included(end_key) => key.cmp(end_key) <= std::cmp::Ordering::Equal,
                Bound::Excluded(end_key) => key.cmp(end_key) < std::cmp::Ordering::Equal,
                Bound::Unbounded => true,
            };

            if !satisfies_end {
                self.current = None;
                return InnerResult::Iter(None);
            }

            // Move to next leaf
            self.current = leaf_data.next;

            InnerResult::Iter(Some((key, value)))
        }
    }
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInnerWithBounds<'a, K, V> {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        loop {
            let current = match self.current {
                Some(ptr) => ptr,
                None => return InnerResult::Iter(None),
            };

            unsafe {
                let leaf_data = &*current;
                let key = K::new_from_slice(&leaf_data.key_bytes);
                let value = &leaf_data.value;

                // Move to next leaf before checking bounds (for next iteration)
                self.current = leaf_data.next;

                // Check start bound
                let satisfies_start = match &self.start {
                    Bound::Included(start_key) => key.cmp(start_key) >= std::cmp::Ordering::Equal,
                    Bound::Excluded(start_key) => key.cmp(start_key) > std::cmp::Ordering::Equal,
                    Bound::Unbounded => true,
                };

                if !satisfies_start {
                    continue; // Skip this key
                }

                // Check end bound
                let satisfies_end = match &self.end {
                    Bound::Included(end_key) => key.cmp(end_key) <= std::cmp::Ordering::Equal,
                    Bound::Excluded(end_key) => key.cmp(end_key) < std::cmp::Ordering::Equal,
                    Bound::Unbounded => true,
                };

                if !satisfies_end {
                    self.current = None;
                    return InnerResult::Iter(None);
                }

                return InnerResult::Iter(Some((key, value)));
            }
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial, V: 'a> Iterator for Range<'a, K, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<(K, &'a V)> {
        match self.inner.next() {
            InnerResult::Iter(i) => i,
        }
    }
}

impl<'a, K: KeyTrait + 'a, V: 'a> Range<'a, K, V> {
    pub fn empty() -> Self {
        Self {
            inner: Box::new(RangeInnerNone {}),
        }
    }

    pub(crate) fn for_linked_list(current: Option<*mut LeafData<V>>, end: Bound<K>) -> Self {
        Self {
            inner: Box::new(RangeInner::new(current, end)),
        }
    }

    pub(crate) fn for_linked_list_with_bounds(
        current: Option<*mut LeafData<V>>,
        start: Bound<K>,
        end: Bound<K>,
    ) -> Self {
        Self {
            inner: Box::new(RangeInnerWithBounds::new(current, start, end)),
        }
    }
}
