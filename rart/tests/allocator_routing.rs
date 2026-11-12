//! Verifies that a tree built in a custom allocator keeps all node storage out of the global
//! heap.
//!
//! A counting `#[global_allocator]` records allocations made on the current thread, so any
//! node storage that bypasses the tree's allocator shows up as a non-zero count.

#![cfg(not(miri))]

use std::alloc::{AllocError, Allocator, GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ptr::NonNull;

use rart::{AdaptiveRadixTree, ArrayKey};

thread_local! {
    static GLOBAL_ALLOCS: Cell<usize> = const { Cell::new(0) };
}

struct CountingGlobal;

// SAFETY: forwards every call unchanged to `System`.
unsafe impl GlobalAlloc for CountingGlobal {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `try_with` keeps this safe during thread teardown.
        let _ = GLOBAL_ALLOCS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: forwarded caller contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded caller contract.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingGlobal = CountingGlobal;

fn global_allocs() -> usize {
    GLOBAL_ALLOCS.with(Cell::get)
}

/// Tree-side allocator: goes straight to `System`, so it never touches the counted global
/// heap itself.
#[derive(Default)]
struct TreeAlloc {
    allocs: Cell<usize>,
    deallocs: Cell<usize>,
}

// SAFETY: blocks come from `System` and are returned to it with the same layout.
unsafe impl Allocator for TreeAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocs.set(self.allocs.get() + 1);
        System.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocs.set(self.deallocs.get() + 1);
        // SAFETY: `ptr` was returned by `System.allocate(layout)` above.
        unsafe { System.deallocate(ptr, layout) }
    }
}

fn dense_key(i: u64) -> ArrayKey<16> {
    i.into()
}

fn sparse_key(i: u64) -> ArrayKey<16> {
    i.wrapping_mul(0x9E37_79B9_7F4A_7C15).into()
}

/// Dense keys reach Node48 and Node256; sparse keys add many Node4 and Node16.
fn keys() -> Vec<ArrayKey<16>> {
    let mut keys: Vec<_> = (0..20_000).map(dense_key).collect();
    keys.extend((0..20_000).map(sparse_key));
    keys.sort();
    keys.dedup();
    keys
}

#[test]
fn incremental_updates_do_not_touch_the_global_heap() {
    let keys = keys();
    let tree_alloc = TreeAlloc::default();

    let before = global_allocs();
    {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, u64, _>::new_in(&tree_alloc);
        for (value, key) in keys.iter().enumerate() {
            tree.insert_k(key, value as u64);
        }
        // Removing most entries walks every shrink transition.
        for key in keys.iter().step_by(2) {
            tree.remove_k(key);
        }
        for key in keys.iter().skip(1).step_by(4) {
            tree.remove_k(key);
        }
        assert_eq!(tree.len(), keys.len() / 4);
    }
    let after = global_allocs();

    assert_eq!(after - before, 0, "node storage leaked to the global heap");
    assert!(tree_alloc.allocs.get() > 0);
    assert_eq!(tree_alloc.allocs.get(), tree_alloc.deallocs.get());
}

#[test]
fn bulk_load_by_index_does_not_touch_the_global_heap() {
    let keys = keys();
    let tree_alloc = TreeAlloc::default();

    let before = global_allocs();
    {
        let tree = AdaptiveRadixTree::bulk_load_sorted_unique_by_index_in(
            keys.len(),
            |index| &keys[index],
            |index| index as u64,
            &tree_alloc,
        );
        assert_eq!(tree.len(), keys.len());
        assert_eq!(
            tree.get_k(&keys[keys.len() / 2]),
            Some(&(keys.len() as u64 / 2))
        );
    }
    let after = global_allocs();

    assert_eq!(after - before, 0, "node storage leaked to the global heap");
    assert!(tree_alloc.allocs.get() > 0);
    assert_eq!(tree_alloc.allocs.get(), tree_alloc.deallocs.get());
}
