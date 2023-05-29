use crate::iter::Iter;
use crate::partials::key::Key;
use crate::tree::PrefixTraits;
use std::collections::Bound;

enum InnerResult<'a, V> {
    OneMore,
    Iter(Option<(Vec<u8>, &'a V)>),
}

struct RangeInner<'a, K: Key + 'a, P: PrefixTraits, V> {
    iter: Iter<'a, P, V>,
    end: Bound<K>,
    _marker: std::marker::PhantomData<P>,
}

struct RangeInnerNone {}

trait RangeInnerTrait<'a, K: Key + 'a, P: PrefixTraits, V> {
    fn next(&mut self) -> InnerResult<'a, V>;
}

pub struct Range<'a, K: Key + 'a, P: PrefixTraits, V> {
    inner: Box<dyn RangeInnerTrait<'a, K, P, V> + 'a>,
}

impl<'a, K: Key + 'a, P: PrefixTraits, V> RangeInnerTrait<'a, K, P, V> for RangeInnerNone {
    fn next(&mut self) -> InnerResult<'a, V> {
        InnerResult::Iter(None)
    }
}

impl<'a, K: Key, P: PrefixTraits, V> RangeInner<'a, K, P, V> {
    pub fn new(iter: Iter<'a, P, V>, end: Bound<K>) -> Self {
        Self {
            iter,
            end,
            _marker: Default::default(),
        }
    }
}

impl<'a, K: Key + 'a, P: PrefixTraits, V> RangeInnerTrait<'a, K, P, V> for RangeInner<'a, K, P, V> {
    fn next(&mut self) -> InnerResult<'a, V> {
        let Some(next) = self.iter.next() else {
            return InnerResult::Iter(None)
        };
        let next_key = next.0.as_slice();
        match &self.end {
            Bound::Included(k) if next_key == k.as_slice() => InnerResult::OneMore,
            Bound::Excluded(k) if next_key == k.as_slice() => InnerResult::Iter(None),
            Bound::Unbounded => InnerResult::Iter(Some(next)),
            _ => InnerResult::Iter(Some(next)),
        }
    }
}

impl<'a, K: Key, P: PrefixTraits, V: 'a> Iterator for Range<'a, K, P, V> {
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

impl<'a, K: Key + 'a, P: PrefixTraits + 'a, V> Range<'a, K, P, V> {
    pub fn empty() -> Self {
        Self {
            inner: Box::new(RangeInnerNone {}),
        }
    }

    pub fn for_iter(iter: Iter<'a, P, V>, end: Bound<K>) -> Self {
        Self {
            inner: Box::new(RangeInner::new(iter, end)),
        }
    }
}
