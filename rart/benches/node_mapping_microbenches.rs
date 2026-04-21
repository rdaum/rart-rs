/// Microbenches for the specific node mapping types and arrangements.
use std::collections::HashSet;
use std::time::Duration;

use micromeasure::{
    BenchContext, BenchmarkRunner, BenchmarkRuntimeOptions, Throughput, benchmark_main, black_box,
};

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

fn runtime_options() -> BenchmarkRuntimeOptions {
    if full_bench_profile() {
        BenchmarkRuntimeOptions {
            warm_up_duration: Duration::from_secs(1),
            benchmark_duration: Duration::from_secs(10),
            min_samples: 20,
            max_samples: 100,
        }
    } else {
        BenchmarkRuntimeOptions {
            warm_up_duration: Duration::from_millis(250),
            benchmark_duration: Duration::from_secs(2),
            min_samples: 10,
            max_samples: 40,
        }
    }
}

fn microbench_chunk_size() -> usize {
    if full_bench_profile() { 1024 } else { 128 }
}

struct EmptyMappingSetContext<const WIDTH: usize, MappingType> {
    mapping_set: Vec<(MappingType, Vec<u8>)>,
}

impl<const WIDTH: usize, MappingType> BenchContext for EmptyMappingSetContext<WIDTH, MappingType>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            mapping_set: make_mapping_sets::<WIDTH, MappingType>(num_chunks, false),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

struct FilledMappingSetContext<const WIDTH: usize, MappingType> {
    mapping_set: Vec<(MappingType, Vec<u8>)>,
}

impl<const WIDTH: usize, MappingType> BenchContext for FilledMappingSetContext<WIDTH, MappingType>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            mapping_set: make_mapping_sets::<WIDTH, MappingType>(num_chunks, true),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

