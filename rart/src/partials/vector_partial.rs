use std::cmp::min;

use crate::keys::KeyTrait;
use crate::keys::vector_key::VectorKey;
use crate::partials::Partial;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VectorPartial {
    data: Box<[u8]>,
}

impl AsRef<[u8]> for VectorPartial {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl VectorPartial {
    pub fn key(src: &[u8]) -> Self {
        let mut data = Vec::with_capacity(src.len() + 1);
        data.extend_from_slice(src);
        data.push(0);
        Self {
            data: data.into_boxed_slice(),
        }
    }

    pub fn from_slice(src: &[u8]) -> Self {
        Self {
            data: Box::from(src),
        }
    }

    pub fn to_slice(&self) -> &[u8] {
        &self.data
    }
}

impl From<&[u8]> for VectorPartial {
    fn from(src: &[u8]) -> Self {
        Self::from_slice(src)
    }
}

impl Partial for VectorPartial {
    fn partial_before(&self, length: usize) -> Self {
        debug_assert!(length <= self.data.len());
        VectorPartial::from_slice(&self.data[..length])
    }

    fn partial_from(&self, src_offset: usize, length: usize) -> Self {
        debug_assert!(src_offset + length <= self.data.len());
        VectorPartial::from_slice(&self.data[src_offset..src_offset + length])
    }

    fn partial_after(&self, start: usize) -> Self {
        debug_assert!(start <= self.data.len());
        VectorPartial::from_slice(&self.data[start..self.data.len()])
    }

    fn partial_extended_with(&self, other: &Self) -> Self {
        let mut data = Vec::with_capacity(self.data.len() + other.data.len());
        data.extend_from_slice(&self.data);
        data.extend_from_slice(&other.data);
        Self {
            data: data.into_boxed_slice(),
        }
    }

    #[inline(always)]
    fn at(&self, pos: usize) -> u8 {
        debug_assert!(pos < self.data.len());
        self.data[pos]
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.data.len()
    }

    fn prefix_length_common(&self, other: &Self) -> usize {
        self.prefix_length_slice(other.to_slice())
    }

    fn prefix_length_key<'a, K>(&self, key: &'a K, at_depth: usize) -> usize
    where
        K: KeyTrait<PartialType = Self> + 'a,
    {
        let len = min(self.data.len(), key.length_at(0));
        let mut idx = 0;
        while idx < len {
            if self.data[idx] != key.at(idx + at_depth) {
                break;
            }
            idx += 1;
        }
        idx
    }

    fn prefix_length_slice(&self, slice: &[u8]) -> usize {
        let len = min(self.data.len(), slice.len());
        let mut idx = 0;
        while idx < len {
            if self.data[idx] != slice[idx] {
                break;
            }
            idx += 1;
        }
        idx
    }

    fn to_slice(&self) -> &[u8] {
        &self.data[..self.data.len()]
    }
}

impl From<VectorKey> for VectorPartial {
    fn from(value: VectorKey) -> Self {
        value.to_partial(0)
    }
}
