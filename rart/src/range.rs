//! Range query implementation for RART.
//!
//! This module provides efficient range iteration over Adaptive Radix Trees,
//! allowing traversal of key-value pairs within specified bounds.
//!
//! Range queries leverage the tree's trie structure to efficiently skip
//! subtrees that fall outside the requested range, providing O(log n)
//! navigation to the start of the range.

use std::collections::Bound;

use crate::iter::Iter;
use crate::keys::KeyTrait;
use crate::partials::Partial;

enum InnerResult<'a, K, V> {
    Iter(Option<(K, &'a V)>),
}

struct RangeInner<'a, K: KeyTrait + 'a, V> {
    iter: Iter<'a, K, K::PartialType, V>,
    end: Bound<K>,
}

struct RangeInnerNone {}

struct RangeInnerWithFirst<'a, K: KeyTrait + 'a, V> {
    first: Option<(K, &'a V)>,
    iter: Iter<'a, K, K::PartialType, V>,
    end: Bound<K>,
}

struct RangeInnerWithBounds<'a, K: KeyTrait + 'a, V> {
    iter: Iter<'a, K, K::PartialType, V>,
    start: Bound<K>,
    end: Bound<K>,
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
/// assert_eq!(range_items.len(), 2);
/// ```
pub struct Range<'a, K: KeyTrait + 'a, V> {
    inner: Box<dyn RangeInnerTrait<'a, K, V> + 'a>,
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInnerNone {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        InnerResult::Iter(None)
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial, V> RangeInner<'a, K, V> {
    pub fn new(iter: Iter<'a, K, P, V>, end: Bound<K>) -> Self {
        Self { iter, end }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial, V> RangeInnerWithFirst<'a, K, V> {
    pub fn new(first: (K, &'a V), iter: Iter<'a, K, P, V>, end: Bound<K>) -> Self {
        Self {
            first: Some(first),
            iter,
            end,
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial, V> RangeInnerWithBounds<'a, K, V> {
    pub fn new(iter: Iter<'a, K, P, V>, start: Bound<K>, end: Bound<K>) -> Self {
        Self { iter, start, end }
    }
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInner<'a, K, V> {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        loop {
            let Some(next) = self.iter.next() else {
                return InnerResult::Iter(None);
            };
            match &self.end {
                Bound::Included(end_key) => match next.0.cmp(end_key) {
                    std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                        return InnerResult::Iter(Some(next));
                    }
                    std::cmp::Ordering::Greater => continue, // Skip and continue iterating
                },
                Bound::Excluded(end_key) => match next.0.cmp(end_key) {
                    std::cmp::Ordering::Less => return InnerResult::Iter(Some(next)),
                    std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => continue, // Skip and continue iterating
                },
                Bound::Unbounded => return InnerResult::Iter(Some(next)),
            }
        }
    }
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInnerWithFirst<'a, K, V> {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        // Handle the first element if it exists
        if let Some(first) = self.first.take() {
            match &self.end {
                Bound::Included(end_key) => match first.0.cmp(end_key) {
                    std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                        return InnerResult::Iter(Some(first));
                    }
                    std::cmp::Ordering::Greater => {
                        // First element doesn't match, continue with rest of iterator
                    }
                },
                Bound::Excluded(end_key) => match first.0.cmp(end_key) {
                    std::cmp::Ordering::Less => return InnerResult::Iter(Some(first)),
                    std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
                        // First element doesn't match, continue with rest of iterator
                    }
                },
                Bound::Unbounded => return InnerResult::Iter(Some(first)),
            }
        }

        // Continue with the rest of the iterator
        loop {
            let Some(next) = self.iter.next() else {
                return InnerResult::Iter(None);
            };
            match &self.end {
                Bound::Included(end_key) => match next.0.cmp(end_key) {
                    std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                        return InnerResult::Iter(Some(next));
                    }
                    std::cmp::Ordering::Greater => continue, // Skip and continue iterating
                },
                Bound::Excluded(end_key) => match next.0.cmp(end_key) {
                    std::cmp::Ordering::Less => return InnerResult::Iter(Some(next)),
                    std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => continue, // Skip and continue iterating
                },
                Bound::Unbounded => return InnerResult::Iter(Some(next)),
            }
        }
    }
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInnerWithBounds<'a, K, V> {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        loop {
            let Some(next) = self.iter.next() else {
                return InnerResult::Iter(None);
            };
            let key = &next.0;

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
            match &self.end {
                Bound::Included(end_key) => match next.0.cmp(end_key) {
                    std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                        return InnerResult::Iter(Some(next));
                    }
                    std::cmp::Ordering::Greater => continue, // Skip and continue iterating
                },
                Bound::Excluded(end_key) => match next.0.cmp(end_key) {
                    std::cmp::Ordering::Less => return InnerResult::Iter(Some(next)),
                    std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => continue, // Skip and continue iterating
                },
                Bound::Unbounded => return InnerResult::Iter(Some(next)),
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

impl<'a, K: KeyTrait + 'a, V> Range<'a, K, V> {
    pub fn empty() -> Self {
        Self {
            inner: Box::new(RangeInnerNone {}),
        }
    }

    pub fn for_iter(iter: Iter<'a, K, K::PartialType, V>, end: Bound<K>) -> Self {
        Self {
            inner: Box::new(RangeInner::new(iter, end)),
        }
    }

    pub fn for_iter_with_first(
        first: (K, &'a V),
        iter: Iter<'a, K, K::PartialType, V>,
        end: Bound<K>,
    ) -> Self {
        Self {
            inner: Box::new(RangeInnerWithFirst::new(first, iter, end)),
        }
    }

    pub fn for_iter_with_bounds(
        iter: Iter<'a, K, K::PartialType, V>,
        start: Bound<K>,
        end: Bound<K>,
    ) -> Self {
        Self {
            inner: Box::new(RangeInnerWithBounds::new(iter, start, end)),
        }
    }
}
