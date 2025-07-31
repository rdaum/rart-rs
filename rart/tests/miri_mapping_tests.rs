//! Miri tests for mapping layer memory safety.
//!
//! These tests focus on the low-level memory operations in the mapping layer
//! that are most likely to have memory safety issues under Miri's strict checking.
//!
//! Note: These tests are only compiled when SIMD features are disabled,
//! as Miri cannot handle SIMD instructions.

#![cfg(not(feature = "simd_keys"))]

use rart::mapping::{
    NodeMapping, direct_mapping::DirectMapping, indexed_mapping::IndexedMapping,
    sorted_keyed_mapping::SortedKeyedMapping,
};
use rart::utils::bitset::Bitset64;

/// Test DirectMapping for memory safety issues around BitArray operations
#[test]
fn miri_direct_mapping_basic_ops() {
    let mut dm = DirectMapping::<i32>::new();

    // Add children across the full range to test BitArray bounds
    for i in 0..=255u8 {
        dm.add_child(i, i as i32);
        assert_eq!(*dm.seek_child(i).unwrap(), i as i32);
    }

    // Test mutable access
    for i in 0..=255u8 {
        *dm.seek_child_mut(i).unwrap() = (i as i32) * 2;
        assert_eq!(*dm.seek_child(i).unwrap(), (i as i32) * 2);
    }

    // Delete and verify cleanup
    for i in 0..=255u8 {
        assert_eq!(dm.delete_child(i), Some((i as i32) * 2));
        assert_eq!(dm.seek_child(i), None);
    }

    assert_eq!(dm.num_children(), 0);
}

/// Test DirectMapping iterator for potential use-after-free issues
#[test]
fn miri_direct_mapping_iterator() {
    let mut dm = DirectMapping::<Box<i32>>::new();

    // Use Box to make ownership issues more visible to Miri
    for i in 0..10u8 {
        dm.add_child(i, Box::new(i as i32));
    }

    // Test iterator that consumes via into_iter
    let items: Vec<_> = dm.into_iter().collect();
    assert_eq!(items.len(), 10);

    for (key, boxed_val) in items {
        assert_eq!(*boxed_val, key as i32);
    }
}

/// Test IndexedMapping for memory safety around MaybeUninit usage
#[test]
fn miri_indexed_mapping_basic_ops() {
    let mut im = IndexedMapping::<i32, 48, Bitset64<1>>::new();

    // Fill to capacity to stress the internal arrays
    for i in 0..48u8 {
        im.add_child(i, i as i32);
        assert_eq!(*im.seek_child(i).unwrap(), i as i32);
    }

    // Test mutable access
    for i in 0..48u8 {
        *im.seek_child_mut(i).unwrap() = (i as i32) * 3;
        assert_eq!(*im.seek_child(i).unwrap(), (i as i32) * 3);
    }

    // Delete half, then add new ones to test array reuse
    for i in 0..24u8 {
        assert_eq!(im.delete_child(i), Some((i as i32) * 3));
        assert_eq!(im.seek_child(i), None);
    }

    // Add new values in the deleted slots
    for i in 0..24u8 {
        im.add_child(i, (i as i32) * 5);
        assert_eq!(*im.seek_child(i).unwrap(), (i as i32) * 5);
    }
}

/// Test the dangerous MaybeUninit usage in from_sorted_keyed conversion
#[test]
fn miri_indexed_mapping_from_sorted_keyed() {
    let mut skm = SortedKeyedMapping::<Box<i32>, 16>::new();

    // Add some boxed values
    for i in 0..10u8 {
        skm.add_child(i, Box::new(i as i32));
    }

    // This conversion uses MaybeUninit and assume_init() - potential UB
    let im = IndexedMapping::<Box<i32>, 48, Bitset64<1>>::from_sorted_keyed(&mut skm);

    // Verify the conversion worked correctly
    for i in 0..10u8 {
        assert_eq!(**im.seek_child(i).unwrap(), i as i32);
    }

    // Original should be emptied
    assert_eq!(skm.num_children(), 0);
}

/// Test IndexedMapping iterator with owned values to catch use-after-free
#[test]
fn miri_indexed_mapping_iterator() {
    let mut im = IndexedMapping::<String, 48, Bitset64<1>>::new();

    // Use String to test proper Drop handling
    for i in 0..20u8 {
        im.add_child(i, format!("value_{}", i));
    }

    let items: Vec<_> = im.into_iter().collect();
    assert_eq!(items.len(), 20);

    for (key, string_val) in items {
        assert_eq!(string_val, format!("value_{}", key));
    }
}

/// Test SortedKeyedMapping for potential array bounds issues
#[test]
fn miri_sorted_keyed_mapping_basic_ops() {
    let mut skm = SortedKeyedMapping::<i32, 16>::new();

    // Fill completely to test bounds
    for i in 0..16u8 {
        skm.add_child(i, i as i32);
        assert_eq!(*skm.seek_child(i).unwrap(), i as i32);
    }

    // Test deletion and reinsertion
    for i in (0..16u8).step_by(2) {
        assert_eq!(skm.delete_child(i), Some(i as i32));
        assert_eq!(skm.seek_child(i), None);
    }

    // Reinsert with different values
    for i in (0..16u8).step_by(2) {
        skm.add_child(i, (i as i32) * 10);
        assert_eq!(*skm.seek_child(i).unwrap(), (i as i32) * 10);
    }
}

