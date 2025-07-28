use crate::iter::Iter;
use crate::keys::KeyTrait;
use crate::range::Range;
use std::ops::RangeBounds;

mod debug_test;
pub mod iter;
pub mod keys;
pub mod mapping;
mod node;
pub mod partials;
pub mod range;
pub mod stats;
pub mod tree;
pub mod utils;

pub trait TreeTrait<KeyType, ValueType>
where
    KeyType: KeyTrait,
{
    type NodeType;

    fn get<Key>(&self, key: Key) -> Option<&ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_k(&key.into())
    }
    fn get_k(&self, key: &KeyType) -> Option<&ValueType>;
    fn get_mut<Key>(&mut self, key: Key) -> Option<&mut ValueType>
    where
        Key: Into<KeyType>,
    {
        self.get_mut_k(&key.into())
    }
    fn get_mut_k(&mut self, key: &KeyType) -> Option<&mut ValueType>;
    fn insert<KV>(&mut self, key: KV, value: ValueType) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.insert_k(&key.into(), value)
    }
    fn insert_k(&mut self, key: &KeyType, value: ValueType) -> Option<ValueType>;

    fn remove<KV>(&mut self, key: KV) -> Option<ValueType>
    where
        KV: Into<KeyType>,
    {
        self.remove_k(&key.into())
    }
    fn remove_k(&mut self, key: &KeyType) -> Option<ValueType>;

    fn iter(&self) -> Iter<'_, KeyType, KeyType::PartialType, ValueType>;

    fn range<'a, R>(&'a self, range: R) -> Range<'a, KeyType, ValueType>
    where
        R: RangeBounds<KeyType> + 'a;

    fn is_empty(&self) -> bool;
}
