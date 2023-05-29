use std::cmp::min;
use std::ops::Index;
use std::rc::Rc;

use crate::Partial;
use crate::partials::key::Key;

// A prefix root.
pub struct SharedPartialRoot<const SIZE: usize> {
    pub buffer: [u8; SIZE],
    length: usize,
}

impl<const SIZE: usize> SharedPartialRoot<SIZE> {
    // Copy from a slice.
    pub fn from_slice(src: &[u8]) -> SharedPartial<SIZE> {
        let mut data = [0; SIZE];
        data[..src.len()].copy_from_slice(src);
        let root = Rc::new(Self {
            buffer: data,
            length: src.len(),
        });
        SharedPartialRoot::partial(&root)
    }

    pub fn key(src: &[u8]) -> SharedPartial<SIZE> {
        let mut data = [0; SIZE];
        data[..src.len()].copy_from_slice(src);
        data[src.len()] = 0;
        let root = Rc::new(Self {
            buffer: data,
            length: src.len() + 1,
        });
        SharedPartialRoot::partial(&root)
    }

    pub fn slice_for(&self, partial: &SharedPartial<SIZE>) -> &[u8] {
        &self.buffer[partial.offset..partial.offset + partial.length]
    }

    pub fn partial(rc: &Rc<Self>) -> SharedPartial<SIZE> {
        SharedPartial {
            root: rc.clone(),
            offset: 0,
            length: rc.length,
        }
    }

    #[inline]
    pub fn partial_from(rc: &Rc<Self>, offset: usize, length: usize) -> SharedPartial<SIZE> {
        SharedPartial {
            root: rc.clone(),
            offset,
            length,
        }
    }
}

#[derive(Clone)]
pub struct SharedPartial<const SIZE: usize> {
    root: Rc<SharedPartialRoot<SIZE>>,
    offset: usize,
    length: usize,
}

impl<const SIZE: usize> SharedPartial<SIZE> {
    fn to_slice(&self) -> &[u8] {
        &self.root.buffer[self.offset..self.offset + self.length]
    }
}
impl<const SIZE: usize> Index<usize> for SharedPartial<SIZE> {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        self.root.buffer.index(self.offset + index)
    }
}

impl<const SIZE: usize> PartialEq for SharedPartial<SIZE> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
            || Rc::ptr_eq(&self.root, &other.root)
                && self.offset == other.offset
                && self.length == other.length
            || self.to_slice() == other.to_slice()
    }
}
impl<const SIZE: usize> Eq for SharedPartial<SIZE> {}

impl<const SIZE: usize> Partial for SharedPartial<SIZE> {
    #[inline]
    fn partial_before(&self, length: usize) -> Self {
        // Go back to our root and make a new partial relative to us...
        SharedPartialRoot::partial_from(&self.root, self.offset, length)
    }

    #[inline]
    fn partial_from(&self, src_offset: usize, length: usize) -> Self {
        // Go back to our root and make a new partial relative to us...
        SharedPartialRoot::partial_from(&self.root, self.offset + src_offset, length)
    }

    #[inline]
    fn partial_after(&self, start: usize) -> Self {
        SharedPartialRoot::partial_from(&self.root, self.offset + start, self.length - start)
    }

    #[inline]
    fn at(&self, pos: usize) -> u8 {
        self.root.buffer[self.offset + pos]
    }

    fn length(&self) -> usize {
        self.length
    }

    fn prefix_length_common(&self, other: &Self) -> usize {
        if std::ptr::eq(self, other) {
            return self.length;
        }

        if self.length == 0 || other.length == 0 {
            return 0;
        }

        // If we share a common root, and our offsets match, then we can use a fast path.
        if Rc::ptr_eq(&self.root, &other.root) && self.offset == other.offset {
            return min(self.length, other.length);
        }

        let len = min(self.length, other.length);

        let mut idx = 0;
        while idx < len {
            if self.at(idx) != other.at(idx) {
                break;
            }
            idx += 1;
        }
        idx
    }

    fn prefix_length_key<K: Key>(&self, key: &K) -> usize {
        let len = min(self.length, key.length());
        let mut idx = 0;
        while idx < len {
            if self.at(idx) != key.at(idx) {
                break;
            }
            idx += 1;
        }
        idx
    }

    fn prefix_length_slice(&self, key: &[u8]) -> usize {
        let len = min(self.length, key.len());
        let mut idx = 0;
        while idx < len {
            if self.at(idx) != key[idx] {
                break;
            }
            idx += 1;
        }
        idx
    }

    fn to_slice(&self) -> &[u8] {
        self.root.slice_for(self)
    }
}

impl<const SIZE: usize> From<&[u8]> for SharedPartial<SIZE> {
    fn from(src: &[u8]) -> Self {
        SharedPartialRoot::from_slice(src)
    }
}

impl<const SIZE: usize, K: Key> From<K> for SharedPartial<SIZE> {
    fn from(src: K) -> Self {
        SharedPartialRoot::from_slice(src.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partial_before() {
        let arr: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, world!");
        assert_eq!(arr.partial_before(5).to_slice(), b"Hello");
    }

    #[test]
    fn test_partial_from() {
        let arr: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, world!");
        assert_eq!(arr.partial_from(7, 5).to_slice(), b"world");
    }

    #[test]
    fn test_prefix_after() {
        let arr: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, world!");
        assert_eq!(arr.partial_after(7).to_slice(), b"world!");
    }

    #[test]
    fn test_at() {
        let arr: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, world!");
        assert_eq!(arr.at(0), 72);
    }

    #[test]
    fn test_length() {
        let arr: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, world!");
        assert_eq!(arr.length(), 13);
    }

    #[test]
    fn test_prefix_length_common() {
        let arr1: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, world!");
        let arr2: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, there!");
        assert_eq!(arr1.prefix_length_common(&arr2), 7);
    }

    #[test]
    fn test_key() {
        let arr: SharedPartial<16> = SharedPartialRoot::key(b"Hello, world!");
        assert_eq!(
            arr.to_slice(),
            &[72, 101, 108, 108, 111, 44, 32, 119, 111, 114, 108, 100, 33, 0]
        );
    }

    #[test]
    fn test_from_slice() {
        let arr: SharedPartial<16> = SharedPartialRoot::from_slice(b"Hello, world!");
        assert_eq!(arr.to_slice(), b"Hello, world!");
    }

    #[test]
    fn test_partial_chain_with_key() {
        let arr1: SharedPartial<16> = SharedPartialRoot::key(b"Hello, world!");
        let arr2: SharedPartial<16> = SharedPartialRoot::key(b"Hello, there!");
        let partial1 = arr1.partial_before(6);
        assert_eq!(partial1.to_slice(), b"Hello,");
        let partial2 = arr2.partial_from(7, 5);
        assert_eq!(partial2.to_slice(), b"there");
        let partial3 = partial1.partial_after(1);
        assert_eq!(partial3.to_slice(), b"ello,");
        assert_eq!(0, partial3.prefix_length_common(&partial2));
    }
}
