//! Benchmarks comparing performance before and after multilevel optimization
//! Tests Node4 chains vs optimized MultilevelNode4 nodes

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::prelude::SliceRandom;
use rand::{Rng, rng};

use rart::keys::array_key::ArrayKey;
use rart::tree::AdaptiveRadixTree;
use rart::stats::TreeStatsTrait;

/// Generate keys that create Node4 chains suitable for multilevel optimization
/// These are keys with common prefixes that will create chain patterns
fn generate_chainable_keys(num_patterns: usize, chain_depth: usize, keys_per_pattern: usize) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rng = rng();
    
    for pattern_id in 0..num_patterns {
        // Create a base pattern that will form a chain
        let base_bytes: Vec<u8> = (0..chain_depth)
            .map(|i| ((pattern_id * 10 + i) % 256) as u8)
            .collect();
            
        // Generate keys that will all share this prefix chain
        for key_id in 0..keys_per_pattern {
            let mut key_bytes = base_bytes.clone();
            // Add some variation at the end
            key_bytes.push((key_id % 256) as u8);
            key_bytes.push(rng.random_range(0..=255));
            
            // Convert to string representation for easier handling
            let key_string = key_bytes.iter()
                .map(|&b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join("");
            keys.push(key_string);
        }
    }
    
    keys.shuffle(&mut rng);
    keys
}

/// Benchmark lookup performance before optimization (Node4 chains)
pub fn lookup_before_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("multilevel_lookup_before_opt");
    group.throughput(Throughput::Elements(1));
    
    // Test different tree sizes
    for &size in &[256, 1024, 4096] {
        let keys = generate_chainable_keys(size / 4, 4, 4); // Creates deeper chains with fewer children per level
        
        // Build tree without optimization
        let mut tree = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
        for key in &keys {
            tree.insert(key, format!("value_{}", key));
        }
        
        // Get stats to show node composition
        let stats = tree.get_tree_stats();
        println!("Before optimization (size {}): {:?}", size, stats.node_stats);
        
        group.bench_with_input(
            BenchmarkId::new("lookup_node4_chains", size),
            &size,
            |b, _| {
                let mut rng = rng();
                b.iter(|| {
                    let key = &keys[rng.random_range(0..keys.len())];
                    tree.get(key)
                })
            },
        );
    }
    
    group.finish();
}

/// Benchmark lookup performance after optimization (MultilevelNode4)
pub fn lookup_after_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("multilevel_lookup_after_opt");
    group.throughput(Throughput::Elements(1));
    
    // Test different tree sizes
    for &size in &[256, 1024, 4096] {
        let keys = generate_chainable_keys(size / 4, 4, 4); // Creates deeper chains with fewer children per level
        
        // Build tree and optimize
        let mut tree = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
        for key in &keys {
            tree.insert(key, format!("value_{}", key));
        }
        
        let optimizations = tree.optimize_multilevel();
        
        // Get stats to show node composition after optimization
        let stats = tree.get_tree_stats();
        println!("After optimization (size {}, {} optimizations): {:?}", 
                 size, optimizations, stats.node_stats);
        
        group.bench_with_input(
            BenchmarkId::new("lookup_multilevel_nodes", size),
            &size,
            |b, _| {
                let mut rng = rng();
                b.iter(|| {
                    let key = &keys[rng.random_range(0..keys.len())];
                    tree.get(key)
                })
            },
        );
    }
    
    group.finish();
}

