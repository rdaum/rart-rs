//! Benchmarks comparing merge join implementations against naive approaches.
//!
//! This benchmark suite evaluates:
//! - Streaming merge join vs naive intersection
//! - Regular vs versioned tree performance
//! - Various dataset sizes and intersection ratios
//! - Join with and without value collection

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rart::versioned_tree::VersionedAdaptiveRadixTree;
use rart::{AdaptiveRadixTree, ArrayKey};
use std::collections::{BTreeMap, BTreeSet};
use std::hint::black_box;

/// Generate test keys with controlled intersection patterns
fn generate_test_keys(size: usize, prefix: &str) -> Vec<String> {
    (0..size).map(|i| format!("{}{:06}", prefix, i)).collect()
}

/// Generate overlapping key sets for intersection testing
fn generate_overlapping_keys(base_size: usize, overlap_ratio: f64) -> (Vec<String>, Vec<String>) {
    let overlap_size = (base_size as f64 * overlap_ratio) as usize;
    let unique_size = base_size - overlap_size;

    let mut set1 = generate_test_keys(overlap_size, "common_");
    set1.extend(generate_test_keys(unique_size, "set1_"));

    let mut set2 = generate_test_keys(overlap_size, "common_");
    set2.extend(generate_test_keys(unique_size, "set2_"));

    (set1, set2)
}

/// Build a regular ART from keys
fn build_regular_tree(keys: &[String]) -> AdaptiveRadixTree<ArrayKey<32>, usize> {
    let mut tree = AdaptiveRadixTree::new();
    for (i, key) in keys.iter().enumerate() {
        tree.insert(key, i);
    }
    tree
}

/// Build a versioned ART from keys
fn build_versioned_tree(keys: &[String]) -> VersionedAdaptiveRadixTree<ArrayKey<32>, usize> {
    let mut tree = VersionedAdaptiveRadixTree::new();
    for (i, key) in keys.iter().enumerate() {
        tree.insert(key, i);
    }
    tree
}

/// Build a BTreeMap from keys
fn build_btree_map(keys: &[String]) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for (i, key) in keys.iter().enumerate() {
        map.insert(key.clone(), i);
    }
    map
}

/// Merge join using BTreeMap iterators (our algorithm but with BTreeMap)
fn btree_merge_join(maps: &[&BTreeMap<String, usize>]) -> Vec<String> {
    if maps.is_empty() {
        return Vec::new();
    }

    // Create iterators for each BTreeMap
    let mut iters: Vec<_> = maps.iter().map(|m| m.iter()).collect();
    let mut current_entries: Vec<_> = iters.iter_mut().map(|iter| iter.next()).collect();

    // Check if any iterator is empty - if so, result is empty
    if current_entries.iter().any(|entry| entry.is_none()) {
        return Vec::new();
    }

    let mut results = Vec::new();

    loop {
        // Check if any iterator is exhausted
        if current_entries.iter().any(|entry| entry.is_none()) {
            break;
        }

        // Find minimum key across all current positions
        let min_key = current_entries
            .iter()
            .filter_map(|entry| entry.as_ref().map(|(k, _)| (*k).clone()))
            .min()
            .unwrap();

        // Check if all maps have this minimum key
        let mut all_match = true;

        for i in 0..maps.len() {
            if let Some((current_key, _)) = &current_entries[i] {
                if **current_key == min_key {
                    // This map matches, advance it
                    current_entries[i] = iters[i].next();
                } else if **current_key > min_key {
                    // This map is ahead, we don't have a complete match
                    all_match = false;
                }
            } else {
                // Map is exhausted
                all_match = false;
            }
        }

        // Advance maps that had keys smaller than min_key
        for i in 0..maps.len() {
            if let Some((current_key, _)) = &current_entries[i] {
                if **current_key < min_key {
                    current_entries[i] = iters[i].next();
                }
            }
        }

        if all_match {
            results.push(min_key);
        }
    }

    results
}

/// Naive intersection using BTreeSet
fn naive_intersection_btreeset(
    trees: &[&AdaptiveRadixTree<ArrayKey<32>, usize>],
) -> Vec<ArrayKey<32>> {
    if trees.is_empty() {
        return Vec::new();
    }

    // Collect all keys from first tree
    let mut intersection: BTreeSet<ArrayKey<32>> = trees[0].iter().map(|(k, _)| k).collect();

    // Intersect with each subsequent tree
    for tree in &trees[1..] {
        let tree_keys: BTreeSet<ArrayKey<32>> = tree.iter().map(|(k, _)| k).collect();
        intersection = intersection.intersection(&tree_keys).cloned().collect();
    }

    intersection.into_iter().collect()
}

