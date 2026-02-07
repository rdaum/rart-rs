/// Microbenches for the specific node mapping types and arrangements.
/// Takes quite a while to run.
use std::collections::HashSet;
use std::time::{Duration, Instant};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

use rart::mapping::NodeMapping;
use rart::mapping::direct_mapping::DirectMapping;
use rart::mapping::indexed_mapping::IndexedMapping;
use rart::mapping::keyed_mapping::KeyedMapping;
use rart::mapping::sorted_keyed_mapping::SortedKeyedMapping;
use rart::utils::bitset::{Bitset8, Bitset16, Bitset32, Bitset64, BitsetTrait};

type KeyMapping32_32x1 = KeyedMapping<u64, 32, Bitset32<1>>;
type KeyMapping32_16x2 = KeyedMapping<u64, 32, Bitset16<2>>;
type KeyMapping32_8x4 = KeyedMapping<u64, 32, Bitset8<4>>;

type KeyMapping16_16x1 = KeyedMapping<u64, 16, Bitset16<1>>;
type KeyMapping16_8x2 = KeyedMapping<u64, 16, Bitset8<2>>;

type KeyMapping4 = KeyedMapping<u64, 4, Bitset8<1>>;

fn full_bench_profile() -> bool {
    std::env::var("RART_BENCH_FULL").as_deref() == Ok("1")
}

fn microbench_sample_size() -> usize {
    if full_bench_profile() { 4096 } else { 256 }
}

fn microbench_measurement_time() -> Duration {
    if full_bench_profile() {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(2)
    }
}

fn benched_grow_keyed_node<const FROM_WIDTH: usize, FromBitset, const TO_WIDTH: usize, ToBitset>(
    iters: u64,
) -> Duration
where
    FromBitset: BitsetTrait,
    ToBitset: BitsetTrait,
{
    // Create the nodes
    let mut mapping_set =
        make_mapping_sets::<FROM_WIDTH, KeyedMapping<u64, FROM_WIDTH, FromBitset>>(iters);

    // Fill them up with children
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }

    // Now go through and grow each node.
    let start = Instant::now();
    for (mapping, _child_set) in &mut mapping_set {
        let _new: KeyedMapping<u64, TO_WIDTH, ToBitset> = KeyedMapping::from_resized_grow(mapping);
    }
    start.elapsed()
}

fn benched_grow_keyed_to_index<
    const FROM_WIDTH: usize,
    FromBitset,
    const TO_WIDTH: usize,
    ToBitset,
>(
    iters: u64,
) -> Duration
where
    FromBitset: BitsetTrait,
    ToBitset: BitsetTrait,
{
    // Create the nodes
    let mut mapping_set =
        make_mapping_sets::<FROM_WIDTH, KeyedMapping<u64, FROM_WIDTH, FromBitset>>(iters);

    // Fill them up with children
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }

    let start = Instant::now();
    for (mapping, _child_set) in &mut mapping_set {
        let _new: IndexedMapping<u64, TO_WIDTH, ToBitset> = IndexedMapping::from_keyed(mapping);
    }
    start.elapsed()
}

fn bench_grow_indexed_to_direct<const FROM_WIDTH: usize, FromBitset: BitsetTrait>(
    iters: u64,
) -> Duration {
    // Create the nodes
    let mut mapping_set =
        make_mapping_sets::<FROM_WIDTH, IndexedMapping<u64, FROM_WIDTH, FromBitset>>(iters);

    // Fill them up with children
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }

    let start = Instant::now();
    for (mapping, _child_set) in &mut mapping_set {
        let _new: DirectMapping<u64> = DirectMapping::from_indexed(mapping);
    }
    start.elapsed()
}

fn benched_add_child<const WIDTH: usize, MappingType>(iters: u64) -> Duration
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    let mut mapping_set = make_mapping_sets::<WIDTH, MappingType>(iters);
    let start = Instant::now();
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }
    start.elapsed()
}

fn benched_del_child<const WIDTH: usize, MappingType>(iters: u64) -> Duration
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    let mut mapping_set = make_mapping_sets::<WIDTH, MappingType>(iters);
    // Fill all the sets.
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }

    // Then time the deletion only.
    let start = Instant::now();
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.delete_child(*child);
        }
    }
    start.elapsed()
}

fn benched_seek_child<const WIDTH: usize, MappingType>(iters: u64) -> Duration
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    let mut mapping_set = make_mapping_sets::<WIDTH, MappingType>(iters);
    // Fill all the sets.
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }

    // Then time the find only.
    let start = Instant::now();
    for (mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.seek_child(*child);
        }
    }
    start.elapsed()
}