fn bench_grow_keyed_node<const FROM_WIDTH: usize, FromBitset, const TO_WIDTH: usize, ToBitset>(
    ctx: &mut FilledMappingSetContext<FROM_WIDTH, KeyedMapping<u64, FROM_WIDTH, FromBitset>>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    FromBitset: BitsetTrait,
    ToBitset: BitsetTrait,
{
    for (mapping, _) in ctx.mapping_set.iter_mut().take(chunk_size) {
        let new_mapping: KeyedMapping<u64, TO_WIDTH, ToBitset> =
            KeyedMapping::from_resized_grow(mapping);
        black_box(new_mapping);
    }
}

fn bench_grow_keyed_to_index<const FROM_WIDTH: usize, FromBitset, const TO_WIDTH: usize, ToBitset>(
    ctx: &mut FilledMappingSetContext<FROM_WIDTH, KeyedMapping<u64, FROM_WIDTH, FromBitset>>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    FromBitset: BitsetTrait,
    ToBitset: BitsetTrait,
{
    for (mapping, _) in ctx.mapping_set.iter_mut().take(chunk_size) {
        let new_mapping: IndexedMapping<u64, TO_WIDTH, ToBitset> =
            IndexedMapping::from_keyed(mapping);
        black_box(new_mapping);
    }
}

fn bench_grow_indexed_to_direct<const FROM_WIDTH: usize, FromBitset>(
    ctx: &mut FilledMappingSetContext<FROM_WIDTH, IndexedMapping<u64, FROM_WIDTH, FromBitset>>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    FromBitset: BitsetTrait,
{
    for (mapping, _) in ctx.mapping_set.iter_mut().take(chunk_size) {
        let new_mapping: DirectMapping<u64> = DirectMapping::from_indexed(mapping);
        black_box(new_mapping);
    }
}

fn bench_add_child<const WIDTH: usize, MappingType>(
    ctx: &mut EmptyMappingSetContext<WIDTH, MappingType>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    for (mapping, child_set) in ctx.mapping_set.iter_mut().take(chunk_size) {
        for &child in child_set.iter() {
            mapping.add_child(child, 0u64);
        }
        black_box(mapping);
    }
}

fn bench_del_child<const WIDTH: usize, MappingType>(
    ctx: &mut FilledMappingSetContext<WIDTH, MappingType>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    for (mapping, child_set) in ctx.mapping_set.iter_mut().take(chunk_size) {
        for &child in child_set.iter() {
            black_box(mapping.delete_child(child));
        }
    }
}

fn bench_seek_child<const WIDTH: usize, MappingType>(
    ctx: &mut FilledMappingSetContext<WIDTH, MappingType>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    for (mapping, child_set) in ctx.mapping_set.iter_mut().take(chunk_size) {
        for &child in child_set.iter() {
            black_box(mapping.seek_child(child));
        }
    }
}

fn make_mapping_sets<const WIDTH: usize, MappingType>(
    num_chunks: usize,
    prefill: bool,
) -> Vec<(MappingType, Vec<u8>)>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    let mut mapping_set = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        let child_set = make_child_set::<WIDTH>();
        let mut mapping = MappingType::default();
        if prefill {
            for &child in &child_set {
                mapping.add_child(child, 0u64);
            }
        }
        mapping_set.push((mapping, child_set));
    }
    mapping_set
}

fn make_child_set<const WIDTH: usize>() -> Vec<u8> {
    let mut child_hash_set = HashSet::with_capacity(WIDTH);
    while child_hash_set.len() < WIDTH {
        child_hash_set.insert(rand::random::<u8>());
    }
    child_hash_set.into_iter().collect()
}

fn register_grow_node_benches(runner: &BenchmarkRunner) {
    runner.group::<FilledMappingSetContext<4, KeyedMapping<u64, 4, Bitset8<1>>>>(
        "grow_node",
        |g| {
            g.throughput(Throughput::ops()).bench(
                "n4_to_n16_16x1",
                bench_grow_keyed_node::<4, Bitset8<1>, 16, Bitset16<1>>,
            );
            g.throughput(Throughput::ops()).bench(
                "n4_to_n16_8x2",
                bench_grow_keyed_node::<4, Bitset8<1>, 16, Bitset8<2>>,
            );
            g.throughput(Throughput::ops()).bench(
                "n16_to_n32_32x1",
                bench_grow_keyed_node::<4, Bitset8<1>, 16, Bitset32<1>>,
            );
            g.throughput(Throughput::ops()).bench(
                "n16_to_n32_16x2",
                bench_grow_keyed_node::<4, Bitset8<1>, 16, Bitset16<2>>,
            );
            g.throughput(Throughput::ops()).bench(
                "n16_to_n32_8x4",
                bench_grow_keyed_node::<4, Bitset8<1>, 16, Bitset8<4>>,
            );
        },
    );

    runner.group::<FilledMappingSetContext<32, KeyedMapping<u64, 32, Bitset32<1>>>>(
        "grow_node",
        |g| {
            g.throughput(Throughput::ops()).bench(
                "n32_32x1_to_n48_16x3",
                bench_grow_keyed_to_index::<32, Bitset32<1>, 48, Bitset16<3>>,
            );
        },
    );

    runner.group::<FilledMappingSetContext<48, IndexedMapping<u64, 48, Bitset16<3>>>>(
        "grow_node",
        |g| {
            g.throughput(Throughput::ops()).bench(
                "n48_16x3_to_direct",
                bench_grow_indexed_to_direct::<48, Bitset16<3>>,
            );
        },
    );
}

fn register_add_child_bench<const WIDTH: usize, MappingType>(
    runner: &BenchmarkRunner,
    name: &'static str,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    runner.group::<EmptyMappingSetContext<WIDTH, MappingType>>("add_child", |g| {
        g.throughput(Throughput::per_operation(WIDTH as u64, "children"))
            .bench(name, bench_add_child::<WIDTH, MappingType>);
    });
}

fn register_del_child_bench<const WIDTH: usize, MappingType>(
    runner: &BenchmarkRunner,
    name: &'static str,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    runner.group::<FilledMappingSetContext<WIDTH, MappingType>>("del_child", |g| {
        g.throughput(Throughput::per_operation(WIDTH as u64, "children"))
            .bench(name, bench_del_child::<WIDTH, MappingType>);
    });
}

fn register_seek_child_bench<const WIDTH: usize, MappingType>(
    runner: &BenchmarkRunner,
    name: &'static str,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    runner.group::<FilledMappingSetContext<WIDTH, MappingType>>("seek_child", |g| {
        g.throughput(Throughput::per_operation(WIDTH as u64, "children"))
            .bench(name, bench_seek_child::<WIDTH, MappingType>);
    });
}

