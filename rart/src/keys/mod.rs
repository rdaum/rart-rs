use crate::partials::Partial;

pub mod array_key;
pub mod vector_key;

pub trait KeyTrait<Prefix>: Clone
where
    Prefix: Partial,
{
    fn at(&self, pos: usize) -> u8;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn to_prefix(&self, at_depth: usize) -> Prefix;
    fn matches_slice(&self, slice: &[u8]) -> bool;
}