/// Test conversion between mapping types for memory safety
#[test]
fn miri_mapping_conversions() {
    // Start with a SortedKeyedMapping
    let mut skm = SortedKeyedMapping::<Box<i64>, 4>::new();
    for i in 0..4u8 {
        skm.add_child(i, Box::new(i as i64));
    }

    // Convert to IndexedMapping (uses MaybeUninit operations)
    let mut im = IndexedMapping::<Box<i64>, 48, Bitset64<1>>::from_sorted_keyed(&mut skm);

    // Add more to IndexedMapping
    for i in 4..20u8 {
        im.add_child(i, Box::new(i as i64));
    }

    // Convert to DirectMapping
    let dm = DirectMapping::from_indexed(&mut im);

    // Verify all values are correct
    for i in 0..20u8 {
        assert_eq!(**dm.seek_child(i).unwrap(), i as i64);
    }
}

/// Test edge cases that might trigger memory issues
#[test]
fn miri_mapping_edge_cases() {
    // Test empty mappings
    let dm = DirectMapping::<i32>::new();
    assert_eq!(dm.num_children(), 0);
    assert_eq!(dm.seek_child(0), None);

    let mut im = IndexedMapping::<i32, 48, Bitset64<1>>::new();
    assert_eq!(im.delete_child(255), None); // Delete from empty

    // Test adding and immediately deleting
    let mut skm = SortedKeyedMapping::<String, 16>::new();
    skm.add_child(100, "test".to_string());
    assert_eq!(skm.delete_child(100), Some("test".to_string()));
    assert_eq!(skm.num_children(), 0);
}

/// Test concurrent-like access patterns that might reveal race conditions
/// (Even though these aren't actually concurrent, they test similar access patterns)
#[test]
fn miri_mapping_stress_patterns() {
    let mut dm = DirectMapping::<Vec<u8>>::new();

    // Rapid insert/delete cycles with owned data
    for round in 0..2 {
        // Reduce rounds to avoid too many leaks in test output
        // Insert batch
        for i in 0..10u8 {
            // Reduce batch size
            let key = (round * 10 + i as usize) as u8;
            dm.add_child(key, vec![key; (key % 10) as usize + 1]); // Smaller vecs
        }

        // Delete every other one
        for i in (0..10u8).step_by(2) {
            let key = (round * 10 + i as usize) as u8;
            let deleted = dm.delete_child(key).unwrap();
            assert_eq!(deleted.len(), (key % 10) as usize + 1);
        }

        // Modify remaining through mutable access
        for i in (1..10u8).step_by(2) {
            let key = (round * 10 + i as usize) as u8;
            if let Some(vec_ref) = dm.seek_child_mut(key) {
                vec_ref.push(255);
            }
        }

        // Clean up remaining entries after each round
        for i in (1..10u8).step_by(2) {
            let key = (round * 10 + i as usize) as u8;
            dm.delete_child(key);
        }
    }

    // Ensure all entries are cleaned up
    assert_eq!(dm.num_children(), 0);
}

/// Test uninitialized access scenarios - the most dangerous for memory safety
#[test]
fn miri_uninitialized_access_patterns() {
    // Test DirectMapping with sparse, uninitialized access
    let mut dm = DirectMapping::<Box<i32>>::new();

    // Only fill a few scattered positions
    dm.add_child(0, Box::new(0));
    dm.add_child(127, Box::new(127));
    dm.add_child(255, Box::new(255));

    // Test accessing uninitialized slots
    for i in 1..127u8 {
        assert_eq!(dm.seek_child(i), None);
        assert_eq!(dm.seek_child_mut(i), None);
    }
    for i in 128..255u8 {
        assert_eq!(dm.seek_child(i), None);
        assert_eq!(dm.seek_child_mut(i), None);
    }

    // Test IndexedMapping with partial initialization
    let mut im = IndexedMapping::<String, 48, Bitset64<1>>::new();

    // Only use positions 5, 15, 30 - leave gaps
    im.add_child(5, "five".to_string());
    im.add_child(15, "fifteen".to_string());
    im.add_child(30, "thirty".to_string());

    // Access uninitialized positions
    for i in [
        0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 29, 31, 47,
    ] {
        assert_eq!(im.seek_child(i), None);
    }

    // Verify initialized positions still work
    assert_eq!(im.seek_child(5).unwrap(), "five");
    assert_eq!(im.seek_child(15).unwrap(), "fifteen");
    assert_eq!(im.seek_child(30).unwrap(), "thirty");
}