/// Naive intersection using manual iteration (no Hash required)
fn naive_manual_intersection(
    trees: &[&AdaptiveRadixTree<ArrayKey<32>, usize>],
) -> Vec<ArrayKey<32>> {
    if trees.is_empty() {
        return Vec::new();
    }

    // Collect all keys from first tree
    let mut intersection: Vec<ArrayKey<32>> = trees[0].iter().map(|(k, _)| k).collect();

    // For each subsequent tree, filter intersection
    for tree in &trees[1..] {
        let tree_keys: BTreeSet<ArrayKey<32>> = tree.iter().map(|(k, _)| k).collect();
        intersection.retain(|key| tree_keys.contains(key));
    }

    intersection.sort();
    intersection
}

/// Nested loop join (worst case naive approach)
fn naive_nested_loop_join(trees: &[&AdaptiveRadixTree<ArrayKey<32>, usize>]) -> Vec<ArrayKey<32>> {
    if trees.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    // For each key in first tree
    for (key1, _) in trees[0].iter() {
        let mut found_in_all = true;

        // Check if it exists in all other trees
        for tree in &trees[1..] {
            if tree.get_k(&key1).is_none() {
                found_in_all = false;
                break;
            }
        }

        if found_in_all {
            results.push(key1);
        }
    }

    results
}

fn bench_two_way_join(c: &mut Criterion) {
    let mut group = c.benchmark_group("two_way_join");

    for size in [1_000, 10_000, 100_000] {
        for overlap_ratio in [0.1, 0.5, 0.9] {
            let (keys1, keys2) = generate_overlapping_keys(size, overlap_ratio);
            let tree1 = build_regular_tree(&keys1);
            let tree2 = build_regular_tree(&keys2);
            let trees = vec![&tree1, &tree2];

            let bench_id = BenchmarkId::new(
                "streaming_merge_join",
                format!("{}k_{}%", size / 1000, (overlap_ratio * 100.0) as u32),
            );
            group.bench_with_input(bench_id, &trees, |b, trees| {
                b.iter(|| {
                    let result: Vec<_> =
                        AdaptiveRadixTree::merge_join_keys(black_box(trees)).collect();
                    black_box(result)
                });
            });

            let bench_id = BenchmarkId::new(
                "naive_btreeset",
                format!("{}k_{}%", size / 1000, (overlap_ratio * 100.0) as u32),
            );
            group.bench_with_input(bench_id, &trees, |b, trees| {
                b.iter(|| {
                    let result = naive_intersection_btreeset(black_box(trees));
                    black_box(result)
                });
            });

            let bench_id = BenchmarkId::new(
                "naive_manual",
                format!("{}k_{}%", size / 1000, (overlap_ratio * 100.0) as u32),
            );
            group.bench_with_input(bench_id, &trees, |b, trees| {
                b.iter(|| {
                    let result = naive_manual_intersection(black_box(trees));
                    black_box(result)
                });
            });

            let bench_id = BenchmarkId::new(
                "naive_nested_loop",
                format!("{}k_{}%", size / 1000, (overlap_ratio * 100.0) as u32),
            );
            group.bench_with_input(bench_id, &trees, |b, trees| {
                b.iter(|| {
                    let result = naive_nested_loop_join(black_box(trees));
                    black_box(result)
                });
            });
        }
    }

    group.finish();
}

