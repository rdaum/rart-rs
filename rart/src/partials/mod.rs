use crate::keys::KeyTrait;

pub mod array_partial;
pub mod vector_partial;

pub trait Partial {
    /// Returns a partial up to `length` bytes.
    fn partial_before(&self, length: usize) -> Self;
    /// Returns a partial from `src_offset` onwards with `length` bytes.
    fn partial_from(&self, src_offset: usize, length: usize) -> Self;
    /// Returns a partial from `start` onwards.
    fn partial_after(&self, start: usize) -> Self;
    /// Returns the byte at `pos`.
    fn at(&self, pos: usize) -> u8;
    /// Returns the length of the partial.
    fn len(&self) -> usize;
    /// Returns true if the partial is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Returns the length of the common prefix between `self` and `other`.
    fn prefix_length_common(&self, other: &Self) -> usize;
    /// Returns the length of the common prefix between `self` and `key`.
    fn prefix_length_key<'a, P, K>(&self, key: &'a K, at_depth: usize) -> usize
    where
        P: Partial,
        K: KeyTrait<P> + 'a;
    /// Returns the length of the common prefix between `self` and `slice`.
    fn prefix_length_slice(&self, slice: &[u8]) -> usize;
    /// Return a slice form of the partial. Warning: could take copy, depending on the implementation.
    /// Really just for debugging purposes.
    fn to_slice(&self) -> &[u8];
}