/// Test create/destroy without any manipulation - tests default states
#[test]
fn miri_create_destroy_untouched() {
    // DirectMapping - create and drop without any operations
    {
        let dm = DirectMapping::<Vec<u8>>::new();
        assert_eq!(dm.num_children(), 0);
        // Test accessing empty mapping
        assert_eq!(dm.seek_child(0), None);
        assert_eq!(dm.seek_child(255), None);
    } // Drop should be clean

    // IndexedMapping - create and drop without operations
    {
        let im = IndexedMapping::<Box<String>, 48, Bitset64<1>>::new();
        assert_eq!(im.num_children(), 0);
        // Test accessing empty mapping
        assert_eq!(im.seek_child(0), None);
        assert_eq!(im.seek_child(47), None);
    } // Drop should be clean

    // SortedKeyedMapping - create and drop
    {
        let skm = SortedKeyedMapping::<Vec<i32>, 16>::new();
        assert_eq!(skm.num_children(), 0);
        assert_eq!(skm.seek_child(0), None);
    } // Drop should be clean
}

/// Test partial fill with manipulation - only half full, various operations
#[test]
fn miri_partial_fill_manipulation() {
    // DirectMapping - only fill every 4th position
    let mut dm = DirectMapping::<Box<i64>>::new();

    for i in (0..=255u8).step_by(4) {
        dm.add_child(i, Box::new(i as i64));
    }

    // Verify only the filled positions work
    for i in 0..=255u8 {
        if i % 4 == 0 {
            assert_eq!(**dm.seek_child(i).unwrap(), i as i64);
        } else {
            assert_eq!(dm.seek_child(i), None);
        }
    }

    // Modify some values
    for i in (0..=255u8).step_by(8) {
        if let Some(val) = dm.seek_child_mut(i) {
            **val *= 2;
        }
    }

    // Verify modifications
    for i in (0..=255u8).step_by(4) {
        let expected = if i % 8 == 0 { (i as i64) * 2 } else { i as i64 };
        assert_eq!(**dm.seek_child(i).unwrap(), expected);
    }

    // Delete some entries, leaving gaps
    for i in (0..=255u8).step_by(12) {
        dm.delete_child(i);
    }

    // Verify deletions created gaps
    for i in (0..=255u8).step_by(4) {
        if i % 12 == 0 {
            assert_eq!(dm.seek_child(i), None);
        } else {
            assert!(dm.seek_child(i).is_some());
        }
    }
}

/// Test IndexedMapping with minimal fills and MaybeUninit edge cases  
#[test]
fn miri_indexed_mapping_minimal_usage() {
    let mut im = IndexedMapping::<Box<String>, 48, Bitset64<1>>::new();

    // Add just one element
    im.add_child(42, Box::new("answer".to_string()));

    // Test accessing the one element and empty slots
    assert_eq!(im.seek_child(42).unwrap().as_str(), "answer");
    for i in 0..48u8 {
        if i != 42 {
            assert_eq!(im.seek_child(i), None);
        }
    }

    // Delete the one element - should be completely empty now
    let deleted = im.delete_child(42).unwrap();
    assert_eq!(deleted.as_str(), "answer");
    assert_eq!(im.num_children(), 0);

    // Test accessing after complete emptying
    for i in 0..48u8 {
        assert_eq!(im.seek_child(i), None);
    }

    // Add a few scattered elements
    im.add_child(1, Box::new("one".to_string()));
    im.add_child(25, Box::new("twenty-five".to_string()));
    im.add_child(47, Box::new("forty-seven".to_string()));

    // Test iterator with sparse data
    let items: Vec<_> = im.into_iter().collect();
    assert_eq!(items.len(), 3);

    // Should contain our three items
    let values: Vec<String> = items.into_iter().map(|(_, v)| *v).collect();
    assert!(values.contains(&"one".to_string()));
    assert!(values.contains(&"twenty-five".to_string()));
    assert!(values.contains(&"forty-seven".to_string()));
}

/// Test SortedKeyedMapping with dangerous conversion scenarios
#[test]
fn miri_sorted_keyed_dangerous_conversions() {
    // Test conversion with minimal data
    let mut skm = SortedKeyedMapping::<Box<i32>, 16>::new();

    // Add just two elements
    skm.add_child(5, Box::new(50));
    skm.add_child(10, Box::new(100));

    // This hits the MaybeUninit::assume_init() path with mostly uninitialized data
    let im = IndexedMapping::<Box<i32>, 48, Bitset64<1>>::from_sorted_keyed(&mut skm);

    // Verify conversion worked with sparse data
    assert_eq!(**im.seek_child(5).unwrap(), 50);
    assert_eq!(**im.seek_child(10).unwrap(), 100);
    assert_eq!(im.num_children(), 2);

    // Test accessing positions that were never initialized in the original
    for i in [0, 1, 2, 3, 4, 6, 7, 8, 9, 11, 12, 13, 14, 15] {
        assert_eq!(im.seek_child(i), None);
    }

    // Test empty conversion
    let mut empty_skm = SortedKeyedMapping::<Box<i32>, 4>::new();
    let empty_im = IndexedMapping::<Box<i32>, 48, Bitset64<1>>::from_sorted_keyed(&mut empty_skm);
    assert_eq!(empty_im.num_children(), 0);

    // Should be safe to access any position in empty converted mapping
    for i in 0..48u8 {
        assert_eq!(empty_im.seek_child(i), None);
    }
}
