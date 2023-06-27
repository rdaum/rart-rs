use std::collections::Bound;

use crate::iter::Iter;
use crate::keys::KeyTrait;
use crate::partials::Partial;

enum InnerResult<'a, V> {
    OneMore,
    Iter(Option<(Vec<u8>, &'a V)>),
}

struct RangeInner<'a, K: KeyTrait + 'a, V> {
    iter: Iter<'a, K::PartialType, V>,
    end: Bound<K>,
}

struct RangeInnerNone {}

trait RangeInnerTrait<'a, K: KeyTrait + 'a, V> {
    fn next(&mut self) -> InnerResult<'a, V>;
}

pub struct Range<'a, K: KeyTrait + 'a, V> {
    inner: Box<dyn RangeInnerTrait<'a, K, V> + 'a>,
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInnerNone {
    fn next(&mut self) -> InnerResult<'a, V> {
        InnerResult::Iter(None)
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial, V> RangeInner<'a, K, V> {
    pub fn new(iter: Iter<'a, P, V>, end: Bound<K>) -> Self {
        Self { iter, end }
    }
}

impl<'a, K: KeyTrait + 'a, V> RangeInnerTrait<'a, K, V> for RangeInner<'a, K, V> {
    fn next(&mut self) -> InnerResult<'a, V> {
        let Some(next) = self.iter.next() else {
            return InnerResult::Iter(None)
        };
        let next_key = next.0.as_slice();
        match &self.end {
            Bound::Included(k) if k.matches_slice(next_key) => InnerResult::OneMore,
            Bound::Excluded(k) if k.matches_slice(next_key) => InnerResult::Iter(None),
            Bound::Unbounded => InnerResult::Iter(Some(next)),
            _ => InnerResult::Iter(Some(next)),
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial, V: 'a> Iterator for Range<'a, K, V> {
    type Item = (Vec<u8>, &'a V);

    fn next(&mut self) -> Option<(Vec<u8>, &'a V)> {
        match self.inner.next() {
            InnerResult::OneMore => {
                let r = self.next();
                self.inner = Box::new(RangeInnerNone {});
                r
            }
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

    pub fn for_iter(iter: Iter<'a, K::PartialType, V>, end: Bound<K>) -> Self {
        Self {
            inner: Box::new(RangeInner::new(iter, end)),
        }
    }
}
