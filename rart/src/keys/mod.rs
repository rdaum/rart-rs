use crate::partials::Partial;

pub mod array_key;
pub mod vector_key;

pub trait KeyTrait: Clone + PartialEq + Eq {
    type PartialType: Partial + From<Self> + Clone + PartialEq;

    const MAXIMUM_SIZE: Option<usize>;

    fn new_from_slice(slice: &[u8]) -> Self;
    fn new_from_partial(partial: &Self::PartialType) -> Self;

    fn extend_from_partial(&self, partial: &Self::PartialType) -> Self;
    fn truncate(&self, at_depth: usize) -> Self;
    fn at(&self, pos: usize) -> u8;
    fn length_at(&self, at_depth: usize) -> usize;
    fn to_partial(&self, at_depth: usize) -> Self::PartialType;
    fn matches_slice(&self, slice: &[u8]) -> bool;
}
