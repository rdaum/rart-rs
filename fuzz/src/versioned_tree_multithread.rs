#![no_main]

use std::collections::HashMap;
use std::sync::{Arc, Barrier};
use std::thread;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use rart::VersionedAdaptiveRadixTree;
use rart::keys::array_key::ArrayKey;

#[derive(Arbitrary, Debug, Clone)]
enum MainOp {
    Insert { key: usize, val: usize },
    Remove { key: usize },
    Get { key: usize },
    CreateSnapshot { thread_id: u8, ops: Vec<ThreadOp> },
}

#[derive(Arbitrary, Debug, Clone)]
enum ThreadOp {
    Insert { key: usize, val: usize },
    Remove { key: usize },
    Get { key: usize },
    CreateNestedSnapshot { ops: Vec<ThreadOp> },
}

#[derive(Arbitrary, Debug)]
struct MultithreadedFuzzInput {
    // Initial setup operations on main thread
    setup_ops: Vec<MainOp>,
    // Number of threads to spawn (1-8)
    num_threads: u8,
    // Operations to distribute across threads
    thread_ops: Vec<Vec<ThreadOp>>,
}

fuzz_target!(|input: MultithreadedFuzzInput| {
    // Bound the number of threads and operations to keep fuzzing reasonable
    let num_threads = ((input.num_threads % 8) + 1) as usize; // 1-8 threads
    let setup_ops = input.setup_ops.into_iter().take(50).collect::<Vec<_>>();

    let mut main_tree = VersionedAdaptiveRadixTree::<ArrayKey<16>, usize>::new();
    let mut reference_map = HashMap::<usize, usize>::new();

    // Phase 1: Setup operations on main thread
    for op in setup_ops {
        match op {
            MainOp::Insert { key, val } => {
                main_tree.insert(key, val);
                reference_map.insert(key, val);
            }
            MainOp::Remove { key } => {
                main_tree.remove(key);
                reference_map.remove(&key);
            }
            MainOp::Get { key } => {
                let tree_result = main_tree.get(key).copied();
                let ref_result = reference_map.get(&key).copied();
                assert_eq!(
                    tree_result, ref_result,
                    "Setup Get mismatch for key {}",
                    key
                );
            }
            MainOp::CreateSnapshot { .. } => {
                // Skip snapshots in setup phase
            }
        }
    }

    // Phase 2: Create snapshots and send to threads
    let mut thread_handles = Vec::new();
    let barrier = Arc::new(Barrier::new(num_threads + 1)); // +1 for main thread

    for thread_id in 0..num_threads {
        let snapshot = main_tree.snapshot();
        let thread_ops = input
            .thread_ops
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(100) // Limit ops per thread
            .collect::<Vec<_>>();
        let barrier_clone = Arc::clone(&barrier);
        let reference_clone = reference_map.clone();

        let handle = thread::spawn(move || {
            // Wait for all threads to be ready
            barrier_clone.wait();

            thread_worker(thread_id, snapshot, thread_ops, reference_clone);
        });

        thread_handles.push(handle);
    }

    // Main thread waits for all threads to be ready, then releases them
    barrier.wait();

    // Wait for all threads to complete
    for handle in thread_handles {
        handle.join().expect("Thread panicked");
    }

    // Phase 3: Final consistency check on main tree
    for (key, expected_val) in &reference_map {
        let actual_val = main_tree.get(*key).copied();
        assert_eq!(
            actual_val,
            Some(*expected_val),
            "Final consistency check failed for key {}",
            key
        );
    }
});

fn thread_worker(
    thread_id: usize,
    mut snapshot: VersionedAdaptiveRadixTree<ArrayKey<16>, usize>,
    ops: Vec<ThreadOp>,
    mut reference_map: HashMap<usize, usize>,
) {
    let mut nested_snapshots: Vec<VersionedAdaptiveRadixTree<ArrayKey<16>, usize>> = Vec::new();

    for op in ops {
        match op {
            ThreadOp::Insert { key, val } => {
                // Modify the thread-local snapshot
                snapshot.insert(key, val);
                reference_map.insert(key, val);

                // Also modify any nested snapshots
                for nested in &mut nested_snapshots {
                    nested.insert(key, val);
                }
            }
            ThreadOp::Remove { key } => {
                snapshot.remove(key);
                reference_map.remove(&key);

                for nested in &mut nested_snapshots {
                    nested.remove(key);
                }
            }
            ThreadOp::Get { key } => {
                let snapshot_result = snapshot.get(key).copied();
                let reference_result = reference_map.get(&key).copied();
                assert_eq!(
                    snapshot_result, reference_result,
                    "Thread {} Get mismatch for key {}: snapshot={:?}, reference={:?}",
                    thread_id, key, snapshot_result, reference_result
                );

                // Check nested snapshots too
                for (i, nested) in nested_snapshots.iter().enumerate() {
                    let nested_result = nested.get(key).copied();
                    assert_eq!(
                        nested_result, reference_result,
                        "Thread {} Nested snapshot {} Get mismatch for key {}",
                        thread_id, i, key
                    );
                }
            }
            ThreadOp::CreateNestedSnapshot { ops } => {
                // Create a snapshot of the current thread's snapshot
                let mut nested_snapshot = snapshot.snapshot();
                let mut nested_reference = reference_map.clone();

                // Execute nested operations
                for nested_op in ops.into_iter().take(20) {
                    // Limit nested ops
                    match nested_op {
                        ThreadOp::Insert { key, val } => {
                            nested_snapshot.insert(key, val);
                            nested_reference.insert(key, val);
                        }
                        ThreadOp::Remove { key } => {
                            nested_snapshot.remove(key);
                            nested_reference.remove(&key);
                        }
                        ThreadOp::Get { key } => {
                            let nested_result = nested_snapshot.get(key).copied();
                            let ref_result = nested_reference.get(&key).copied();
                            assert_eq!(
                                nested_result, ref_result,
                                "Thread {} Nested Get mismatch for key {}",
                                thread_id, key
                            );
                        }
                        ThreadOp::CreateNestedSnapshot { .. } => {
                            // Don't nest too deeply
                        }
                    }
                }

                // Verify the nested snapshot is consistent
                for (key, expected_val) in &nested_reference {
                    let actual_val = nested_snapshot.get(*key).copied();
                    assert_eq!(
                        actual_val,
                        Some(*expected_val),
                        "Thread {} Nested snapshot inconsistent for key {}",
                        thread_id,
                        key
                    );
                }

                // Keep the nested snapshot around for later operations
                if nested_snapshots.len() < 5 {
                    // Limit number of nested snapshots
                    nested_snapshots.push(nested_snapshot);
                }
            }
        }
    }

    // Final consistency check for this thread's snapshot
    for (key, expected_val) in &reference_map {
        let actual_val = snapshot.get(*key).copied();
        assert_eq!(
            actual_val,
            Some(*expected_val),
            "Thread {} final consistency check failed for key {}",
            thread_id,
            key
        );
    }
}
