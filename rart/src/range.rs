use std::collections::Bound;

use crate::iter::Iter;
use crate::keys::KeyTrait;
use crate::partials::Partial;

enum InnerResult<'a, K, V> {
    OneMore((K, &'a V)),
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
        let next_key = next.0.clone();
        match &self.end {
            Bound::Included(end_key) if *end_key == next_key => InnerResult::OneMore(next),
            Bound::Excluded(end_key) if *end_key == next_key => InnerResult::Iter(None),
            Bound::Unbounded => InnerResult::Iter(Some(next)),
            _ => InnerResult::Iter(Some(next)),
        }
    }
}

impl<'a, K: KeyTrait<PartialType = P>, P: Partial, V: 'a> Iterator for Range<'a, K, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<(K, &'a V)> {
        match self.inner.next() {
            InnerResult::OneMore(v) => {
                self.inner = Box::new(RangeInnerNone {});
                Some(v)
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

    pub fn for_iter(iter: Iter<'a, K, K::PartialType, V>, end: Bound<K>) -> Self {
        Self {
            inner: Box::new(RangeInner::new(iter, end)),
        }
    }
}