fn register_add_child_benches(runner: &BenchmarkRunner) {
    register_add_child_bench::<256, DirectMapping<u64>>(runner, "direct");
    register_add_child_bench::<48, IndexedMapping<u64, 48, Bitset16<3>>>(runner, "indexed48_16x3");
    register_add_child_bench::<48, IndexedMapping<u64, 48, Bitset8<6>>>(runner, "indexed48_8x6");
    register_add_child_bench::<48, IndexedMapping<u64, 48, Bitset32<2>>>(runner, "indexed48_32x2");
    register_add_child_bench::<48, IndexedMapping<u64, 48, Bitset64<1>>>(runner, "indexed48_64x1");
    register_add_child_bench::<32, KeyMapping32_32x1>(runner, "keyed32_32x1");
    register_add_child_bench::<32, KeyMapping32_16x2>(runner, "keyed32_16x2");
    register_add_child_bench::<32, KeyMapping32_8x4>(runner, "keyed32_8x4");
    register_add_child_bench::<16, KeyMapping16_16x1>(runner, "keyed16_16x1");
    register_add_child_bench::<16, KeyMapping16_8x2>(runner, "keyed16_8x2");
    register_add_child_bench::<4, KeyMapping4>(runner, "keyed4");
    register_add_child_bench::<32, SortedKeyedMapping<u64, 32>>(runner, "sorted_keyed32");
    register_add_child_bench::<16, SortedKeyedMapping<u64, 16>>(runner, "sorted_keyed16");
    register_add_child_bench::<4, SortedKeyedMapping<u64, 4>>(runner, "sorted_keyed4");
}

fn register_del_child_benches(runner: &BenchmarkRunner) {
    register_del_child_bench::<256, DirectMapping<u64>>(runner, "direct");
    register_del_child_bench::<48, IndexedMapping<u64, 48, Bitset16<3>>>(runner, "indexed48_16x3");
    register_del_child_bench::<48, IndexedMapping<u64, 48, Bitset8<6>>>(runner, "indexed48_8x6");
    register_del_child_bench::<48, IndexedMapping<u64, 48, Bitset32<2>>>(runner, "indexed48_32x2");
    register_del_child_bench::<48, IndexedMapping<u64, 48, Bitset64<1>>>(runner, "indexed48_64x1");
    register_del_child_bench::<32, KeyMapping32_32x1>(runner, "keyed32_32x1");
    register_del_child_bench::<32, KeyMapping32_16x2>(runner, "keyed32_16x2");
    register_del_child_bench::<32, KeyMapping32_8x4>(runner, "keyed32_8x4");
    register_del_child_bench::<16, KeyMapping16_16x1>(runner, "keyed16_16x1");
    register_del_child_bench::<16, KeyMapping16_8x2>(runner, "keyed16_8x2");
    register_del_child_bench::<4, KeyMapping4>(runner, "keyed4");
    register_del_child_bench::<32, SortedKeyedMapping<u64, 32>>(runner, "sorted_keyed32");
    register_del_child_bench::<16, SortedKeyedMapping<u64, 16>>(runner, "sorted_keyed16");
    register_del_child_bench::<4, SortedKeyedMapping<u64, 4>>(runner, "sorted_keyed4");
}

fn register_seek_child_benches(runner: &BenchmarkRunner) {
    register_seek_child_bench::<256, DirectMapping<u64>>(runner, "direct");
    register_seek_child_bench::<48, IndexedMapping<u64, 48, Bitset16<3>>>(runner, "indexed48_16x3");
    register_seek_child_bench::<48, IndexedMapping<u64, 48, Bitset8<6>>>(runner, "indexed48_8x6");
    register_seek_child_bench::<48, IndexedMapping<u64, 48, Bitset32<2>>>(runner, "indexed48_32x2");
    register_seek_child_bench::<48, IndexedMapping<u64, 48, Bitset64<1>>>(runner, "indexed48_64x1");
    register_seek_child_bench::<32, KeyMapping32_32x1>(runner, "keyed32_32x1");
    register_seek_child_bench::<32, KeyMapping32_16x2>(runner, "keyed32_16x2");
    register_seek_child_bench::<32, KeyMapping32_8x4>(runner, "keyed32_8x4");
    register_seek_child_bench::<16, KeyMapping16_16x1>(runner, "keyed16_16x1");
    register_seek_child_bench::<16, KeyMapping16_8x2>(runner, "keyed16_8x2");
    register_seek_child_bench::<4, KeyMapping4>(runner, "keyed4");
    register_seek_child_bench::<32, SortedKeyedMapping<u64, 32>>(runner, "sorted_keyed32");
    register_seek_child_bench::<16, SortedKeyedMapping<u64, 16>>(runner, "sorted_keyed16");
    register_seek_child_bench::<4, SortedKeyedMapping<u64, 4>>(runner, "sorted_keyed4");
}

benchmark_main!(|runner| {
    runner.set_runtime(runtime_options());

    // micromeasure's context type is fixed per group, so these families are
    // registered as individual cases under the same logical group names.
    register_grow_node_benches(runner);
    register_add_child_benches(runner);
    register_del_child_benches(runner);
    register_seek_child_benches(runner);
});
