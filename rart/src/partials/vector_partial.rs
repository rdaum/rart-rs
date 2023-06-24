use std::cmp::min;

use crate::keys::KeyTrait;
use crate::partials::Partial;

pub struct VectorPartial {
    data: Vec<u8>,
}

impl VectorPartial {
    pub fn key(src: &[u8]) -> Self {
        let mut data = Vec::with_capacity(src.len() + 1);
        data.extend_from_slice(src);
        data.push(0);
        Self { data }
    }

    pub fn from_slice(src: &[u8]) -> Self {
        let data = Vec::from(src);
        Self { data }
    }

    pub fn to_slice(&self) -> &[u8] {
        &self.data
    }
}

impl Partial for VectorPartial {
    fn partial_before(&self, length: usize) -> Self {
        let mut data = Vec::with_capacity(length);
        data.extend_from_slice(&self.data[..length]);
        Self { data }
    }

    fn partial_from(&self, src_offset: usize, length: usize) -> Self {
        let mut data = Vec::with_capacity(length);
        data.extend_from_slice(&self.data[src_offset..src_offset + length]);
        Self { data }
    }

    fn partial_after(&self, start: usize) -> Self {
        let mut data = Vec::with_capacity(self.data.len() - start);
        data.extend_from_slice(&self.data[start..]);
        Self { data }
    }

    fn at(&self, pos: usize) -> u8 {
        self.data[pos]
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn prefix_length_common(&self, other: &Self) -> usize {
        self.prefix_length_slice(other.to_slice())
    }

    fn prefix_length_key<'a, P: Partial, K: KeyTrait<P> + 'a>(
        &self,
        key: &'a K,
        at_depth: usize,
    ) -> usize {
        let len = min(self.data.len(), key.length_at(at_depth));
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
        &self.data
    }
}
