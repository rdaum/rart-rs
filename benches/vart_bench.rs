use std::time::Instant;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, Throughput};
use rand::{Rng, thread_rng};
use rand::prelude::SliceRandom;

use rart::pageable::pageable_tree::PageableAdaptiveRadixTree;
use rart::pageable::vector_node_store::VectorNodeStore;
use rart::partials::array_partial::ArrPartial;
use rart::partials::key::ArrayKey;

pub fn seq_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("seq_insert");
    group.throughput(Throughput::Elements(1));
    group.bench_function("seq_insert", |b| {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<8>, _>::new());
        let mut key = 0u64;
        b.iter(|| {
            tree.insert::<ArrayKey<16>>(&key.into(), key);
            key += 1;
        })
    });

    group.finish();
}

pub fn rand_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("rand_insert");
    group.throughput(Throughput::Elements(1));

    let keys = gen_keys(3, 2, 3);
    let cached_keys = gen_cached_keys(3, 2, 3);

    group.bench_function("vart_cached_keys", |b| {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, _>::new());
        let mut rng = thread_rng();
        b.iter(|| {
            let key = &cached_keys[rng.gen_range(0..cached_keys.len())];
            tree.insert(&key.0, key.1.clone());
        })
    });

    group.bench_function("vart", |b| {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, _>::new());
        let mut rng = thread_rng();
        b.iter(|| {
            let key = &keys[rng.gen_range(0..keys.len())];
            tree.insert::<ArrayKey<16>>(&key.into(), key.clone());
        })
    });

    group.finish();
}

pub fn seq_delete(c: &mut Criterion) {
    let mut group = c.benchmark_group("seq_delete");
    group.throughput(Throughput::Elements(1));
    group.bench_function("vart", |b| {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<8>, _>::new());
        b.iter_custom(|iters| {
            for i in 0..iters {
                tree.insert::<ArrayKey<8>>(&i.into(), i);
            }
            let start = Instant::now();
            for i in 0..iters {
                tree.remove::<ArrayKey<8>>(&i.into());
            }
            start.elapsed()
        })
    });

    group.finish();
}

pub fn rand_delete(c: &mut Criterion) {
    let mut group = c.benchmark_group("rand_delete");
    let keys = gen_keys(3, 2, 3);
    let cached_keys = gen_cached_keys(3, 2, 3);

    group.throughput(Throughput::Elements(1));
    group.bench_function("vart", |b| {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, _>::new());
        let mut rng = thread_rng();
        for key in &keys {
            tree.insert::<ArrayKey<16>>(&key.into(), key);
        }
        b.iter(|| {
            let key = &keys[rng.gen_range(0..keys.len())];
            criterion::black_box(tree.remove::<ArrayKey<16>>(&key.into()));
        })
    });

    group.bench_function("vart_cached_keys", |b| {
        let mut tree = PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, _>::new());
        let mut rng = thread_rng();
        for key in &cached_keys {
            tree.insert(&key.0.clone(), key.1.clone());
        }
        b.iter(|| {
            let key = &cached_keys[rng.gen_range(0..keys.len())];
            criterion::black_box(tree.remove(&key.0));
        })
    });

    group.finish();
}

pub fn rand_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_get");

    group.throughput(Throughput::Elements(1));
    {
        let size = 1_000_000;
        group.bench_with_input(BenchmarkId::new("vart", size), &size, |b, size| {
            let mut tree =
                PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<8>, _>::new());
            for i in 0..*size {
                tree.insert::<ArrayKey<8>>(&i.into(), i);
            }
            let mut rng = thread_rng();
            b.iter(|| {
                let key = rng.gen_range(0..*size);
                criterion::black_box(tree.get::<ArrayKey<16>>(&key.into()));
            })
        });
    }

    group.finish();
}

pub fn rand_get_str(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_get_str");
    let keys = gen_keys(3, 2, 3);
    let cached_keys = gen_cached_keys(3, 2, 3);

    group.throughput(Throughput::Elements(1));
    {
        let size = 1_000_000;
        group.bench_with_input(
            BenchmarkId::new("vart_cached_keys", size),
            &size,
            |b, _size| {
                let mut tree =
                    PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, _>::new());
                for (i, key) in cached_keys.iter().enumerate() {
                    tree.insert(&key.0.clone(), i);
                }
                let mut rng = thread_rng();
                b.iter(|| {
                    let key = &cached_keys[rng.gen_range(0..keys.len())];
                    criterion::black_box(tree.get(&key.0));
                })
            },
        );
    }

    {
        let size = 1_000_000;
        group.bench_with_input(BenchmarkId::new("vart", size), &size, |b, _size| {
            let mut tree =
                PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<16>, _>::new());
            for (i, key) in keys.iter().enumerate() {
                tree.insert::<ArrayKey<16>>(&key.into(), i);
            }
            let mut rng = thread_rng();
            b.iter(|| {
                let key = &keys[rng.gen_range(0..keys.len())];
                criterion::black_box(tree.get::<ArrayKey<16>>(&key.into()));
            })
        });
    }

    group.finish();
}

pub fn seq_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("seq_get");

    group.throughput(Throughput::Elements(1));
    {
        let size = 1_000_000;
        group.bench_with_input(BenchmarkId::new("vart", size), &size, |b, size| {
            let mut tree =
                PageableAdaptiveRadixTree::new(VectorNodeStore::<ArrPartial<8>, _>::new());
            for i in 0..*size {
                tree.insert::<ArrayKey<8>>(&i.into(), i);
            }
            let mut key = 0u64;
            b.iter(|| {
                criterion::black_box(tree.get::<ArrayKey<16>>(&key.into()));
                key += 1;
            })
        });
    }

    group.finish();
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

criterion_group!(delete_benches, seq_delete, rand_delete);
criterion_group!(insert_benches, seq_insert, rand_insert);
criterion_group!(retr_benches, seq_get, rand_get, rand_get_str);
criterion_main!(retr_benches, insert_benches, delete_benches);