fn make_mapping_sets<const WIDTH: usize, MappingType>(iters: u64) -> Vec<(MappingType, Vec<u8>)>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    // Break iters into 256-chunks, and prepare child sets and mappings for each chunk.
    let mut mapping_set = Vec::with_capacity((iters / (WIDTH as u64)) as usize);
    for _ in 0..iters / (WIDTH as u64) {
        // Produce a random set of unique child keys to add, WIDTH-wide. The same child cannot
        // be present twice in the same set.
        let mut child_hash_set = HashSet::with_capacity(WIDTH);
        while child_hash_set.len() < WIDTH {
            child_hash_set.insert(rand::random::<u8>());
        }
        let child_set = child_hash_set.into_iter().collect::<Vec<u8>>();
        mapping_set.push((MappingType::default(), child_set));
    }
    mapping_set
}

pub fn grow_node(c: &mut Criterion) {
    let mut group = c.benchmark_group("grow_node");
    group.throughput(Throughput::Elements(1));
    group.sample_size(microbench_sample_size());
    group.measurement_time(microbench_measurement_time());

    group.bench_function("n4_to_n16_16x1", |b| {
        b.iter_custom(benched_grow_keyed_node::<4, Bitset8<1>, 16, Bitset16<1>>);
    });
    group.bench_function("n4_to_n16_8x2", |b| {
        b.iter_custom(benched_grow_keyed_node::<4, Bitset8<1>, 16, Bitset8<2>>);
    });

    group.bench_function("n16_to_n32_32x1", |b| {
        b.iter_custom(benched_grow_keyed_node::<4, Bitset8<1>, 16, Bitset32<1>>);
    });
    group.bench_function("n16_to_n32_16x2", |b| {
        b.iter_custom(benched_grow_keyed_node::<4, Bitset8<1>, 16, Bitset16<2>>);
    });
    group.bench_function("n16_to_n32_8x4", |b| {
        b.iter_custom(benched_grow_keyed_node::<4, Bitset8<1>, 16, Bitset8<4>>);
    });

    group.bench_function("n32_32x1_to_n48_16x3", |b| {
        b.iter_custom(benched_grow_keyed_to_index::<32, Bitset32<1>, 48, Bitset16<3>>);
    });

    group.bench_function("n48_16x3_to_direct", |b| {
        b.iter_custom(bench_grow_indexed_to_direct::<48, Bitset16<3>>);
    });
}

pub fn add_child(c: &mut Criterion) {
    let mut group = c.benchmark_group("add_child");
    group.throughput(Throughput::Elements(1));
    group.sample_size(microbench_sample_size());
    group.measurement_time(microbench_measurement_time());

    group.bench_function("direct", |b| {
        b.iter_custom(benched_add_child::<256, DirectMapping<u64>>);
    });

    group.bench_function("indexed48_16x3", |b| {
        b.iter_custom(benched_add_child::<48, IndexedMapping<u64, 48, Bitset16<3>>>)
    });

    group.bench_function("indexed48_8x6", |b| {
        b.iter_custom(benched_add_child::<48, IndexedMapping<u64, 48, Bitset8<6>>>)
    });

    group.bench_function("indexed48_32x2", |b| {
        b.iter_custom(benched_add_child::<48, IndexedMapping<u64, 48, Bitset32<2>>>)
    });

    group.bench_function("indexed48_64x1", |b| {
        b.iter_custom(benched_add_child::<48, IndexedMapping<u64, 48, Bitset64<1>>>)
    });

    group.bench_function("keyed32_32x1", |b| {
        b.iter_custom(benched_add_child::<32, KeyMapping32_32x1>)
    });

    group.bench_function("keyed32_16x2", |b| {
        b.iter_custom(benched_add_child::<32, KeyMapping32_16x2>)
    });

    group.bench_function("keyed32_8x4", |b| {
        b.iter_custom(benched_add_child::<32, KeyMapping32_8x4>)
    });

    group.bench_function("keyed16_16x1", |b| {
        b.iter_custom(benched_add_child::<16, KeyMapping16_16x1>)
    });

    group.bench_function("keyed16_8x2", |b| {
        b.iter_custom(benched_add_child::<16, KeyMapping16_8x2>)
    });

    group.bench_function("keyed4", |b| {
        b.iter_custom(benched_add_child::<4, KeyMapping4>)
    });

    group.bench_function("sorted_keyed32", |b| {
        b.iter_custom(benched_add_child::<32, SortedKeyedMapping<u64, 32>>)
    });

    group.bench_function("sorted_keyed16", |b| {
        b.iter_custom(benched_add_child::<16, SortedKeyedMapping<u64, 16>>)
    });

    group.bench_function("sorted_keyed4", |b| {
        b.iter_custom(benched_add_child::<4, SortedKeyedMapping<u64, 4>>)
    });

    group.finish();
}

