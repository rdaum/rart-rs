pub mod direct_mapping;
pub mod indexed_mapping;
pub mod keyed_mapping;
pub mod sorted_keyed_mapping;

use std::alloc::Allocator;
use std::mem::MaybeUninit;

/// Mappings that can be initialized directly inside their final allocation.
///
/// Mappings store their child slots inline, so a Node256 is several kilobytes. Building one
/// on the stack and moving it into a box would copy all of it; this lets the node allocate
/// first and initialize in place.
pub(crate) trait InitEmpty: Sized {
    /// Initialize `slot` as an empty mapping and return it.
    fn init_empty(slot: &mut MaybeUninit<Self>) -> &mut Self;
}

/// Allocate an empty mapping in `alloc` without materializing it on the stack.
#[inline]
pub(crate) fn boxed_empty_in<M: InitEmpty, A: Allocator>(alloc: A) -> Box<M, A> {
    // Moving `MaybeUninit::uninit()` into the box copies nothing.
    let mut slot = Box::new_in(MaybeUninit::<M>::uninit(), alloc);
    M::init_empty(&mut slot);
    let (raw, alloc) = Box::into_raw_with_allocator(slot);
    // SAFETY: `init_empty` initialized the value, and `MaybeUninit<M>` has the same layout
    // as `M`, so the allocation can be reinterpreted in place with its original allocator.
    unsafe { Box::from_raw_in(raw.cast::<M>(), alloc) }
}

pub trait NodeMapping<N, const NUM_CHILDREN: usize> {
    const NUM_CHILDREN: usize = NUM_CHILDREN;

    fn add_child(&mut self, key: u8, node: N);
    fn seek_child(&self, key: u8) -> Option<&N>;
    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N>;
    fn delete_child(&mut self, key: u8) -> Option<N>;
    fn num_children(&self) -> usize;
    fn width(&self) -> usize {
        Self::NUM_CHILDREN
    }
}
