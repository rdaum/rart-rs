//! Range query implementation for RART.
//!
//! This module provides efficient range iteration over Adaptive Radix Trees,
//! allowing traversal of key-value pairs within specified bounds.

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

trait RangeInnerTrait<'a, K: KeyTrait + 'a, V> {
    fn next(&mut self) -> InnerResult<'a, K, V>;
}

/// Iterator over key-value pairs within a specified range in an Adaptive Radix Tree.
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

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInner<'a, K, V> {
    fn next(&mut self) -> InnerResult<'a, K, V> {
        let Some(next) = self.iter.next() else {
            return InnerResult::Iter(None);
        };
        match &self.end {
            Bound::Included(end_key) => match next.0.cmp(end_key) {
                std::cmp::Ordering::Less | std::cmp::Ordering::Equal => {
                    InnerResult::Iter(Some(next))
                }
                std::cmp::Ordering::Greater => InnerResult::Iter(None),
            },
            Bound::Excluded(end_key) => match next.0.cmp(end_key) {
                std::cmp::Ordering::Less => InnerResult::Iter(Some(next)),
                std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => InnerResult::Iter(None),
            },
            Bound::Unbounded => InnerResult::Iter(Some(next)),
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
}