pub fn del_child(c: &mut Criterion) {
    let mut group = c.benchmark_group("del_child");
    group.throughput(Throughput::Elements(1));
    group.sample_size(microbench_sample_size());
    group.measurement_time(microbench_measurement_time());

    group.bench_function("direct", |b| {
        b.iter_custom(benched_del_child::<256, DirectMapping<u64>>);
    });

    group.bench_function("indexed48_16x3", |b| {
        b.iter_custom(benched_del_child::<48, IndexedMapping<u64, 48, Bitset16<3>>>)
    });

    group.bench_function("indexed48_8x6", |b| {
        b.iter_custom(benched_del_child::<48, IndexedMapping<u64, 48, Bitset8<6>>>)
    });

    group.bench_function("indexed48_32x2", |b| {
        b.iter_custom(benched_del_child::<48, IndexedMapping<u64, 48, Bitset32<2>>>)
    });

    group.bench_function("indexed48_64x1", |b| {
        b.iter_custom(benched_del_child::<48, IndexedMapping<u64, 48, Bitset64<1>>>)
    });

    group.bench_function("keyed32_32x1", |b| {
        b.iter_custom(benched_del_child::<32, KeyMapping32_32x1>)
    });

    group.bench_function("keyed32_16x2", |b| {
        b.iter_custom(benched_del_child::<32, KeyMapping32_16x2>)
    });

    group.bench_function("keyed32_8x4", |b| {
        b.iter_custom(benched_del_child::<32, KeyMapping32_8x4>)
    });

    group.bench_function("keyed16_16x1", |b| {
        b.iter_custom(benched_del_child::<16, KeyMapping16_16x1>)
    });

    group.bench_function("keyed16_8x2", |b| {
        b.iter_custom(benched_del_child::<16, KeyMapping16_8x2>)
    });
    group.bench_function("keyed4", |b| {
        b.iter_custom(benched_del_child::<4, KeyMapping4>)
    });

    group.bench_function("sorted_keyed32", |b| {
        b.iter_custom(benched_del_child::<32, SortedKeyedMapping<u64, 32>>)
    });

    group.bench_function("sorted_keyed16", |b| {
        b.iter_custom(benched_del_child::<16, SortedKeyedMapping<u64, 16>>)
    });

    group.bench_function("sorted_keyed4", |b| {
        b.iter_custom(benched_del_child::<4, SortedKeyedMapping<u64, 4>>)
    });

    group.finish();
}

pub fn seek_child(c: &mut Criterion) {
    let mut group = c.benchmark_group("seek_child");
    group.throughput(Throughput::Elements(1));
    group.sample_size(microbench_sample_size());
    group.measurement_time(microbench_measurement_time());

    group.bench_function("direct", |b| {
        b.iter_custom(benched_seek_child::<256, DirectMapping<u64>>);
    });

    group.bench_function("indexed48_16x3", |b| {
        b.iter_custom(benched_seek_child::<48, IndexedMapping<u64, 48, Bitset16<3>>>)
    });

    group.bench_function("indexed48_8x6", |b| {
        b.iter_custom(benched_seek_child::<48, IndexedMapping<u64, 48, Bitset8<6>>>)
    });

    group.bench_function("indexed48_32x2", |b| {
        b.iter_custom(benched_seek_child::<48, IndexedMapping<u64, 48, Bitset32<2>>>)
    });

    group.bench_function("indexed48_64x1", |b| {
        b.iter_custom(benched_seek_child::<48, IndexedMapping<u64, 48, Bitset64<1>>>)
    });

    group.bench_function("keyed32_32x1", |b| {
        b.iter_custom(benched_seek_child::<32, KeyMapping32_32x1>)
    });

    group.bench_function("keyed32_16x2", |b| {
        b.iter_custom(benched_seek_child::<32, KeyMapping32_16x2>)
    });

    group.bench_function("keyed32_8x4", |b| {
        b.iter_custom(benched_seek_child::<32, KeyMapping32_8x4>)
    });

    group.bench_function("keyed16_16x1", |b| {
        b.iter_custom(benched_seek_child::<16, KeyMapping16_16x1>)
    });

    group.bench_function("keyed16_8x2", |b| {
        b.iter_custom(benched_seek_child::<16, KeyMapping16_8x2>)
    });

    group.bench_function("keyed4", |b| {
        b.iter_custom(benched_seek_child::<4, KeyMapping4>)
    });

    group.bench_function("sorted_keyed32", |b| {
        b.iter_custom(benched_seek_child::<32, SortedKeyedMapping<u64, 32>>)
    });

    group.bench_function("sorted_keyed16", |b| {
        b.iter_custom(benched_seek_child::<16, SortedKeyedMapping<u64, 16>>)
    });

    group.bench_function("sorted_keyed4", |b| {
        b.iter_custom(benched_seek_child::<4, SortedKeyedMapping<u64, 4>>)
    });

    group.finish();
}

criterion_group!(benches, grow_node, add_child, del_child, seek_child);
criterion_main!(benches);
