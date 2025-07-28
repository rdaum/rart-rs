#![no_main]

use std::collections::BTreeMap;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use rart::TreeTrait;
use rart::keys::array_key::ArrayKey;
use rart::tree::AdaptiveRadixTree;

#[derive(Arbitrary, Debug)]
enum SetupAction {
    Insert { key: usize, val: usize },
}

#[derive(Arbitrary, Debug)]
enum IterRangeAction {
    IterateAll,
    RangeUnbounded,
    RangeFrom { start: usize },
    RangeTo { end: usize },
    RangeToInclusive { end: usize },
    RangeFull { start: usize, end: usize },
    RangeInclusive { start: usize, end: usize },
}

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    setup_actions: Vec<SetupAction>,
    iter_range_actions: Vec<IterRangeAction>,
}

fuzz_target!(|input: FuzzInput| {
    let mut art = AdaptiveRadixTree::<ArrayKey<16>, usize>::new();
    let mut btree = BTreeMap::<ArrayKey<16>, usize>::new();

    // Setup phase: populate both data structures
    for action in input.setup_actions {
        match action {
            SetupAction::Insert { key, val } => {
                let array_key: ArrayKey<16> = key.into();
                art.insert(array_key, val);
                btree.insert(array_key, val);
            }
        }
    }

    // Test phase: compare iterator and range behaviors
    for action in input.iter_range_actions {
        match action {
            IterRangeAction::IterateAll => {
                // Test full iteration
                let art_items: Vec<_> = art.iter().map(|(k, v)| (k, *v)).collect();
                let btree_items: Vec<_> = btree.iter().map(|(k, v)| (*k, *v)).collect();

                // Both should have same length
                assert_eq!(
                    art_items.len(),
                    btree_items.len(),
                    "Iterator length mismatch: ART={}, BTree={}",
                    art_items.len(),
                    btree_items.len()
                );

                // Convert to sorted sets for comparison (ART iteration order may differ)
                let mut art_sorted = art_items;
                let mut btree_sorted = btree_items;
                art_sorted.sort();
                btree_sorted.sort();

                assert_eq!(art_sorted, btree_sorted, "Iterator contents don't match");
            }

            IterRangeAction::RangeUnbounded => {
                // Test (..) range
                let art_items: Vec<_> = art.range(..).map(|(k, v)| (k, *v)).collect();
                let btree_items: Vec<_> = btree.range(..).map(|(k, v)| (*k, *v)).collect();

                assert_eq!(art_items.len(), btree_items.len());

                let mut art_sorted = art_items;
                let mut btree_sorted = btree_items;
                art_sorted.sort();
                btree_sorted.sort();

                assert_eq!(
                    art_sorted, btree_sorted,
                    "Unbounded range contents don't match"
                );
            }

            IterRangeAction::RangeFrom { start } => {
                // Test (start..) range
                let start_key: ArrayKey<16> = start.into();
                let art_items: Vec<_> = art.range(start_key..).map(|(k, v)| (k, *v)).collect();
                let btree_items: Vec<_> = btree.range(start_key..).map(|(k, v)| (*k, *v)).collect();

                assert_eq!(
                    art_items.len(),
                    btree_items.len(),
                    "RangeFrom({start}) length mismatch"
                );

                let mut art_sorted = art_items;
                let mut btree_sorted = btree_items;
                art_sorted.sort();
                btree_sorted.sort();

                assert_eq!(
                    art_sorted, btree_sorted,
                    "RangeFrom({start}) contents don't match"
                );
            }

            IterRangeAction::RangeTo { end } => {
                // Test (..end) range
                let end_key: ArrayKey<16> = end.into();
                let art_items: Vec<_> = art.range(..end_key).map(|(k, v)| (k, *v)).collect();
                let btree_items: Vec<_> = btree.range(..end_key).map(|(k, v)| (*k, *v)).collect();

                assert_eq!(
                    art_items.len(),
                    btree_items.len(),
                    "RangeTo({end}) length mismatch"
                );

                let mut art_sorted = art_items;
                let mut btree_sorted = btree_items;
                art_sorted.sort();
                btree_sorted.sort();

                assert_eq!(
                    art_sorted, btree_sorted,
                    "RangeTo({end}) contents don't match"
                );
            }

            IterRangeAction::RangeToInclusive { end } => {
                // Test (..=end) range
                let end_key: ArrayKey<16> = end.into();
                let art_items: Vec<_> = art.range(..=end_key).map(|(k, v)| (k, *v)).collect();
                let btree_items: Vec<_> = btree.range(..=end_key).map(|(k, v)| (*k, *v)).collect();

                assert_eq!(
                    art_items.len(),
                    btree_items.len(),
                    "RangeToInclusive({end}) length mismatch"
                );

                let mut art_sorted = art_items;
                let mut btree_sorted = btree_items;
                art_sorted.sort();
                btree_sorted.sort();

                assert_eq!(
                    art_sorted, btree_sorted,
                    "RangeToInclusive({end}) contents don't match"
                );
            }

            IterRangeAction::RangeFull { start, end } => {
                // Test (start..end) range
                if start < end {
                    let start_key: ArrayKey<16> = start.into();
                    let end_key: ArrayKey<16> = end.into();
                    let art_items: Vec<_> = art
                        .range(start_key..end_key)
                        .map(|(k, v)| (k, *v))
                        .collect();
                    let btree_items: Vec<_> = btree
                        .range(start_key..end_key)
                        .map(|(k, v)| (*k, *v))
                        .collect();

                    assert_eq!(
                        art_items.len(),
                        btree_items.len(),
                        "RangeFull({start}..{end}) length mismatch"
                    );

                    let mut art_sorted = art_items;
                    let mut btree_sorted = btree_items;
                    art_sorted.sort();
                    btree_sorted.sort();

                    assert_eq!(
                        art_sorted, btree_sorted,
                        "RangeFull({start}..{end}) contents don't match"
                    );
                }
            }

            IterRangeAction::RangeInclusive { start, end } => {
                // Test (start..=end) range
                if start <= end {
                    let start_key: ArrayKey<16> = start.into();
                    let end_key: ArrayKey<16> = end.into();
                    let art_items: Vec<_> = art
                        .range(start_key..=end_key)
                        .map(|(k, v)| (k, *v))
                        .collect();
                    let btree_items: Vec<_> = btree
                        .range(start_key..=end_key)
                        .map(|(k, v)| (*k, *v))
                        .collect();

                    assert_eq!(
                        art_items.len(),
                        btree_items.len(),
                        "RangeInclusive({start}..={end}) length mismatch"
                    );

                    let mut art_sorted = art_items;
                    let mut btree_sorted = btree_items;
                    art_sorted.sort();
                    btree_sorted.sort();

                    assert_eq!(
                        art_sorted, btree_sorted,
                        "RangeInclusive({start}..={end}) contents don't match"
                    );
                }
            }
        }
    }

    // Additional consistency checks

    // Verify that full iteration and unbounded range return the same results
    let iter_items: Vec<_> = art.iter().map(|(k, v)| (k, *v)).collect();
    let range_items: Vec<_> = art.range(..).map(|(k, v)| (k, *v)).collect();

    let mut iter_sorted = iter_items;
    let mut range_sorted = range_items;
    iter_sorted.sort();
    range_sorted.sort();

    assert_eq!(
        iter_sorted, range_sorted,
        "Iterator and unbounded range should return same results"
    );

    // Verify range bounds are respected
    for (key, _) in btree.iter() {
        // Test that key appears in appropriate ranges
        let key_in_iter = art.iter().any(|(k, _)| k == *key);
        let key_in_range = art.range(..).any(|(k, _)| k == *key);

        assert_eq!(
            key_in_iter, key_in_range,
            "Key {key:?} presence inconsistent between iter and range"
        );
    }
});