fn bench_n_way_join(c: &mut Criterion) {
    let mut group = c.benchmark_group("n_way_join");

    // Test scaling with number of trees
    for num_trees in [2, 4, 8] {
        let size_per_tree = 10_000;
        let overlap_ratio = 0.3;

        // Generate trees with some overlapping keys
        let mut trees = Vec::new();
        let mut owned_trees = Vec::new();

        // Create common keys that appear in all trees
        let common_keys =
            generate_test_keys((size_per_tree as f64 * overlap_ratio) as usize, "common_");

        for i in 0..num_trees {
            let mut keys = common_keys.clone();
            let unique_keys =
                generate_test_keys(size_per_tree - common_keys.len(), &format!("tree{}_", i));
            keys.extend(unique_keys);

            let tree = build_regular_tree(&keys);
            owned_trees.push(tree);
        }

        // Collect references
        for tree in &owned_trees {
            trees.push(tree);
        }

        let bench_id = BenchmarkId::new("streaming_merge_join", format!("{}_trees", num_trees));
        group.bench_with_input(bench_id, &trees, |b, trees| {
            b.iter(|| {
                let result: Vec<_> = AdaptiveRadixTree::merge_join_keys(black_box(trees)).collect();
                black_box(result)
            });
        });

        let bench_id = BenchmarkId::new("naive_btreeset", format!("{}_trees", num_trees));
        group.bench_with_input(bench_id, &trees, |b, trees| {
            b.iter(|| {
                let result = naive_intersection_btreeset(black_box(trees));
                black_box(result)
            });
        });
    }

    group.finish();
}

fn bench_with_values(c: &mut Criterion) {
    let mut group = c.benchmark_group("join_with_values");

    let (keys1, keys2) = generate_overlapping_keys(50_000, 0.5);
    let tree1 = build_regular_tree(&keys1);
    let tree2 = build_regular_tree(&keys2);
    let trees = vec![&tree1, &tree2];

    group.bench_function("streaming_keys_only", |b| {
        b.iter(|| {
            let result: Vec<_> = AdaptiveRadixTree::merge_join_keys(black_box(&trees)).collect();
            black_box(result)
        });
    });

    group.bench_function("streaming_with_values", |b| {
        b.iter(|| {
            let result: Vec<_> =
                AdaptiveRadixTree::merge_join_with_values(black_box(&trees)).collect();
            black_box(result)
        });
    });

    // Naive with values approach
    group.bench_function("naive_with_values", |b| {
        b.iter(|| {
            let keys = naive_intersection_btreeset(black_box(&trees));
            let result: Vec<_> = keys
                .into_iter()
                .map(|key| {
                    let values: Vec<_> = trees.iter().filter_map(|tree| tree.get_k(&key)).collect();
                    (key, values)
                })
                .collect();
            black_box(result)
        });
    });

    group.finish();
}

fn bench_versioned_vs_regular(c: &mut Criterion) {
    let mut group = c.benchmark_group("versioned_vs_regular");

    let (keys1, keys2) = generate_overlapping_keys(20_000, 0.4);

    // Regular trees
    let reg_tree1 = build_regular_tree(&keys1);
    let reg_tree2 = build_regular_tree(&keys2);
    let reg_trees = vec![&reg_tree1, &reg_tree2];

    // Versioned trees
    let ver_tree1 = build_versioned_tree(&keys1);
    let ver_tree2 = build_versioned_tree(&keys2);
    let ver_trees = vec![&ver_tree1, &ver_tree2];

    group.bench_function("regular_trees", |b| {
        b.iter(|| {
            let result: Vec<_> =
                AdaptiveRadixTree::merge_join_keys(black_box(&reg_trees)).collect();
            black_box(result)
        });
    });

    group.bench_function("versioned_trees", |b| {
        b.iter(|| {
            let result: Vec<_> =
                VersionedAdaptiveRadixTree::merge_join_keys(black_box(&ver_trees)).collect();
            black_box(result)
        });
    });

    group.finish();
}

fn bench_early_termination(c: &mut Criterion) {
    let mut group = c.benchmark_group("early_termination");

    // Test case where one tree is much smaller (should terminate early)
    let large_keys = generate_test_keys(100_000, "large_");
    let small_keys = generate_test_keys(100, "small_"); // No overlap with large

    let large_tree = build_regular_tree(&large_keys);
    let small_tree = build_regular_tree(&small_keys);
    let trees = vec![&large_tree, &small_tree];

    group.bench_function("streaming_merge_join", |b| {
        b.iter(|| {
            let result: Vec<_> = AdaptiveRadixTree::merge_join_keys(black_box(&trees)).collect();
            black_box(result)
        });
    });

    group.bench_function("naive_btreeset", |b| {
        b.iter(|| {
            let result = naive_intersection_btreeset(black_box(&trees));
            black_box(result)
        });
    });

    group.finish();
}