/// Direct comparison benchmark showing both before and after in same test
pub fn lookup_optimization_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("multilevel_optimization_comparison");
    group.throughput(Throughput::Elements(1));
    
    for &size in &[256, 1024, 4096] {
        let keys = generate_chainable_keys(size / 4, 4, 4);
        
        // Create two identical trees
        let mut tree_before = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
        let mut tree_after = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
        
        for key in &keys {
            let value = format!("value_{}", key);
            tree_before.insert(key, value.clone());
            tree_after.insert(key, value);
        }
        
        // Only optimize the second tree
        let optimizations = tree_after.optimize_multilevel();
        
        // Show the difference in stats
        let stats_before = tree_before.get_tree_stats();
        let stats_after = tree_after.get_tree_stats();
        println!("\nComparison for size {}:", size);
        println!("  Before: {:?}", stats_before.node_stats);  
        println!("  After ({} optimizations): {:?}", optimizations, stats_after.node_stats);
        
        // Benchmark both
        group.bench_with_input(
            BenchmarkId::new("before_optimization", size),
            &size,
            |b, _| {
                let mut rng = rng();
                b.iter(|| {
                    let key = &keys[rng.random_range(0..keys.len())];
                    tree_before.get(key)
                })
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("after_optimization", size),
            &size,
            |b, _| {
                let mut rng = rng();
                b.iter(|| {
                    let key = &keys[rng.random_range(0..keys.len())];
                    tree_after.get(key)
                })
            },
        );
    }
    
    group.finish();
}

/// Benchmark the optimization process itself
pub fn optimization_process(c: &mut Criterion) {
    let mut group = c.benchmark_group("multilevel_optimization_process");
    group.throughput(Throughput::Elements(1));
    
    for &size in &[256, 1024, 4096] {
        let keys = generate_chainable_keys(size / 4, 4, 4);
        
        group.bench_with_input(
            BenchmarkId::new("optimize_multilevel", size),
            &size,
            |b, _| {
                b.iter_batched(
                    || {
                        // Setup: create a tree with Node4 chains
                        let mut tree = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
                        for key in &keys {
                            tree.insert(key, format!("value_{}", key));
                        }
                        tree
                    },
                    |mut tree| {
                        // The actual operation we're benchmarking
                        tree.optimize_multilevel()
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    
    group.finish();
}

/// Benchmark insertion performance with periodic optimization
pub fn insert_with_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("multilevel_insert_with_optimization");
    group.throughput(Throughput::Elements(1));
    
    for &batch_size in &[100, 500, 1000] {
        let keys = generate_chainable_keys(batch_size / 4, 4, 4);
        
        group.bench_with_input(
            BenchmarkId::new("insert_then_optimize", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    let mut tree = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
                    
                    // Insert all keys
                    for key in &keys {
                        tree.insert(key, format!("value_{}", key));
                    }
                    
                    // Then optimize
                    tree.optimize_multilevel()
                })
            },
        );
    }
    
    group.finish();
}

/// Benchmark sequential access patterns (which should benefit more from multilevel nodes)
pub fn sequential_scan_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("multilevel_sequential_scan");
    group.throughput(Throughput::Elements(1));
    
    for &size in &[256, 1024] {
        let keys = generate_chainable_keys(size / 4, 4, 4);
        let mut sorted_keys = keys.clone();
        sorted_keys.sort();
        
        // Create optimized and unoptimized trees
        let mut tree_before = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
        let mut tree_after = AdaptiveRadixTree::<ArrayKey<32>, String>::new();
        
        for key in &keys {
            let value = format!("value_{}", key);
            tree_before.insert(key, value.clone());
            tree_after.insert(key, value);
        }
        
        tree_after.optimize_multilevel();
        
        // Benchmark sequential iteration through sorted keys
        group.bench_with_input(
            BenchmarkId::new("scan_before_optimization", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let mut count = 0;
                    for key in &sorted_keys {
                        if tree_before.get(key).is_some() {
                            count += 1;
                        }
                    }
                    count
                })
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("scan_after_optimization", size),
            &size,
            |b, _| {
                b.iter(|| {
                    let mut count = 0;
                    for key in &sorted_keys {
                        if tree_after.get(key).is_some() {
                            count += 1;
                        }
                    }
                    count
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    multilevel_benches,
    lookup_before_optimization,
    lookup_after_optimization,
    lookup_optimization_comparison,
    optimization_process,
    insert_with_optimization,
    sequential_scan_comparison
);

criterion_main!(multilevel_benches);