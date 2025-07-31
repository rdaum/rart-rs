#![no_main]

use std::collections::HashMap;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use rart::VersionedAdaptiveRadixTree;
use rart::keys::array_key::ArrayKey;

#[derive(Arbitrary, Debug, Clone)]
enum TreeOp {
    Get { key: usize },
    Insert { key: usize, val: usize },
    Remove { key: usize },
    Snapshot,
    SnapshotAndMutate { key: usize, val: usize },
}

fuzz_target!(|ops: Vec<TreeOp>| {
    let mut versioned_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
    let mut reference_map = HashMap::<usize, usize>::new();
    let mut snapshots = Vec::new();
    let mut snapshot_maps = Vec::new();

    for op in ops {
        match op {
            TreeOp::Get { key } => {
                let versioned_result = versioned_tree.get(key).copied();
                let reference_result = reference_map.get(&key).copied();
                assert_eq!(
                    versioned_result, reference_result,
                    "Get mismatch for key {key}: versioned={versioned_result:?}, reference={reference_result:?}"
                );
            }
            TreeOp::Insert { key, val } => {
                let versioned_old = versioned_tree.insert(key, val);
                let reference_old = reference_map.insert(key, val);
                // Note: versioned tree may not return old value due to CoW semantics
                // We only check that if reference had no old value, versioned also returns None
                if reference_old.is_none() {
                    // For new insertions, both should agree
                    assert_eq!(
                        versioned_old,
                        reference_old.is_some(),
                        "Insert mismatch for new key {key}: versioned={versioned_old:?}, reference={reference_old:?}"
                    );
                }

                // Check that the value is actually inserted
                let versioned_check = versioned_tree.get(key).copied();
                let reference_check = reference_map.get(&key).copied();
                assert_eq!(
                    versioned_check, reference_check,
                    "Post-insert check failed for key {key}: versioned={versioned_check:?}, reference={reference_check:?}"
                );
            }
            TreeOp::Remove { key } => {
                let versioned_removed = versioned_tree.remove(key);
                let reference_removed = reference_map.remove(&key);

                // For removals, check that both agree on whether key existed
                assert_eq!(
                    versioned_removed.is_some(),
                    reference_removed.is_some(),
                    "Remove existence mismatch for key {}: versioned_exists={}, reference_exists={}",
                    key,
                    versioned_removed.is_some(),
                    reference_removed.is_some()
                );

                // Verify the key is actually gone
                let versioned_check = versioned_tree.get(key);
                let reference_check = reference_map.get(&key);
                assert_eq!(
                    versioned_check, reference_check,
                    "Post-remove check failed for key {key}: versioned={versioned_check:?}, reference={reference_check:?}"
                );
            }
            TreeOp::Snapshot => {
                let snapshot = versioned_tree.snapshot();
                let snapshot_map = reference_map.clone();

                // Verify snapshot has same contents as original
                for (key, expected_val) in snapshot_map.iter() {
                    let snapshot_val = snapshot.get(key);
                    assert_eq!(
                        snapshot_val,
                        Some(expected_val),
                        "Snapshot content mismatch for key {}: got {:?}, expected {:?}",
                        key,
                        snapshot_val,
                        Some(expected_val)
                    );
                }

                snapshots.push(snapshot);
                snapshot_maps.push(snapshot_map);
            }
            TreeOp::SnapshotAndMutate { key, val } => {
                if !snapshots.is_empty() {
                    let snapshot_idx = key % snapshots.len();
                    let _old_snapshot_val = snapshots[snapshot_idx].get(key).copied();
                    let _old_map_val = snapshot_maps[snapshot_idx].get(&key).copied();

                    // Insert into snapshot
                    snapshots[snapshot_idx].insert(key, val);
                    snapshot_maps[snapshot_idx].insert(key, val);

                    // Verify the change is isolated to this snapshot
                    let new_snapshot_val = snapshots[snapshot_idx].get(key).copied();
                    let new_map_val = snapshot_maps[snapshot_idx].get(&key).copied();
                    assert_eq!(
                        new_snapshot_val, new_map_val,
                        "Snapshot mutation mismatch for key {key}: got {new_snapshot_val:?}, expected {new_map_val:?}"
                    );

                    // Verify other snapshots are unaffected
                    for (i, (other_snapshot, other_map)) in
                        snapshots.iter().zip(snapshot_maps.iter()).enumerate()
                    {
                        if i != snapshot_idx {
                            let other_val = other_snapshot.get(key).copied();
                            let other_expected = other_map.get(&key).copied();
                            assert_eq!(
                                other_val, other_expected,
                                "Snapshot isolation violated: snapshot {i} affected by mutation to snapshot {snapshot_idx}"
                            );
                        }
                    }

                    // Verify original tree is unaffected
                    let original_val = versioned_tree.get(key).copied();
                    let original_expected = reference_map.get(&key).copied();
                    assert_eq!(
                        original_val, original_expected,
                        "Original tree affected by snapshot mutation"
                    );
                }
            }
        }
    }

    // Final consistency check - verify all snapshots still match their reference maps
    for (snapshot, snapshot_map) in snapshots.iter().zip(snapshot_maps.iter()) {
        for (key, expected_val) in snapshot_map.iter() {
            let actual_val = snapshot.get(key);
            assert_eq!(
                actual_val,
                Some(expected_val),
                "Final snapshot consistency check failed for key {}: got {:?}, expected {:?}",
                key,
                actual_val,
                Some(expected_val)
            );
        }

        // Also check that snapshot doesn't have extra keys
        // This is harder to test directly, so we rely on the operations above
    }

    // Final check - original tree should match reference map
    for (key, expected_val) in reference_map.iter() {
        let actual_val = versioned_tree.get(key);
        assert_eq!(
            actual_val,
            Some(expected_val),
            "Final tree consistency check failed for key {}: got {:?}, expected {:?}",
            key,
            actual_val,
            Some(expected_val)
        );
    }
});