fn bench_optimized_joins(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimized_join_paths");

    let (keys1, keys2) = generate_overlapping_keys(50_000, 0.3);
    let tree1 = build_regular_tree(&keys1);
    let tree2 = build_regular_tree(&keys2);

    // Test 2-way join (should use optimized path)
    let trees_2way = vec![&tree1, &tree2];
    group.bench_function("two_way_join", |b| {
        b.iter(|| {
            let result: Vec<_> =
                AdaptiveRadixTree::merge_join_keys(black_box(&trees_2way)).collect();
            black_box(result)
        });
    });

    // Test 3-way join (should use fallback)
    let tree3 = build_regular_tree(&keys1); // Reuse keys for overlap
    let trees_3way = vec![&tree1, &tree2, &tree3];
    group.bench_function("three_way_join", |b| {
        b.iter(|| {
            let result: Vec<_> =
                AdaptiveRadixTree::merge_join_keys(black_box(&trees_3way)).collect();
            black_box(result)
        });
    });

    group.finish();
}

fn bench_art_vs_btree(c: &mut Criterion) {
    let mut group = c.benchmark_group("art_vs_btree");

    for size in [10_000, 50_000] {
        for overlap_ratio in [0.3, 0.7] {
            let (keys1, keys2) = generate_overlapping_keys(size, overlap_ratio);

            // Build ART trees
            let art_tree1 = build_regular_tree(&keys1);
            let art_tree2 = build_regular_tree(&keys2);
            let art_trees = vec![&art_tree1, &art_tree2];

            // Build BTreeMaps
            let btree_map1 = build_btree_map(&keys1);
            let btree_map2 = build_btree_map(&keys2);
            let btree_maps = vec![&btree_map1, &btree_map2];

            let bench_id = BenchmarkId::new(
                "art_merge_join",
                format!("{}k_{}%", size / 1000, (overlap_ratio * 100.0) as u32),
            );
            group.bench_with_input(bench_id, &art_trees, |b, trees| {
                b.iter(|| {
                    let result: Vec<_> =
                        AdaptiveRadixTree::merge_join_keys(black_box(trees)).collect();
                    black_box(result)
                });
            });

            let bench_id = BenchmarkId::new(
                "btree_merge_join",
                format!("{}k_{}%", size / 1000, (overlap_ratio * 100.0) as u32),
            );
            group.bench_with_input(bench_id, &btree_maps, |b, maps| {
                b.iter(|| {
                    let result = btree_merge_join(black_box(maps));
                    black_box(result)
                });
            });
        }
    }

    // Test N-way joins for ART vs BTreeMap
    for num_trees in [3, 4, 8] {
        let size_per_tree = 20_000;
        let overlap_ratio = 0.4;

        // Generate trees with some overlapping keys
        let mut art_trees = Vec::new();
        let mut art_owned_trees = Vec::new();
        let mut btree_maps = Vec::new();
        let mut btree_owned_maps = Vec::new();

        // Create common keys that appear in all trees
        let common_keys =
            generate_test_keys((size_per_tree as f64 * overlap_ratio) as usize, "common_");

        for i in 0..num_trees {
            let mut keys = common_keys.clone();
            let unique_keys =
                generate_test_keys(size_per_tree - common_keys.len(), &format!("tree{}_", i));
            keys.extend(unique_keys);

            let art_tree = build_regular_tree(&keys);
            let btree_map = build_btree_map(&keys);

            art_owned_trees.push(art_tree);
            btree_owned_maps.push(btree_map);
        }

        // Collect references
        for tree in &art_owned_trees {
            art_trees.push(tree);
        }
        for map in &btree_owned_maps {
            btree_maps.push(map);
        }

        let bench_id = BenchmarkId::new("art_nway_merge_join", format!("{}_trees", num_trees));
        group.bench_with_input(bench_id, &art_trees, |b, trees| {
            b.iter(|| {
                let result: Vec<_> = AdaptiveRadixTree::merge_join_keys(black_box(trees)).collect();
                black_box(result)
            });
        });

        let bench_id = BenchmarkId::new("btree_nway_merge_join", format!("{}_trees", num_trees));
        group.bench_with_input(bench_id, &btree_maps, |b, maps| {
            b.iter(|| {
                let result = btree_merge_join(black_box(maps));
                black_box(result)
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_two_way_join,
    bench_n_way_join,
    bench_with_values,
    bench_versioned_vs_regular,
    bench_early_termination,
    bench_optimized_joins,
    bench_art_vs_btree
);
criterion_main!(benches);
