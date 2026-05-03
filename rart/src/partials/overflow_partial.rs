use std::cmp::min;
use std::fmt;
use std::ops::Index;

use crate::keys::KeyTrait;
use crate::keys::overflow_key::OverflowKey;
use crate::partials::{Partial, prefix_length_bytes};

/// A partial key fragment with inline storage and boxed overflow.
///
/// This is the associated partial type for [`OverflowKey`]. It stores node prefixes inline when
/// their length is at most `N`, and stores longer prefixes as a boxed slice. `OverflowKey` lets the
/// key inline capacity and partial inline capacity differ, which is often useful because many small
/// partials are stored inside tree nodes.
#[derive(Clone)]
pub struct OverflowPartial<const N: usize> {
    inline: [u8; N],
    len: usize,
    overflow: Option<Box<[u8]>>,
}

impl<const N: usize> OverflowPartial<N> {
    #[inline(always)]
    fn heap_data(&self) -> &[u8] {
        self.overflow
            .as_deref()
            .expect("overflow storage must exist when len exceeds inline capacity")
    }

    #[inline(always)]
    fn data(&self) -> &[u8] {
        if self.len <= N {
            &self.inline[..self.len]
        } else {
            self.heap_data()
        }
    }

    pub fn key(src: &[u8]) -> Self {
        let mut data = Vec::with_capacity(src.len() + 1);
        data.extend_from_slice(src);
        data.push(0);
        Self::from_slice(&data)
    }

    pub fn from_slice(src: &[u8]) -> Self {
        if src.len() <= N {
            let mut inline = [0; N];
            inline[..src.len()].copy_from_slice(src);
            Self {
                inline,
                len: src.len(),
                overflow: None,
            }
        } else {
            Self {
                inline: [0; N],
                len: src.len(),
                overflow: Some(Box::from(src)),
            }
        }
    }

    pub fn is_inline(&self) -> bool {
        self.len <= N
    }

    pub fn to_slice(&self) -> &[u8] {
        self.data()
    }
}

impl<const N: usize> AsRef<[u8]> for OverflowPartial<N> {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.data()
    }
}

impl<const N: usize> fmt::Debug for OverflowPartial<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OverflowPartial")
            .field("data", &self.as_ref())
            .field("inline", &self.is_inline())
            .finish()
    }
}

impl<const N: usize> PartialEq for OverflowPartial<N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<const N: usize> Eq for OverflowPartial<N> {}

impl<const N: usize> Index<usize> for OverflowPartial<N> {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        self.as_ref().index(index)
    }
}

impl<const N: usize> Partial for OverflowPartial<N> {
    fn partial_before(&self, length: usize) -> Self {
        debug_assert!(length <= self.len);
        Self::from_slice(&self.data()[..length])
    }

    fn partial_from(&self, src_offset: usize, length: usize) -> Self {
        debug_assert!(src_offset + length <= self.len);
        Self::from_slice(&self.data()[src_offset..src_offset + length])
    }

    fn partial_after(&self, start: usize) -> Self {
        debug_assert!(start <= self.len);
        Self::from_slice(&self.data()[start..])
    }

    fn partial_extended_with(&self, other: &Self) -> Self {
        let mut data = Vec::with_capacity(self.len + other.len);
        data.extend_from_slice(self.data());
        data.extend_from_slice(other.data());
        Self::from_slice(&data)
    }

    #[inline(always)]
    fn at(&self, pos: usize) -> u8 {
        debug_assert!(pos < self.len);
        if self.len <= N {
            self.inline[pos]
        } else {
            self.heap_data()[pos]
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len
    }

    fn prefix_length_common(&self, other: &Self) -> usize {
        self.prefix_length_slice(other.data())
    }

    fn prefix_length_key<'a, K>(&self, key: &'a K, at_depth: usize) -> usize
    where
        K: KeyTrait<PartialType = Self> + 'a,
    {
        let len = min(self.len, key.length_at(at_depth));
        prefix_length_bytes(&self.data()[..len], &key.as_ref()[at_depth..at_depth + len])
    }

    fn prefix_length_slice(&self, slice: &[u8]) -> usize {
        let len = min(self.len, slice.len());
        prefix_length_bytes(&self.data()[..len], &slice[..len])
    }

    fn to_slice(&self) -> &[u8] {
        self.data()
    }
}

impl<const N: usize> From<&[u8]> for OverflowPartial<N> {
    fn from(src: &[u8]) -> Self {
        Self::from_slice(src)
    }
}

impl<const N: usize, const P: usize> From<OverflowKey<N, P>> for OverflowPartial<P> {
    fn from(value: OverflowKey<N, P>) -> Self {
        value.to_partial(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_and_overflow_match_slices() {
        let short = OverflowPartial::<4>::from_slice(b"abc");
        assert!(short.is_inline());
        assert_eq!(short.to_slice(), b"abc");

        let long = OverflowPartial::<4>::from_slice(b"abcdef");
        assert!(!long.is_inline());
        assert_eq!(long.to_slice(), b"abcdef");
    }

    #[test]
    fn partial_operations_cross_inline_boundary() {
        let partial = OverflowPartial::<4>::from_slice(b"abcdef");
        assert_eq!(partial.partial_before(3).to_slice(), b"abc");
        assert_eq!(partial.partial_from(2, 3).to_slice(), b"cde");
        assert_eq!(partial.partial_after(4).to_slice(), b"ef");
        assert_eq!(
            partial
                .partial_before(4)
                .partial_extended_with(&partial.partial_after(4))
                .to_slice(),
            b"abcdef"
        );
    }
}
