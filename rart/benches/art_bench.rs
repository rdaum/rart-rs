/// Overall simple performance bench for static # of keys in a few secnarios. Here to quickly test\
/// for regressions.
use std::time::{Instant, Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::SliceRandom;
use rand::{thread_rng, Rng};

use rart::keys::array_key::ArrayKey;

use rart::tree::AdaptiveRadixTree;

// Variations on the number of keys to insert into the tree for benchmarks that measure retrievals
const TREE_SIZES: [u64; 4] = [1 << 15, 1 << 20, 1 << 22, 1 << 24];

pub fn rand_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("rand_insert");
    group.throughput(Throughput::Elements(1));

    let keys = gen_keys(3, 2, 3);
    let cached_keys = gen_cached_keys(3, 2, 3);

    group.bench_function("cached_keys", |b| {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
        let mut rng = thread_rng();
        b.iter(|| {
            let key = &cached_keys[rng.gen_range(0..cached_keys.len())];
            tree.insert_k(&key.0, key.1.clone());
        })
    });

    group.bench_function("uncached_keys", |b| {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
        let mut rng = thread_rng();
        b.iter(|| {
            let key = &keys[rng.gen_range(0..keys.len())];
            tree.insert(key, key.clone());
        })
    });

    group.finish();
}

pub fn rand_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("rand_remove");
    let keys = gen_keys(3, 2, 3);
    let cached_keys = gen_cached_keys(3, 2, 3);

    group.throughput(Throughput::Elements(1));
    group.bench_function("cached_keys", |b| {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
        let mut rng = thread_rng();
        for key in &cached_keys {
            tree.insert_k(&key.0.clone(), key.1.clone());
        }
        b.iter(|| {
            let key = &cached_keys[rng.gen_range(0..keys.len())];
            criterion::black_box(tree.remove_k(&key.0));
        })
    });
    group.bench_function("uncached_keys", |b| {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
        let mut rng = thread_rng();
        for key in &keys {
            tree.insert(key, key);
        }
        b.iter(|| {
            let key = &keys[rng.gen_range(0..keys.len())];
            criterion::black_box(tree.remove(key));
        })
    });

    group.finish();
}

pub fn rand_get(c: &mut Criterion) {
    for size in TREE_SIZES {
        c.bench_with_input(BenchmarkId::new("rand_get", size), &size, |b, size| {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
            for i in 0..*size {
                tree.insert(i, i);
            }
            let mut rng = thread_rng();
            b.iter(|| {
                let key = rng.gen_range(0..*size);
                criterion::black_box(tree.get(key));
            })
        });
    }
}

pub fn rand_get_str(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_get_str");
    let keys = gen_keys(3, 2, 3);
    let cached_keys = gen_cached_keys(3, 2, 3);
    group.throughput(Throughput::Elements(1));
    for size in TREE_SIZES {
        group.bench_with_input(
            BenchmarkId::new("cached_keys", size),
            &size,
            |b, _size| {
                let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
                for (i, key) in cached_keys.iter().enumerate() {
                    tree.insert_k(&key.0, i);
                }
                let mut rng = thread_rng();
                b.iter(|| {
                    let key = &cached_keys[rng.gen_range(0..keys.len())];
                    criterion::black_box(tree.get_k(&key.0));
                })
            },
        );
    }

    for size in TREE_SIZES {
        group.bench_with_input(BenchmarkId::new("uncached_keys", size), &size, |b, _size| {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
            for (i, key) in keys.iter().enumerate() {
                tree.insert(key, i);
            }
            let mut rng = thread_rng();
            b.iter(|| {
                let key = &keys[rng.gen_range(0..keys.len())];
                criterion::black_box(tree.get(key));
            })
        });
    }

    group.finish();
}

pub fn seq_get(c: &mut Criterion) {
    for size in TREE_SIZES {
        c.bench_with_input(BenchmarkId::new("seq_get", size), &size, |b, size| {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
            for i in 0..*size {
                tree.insert(i, i);
            }
            b.iter_custom(|iters| {
                let mut c = 0;
                let start = Instant::now();
                for _ in 0..iters {
                    if c == *size {
                        c = 0;
                    }
                    tree.get(c).unwrap();
                    c += 1;
                }
                start.elapsed()
            })
        });
    }
}

pub fn seq_insert(c: &mut Criterion) {
    c.bench_function("seq_insert", |b| {
        let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
        let mut key = 0u64;
        b.iter(|| {
            tree.insert(key, key);
            key += 1;
        })
    });
}

pub fn seq_remove(c: &mut Criterion) {
    for size in TREE_SIZES {
        c.bench_with_input(BenchmarkId::new("seq_remove", size), &size, |b, size| {
            let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();
            b.iter_custom(|iters| {
                for i in 0..*size {
                    tree.insert(i, i);
                }
                let mut start = Instant::now();
                let mut cumulative_time = Duration::new(0, 0);
                let mut c = 0;
                for _ in 0..iters {
                    if c == *size {
                        cumulative_time += start.elapsed();
                        c = 0;
                        for i in 0..*size {
                            tree.insert(i, i);
                        }
                        start = Instant::now();
                    }
                    tree.remove(c).unwrap();
                    c += 1;
                }
                cumulative_time += start.elapsed();
                cumulative_time
            })
        });
    }
}

fn gen_keys(l1_prefix: usize, l2_prefix: usize, suffix: usize) -> Vec<String> {
    let mut keys = Vec::new();
    let chars: Vec<char> = ('a'..='z').collect();
    for i in 0..chars.len() {
        let level1_prefix = chars[i].to_string().repeat(l1_prefix);
        for i in 0..chars.len() {
            let level2_prefix = chars[i].to_string().repeat(l2_prefix);
            let key_prefix = level1_prefix.clone() + &level2_prefix;
            for _ in 0..=u8::MAX {
                let suffix: String = (0..suffix)
                    .map(|_| chars[thread_rng().gen_range(0..chars.len())])
                    .collect();
                let k = key_prefix.clone() + &suffix;
                keys.push(k);
            }
        }
    }

    keys.shuffle(&mut thread_rng());
    keys
}

fn gen_cached_keys(
    l1_prefix: usize,
    l2_prefix: usize,
    suffix: usize,
) -> Vec<(ArrayKey<16>, String)> {
    let mut keys = Vec::new();
    let chars: Vec<char> = ('a'..='z').collect();
    for i in 0..chars.len() {
        let level1_prefix = chars[i].to_string().repeat(l1_prefix);
        for i in 0..chars.len() {
            let level2_prefix = chars[i].to_string().repeat(l2_prefix);
            let key_prefix = level1_prefix.clone() + &level2_prefix;
            for _ in 0..=u8::MAX {
                let suffix: String = (0..suffix)
                    .map(|_| chars[thread_rng().gen_range(0..chars.len())])
                    .collect();
                let string = key_prefix.clone() + &suffix;
                let k = string.clone().into();
                keys.push((k, string));
            }
        }
    }

    keys.shuffle(&mut thread_rng());
    keys
}

criterion_group!(
    rand_benches,
    rand_get,
    rand_get_str,
    rand_insert,
    rand_remove
);
criterion_group!(seq_benches, seq_get, seq_insert, seq_remove);
criterion_main!(seq_benches, rand_benches);
