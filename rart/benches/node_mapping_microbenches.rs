/// Microbenches for the specific node mapping types and arrangements.
use std::mem::MaybeUninit;
use std::time::Duration;

use micromeasure::{
    BenchContext, BenchmarkRunner, BenchmarkRuntimeOptions, Throughput, benchmark_main, black_box,
};
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

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

const INLINE_EMPTY_SLOT: u8 = u8::MAX;

struct InlineIndexedMapping48<N> {
    child_ptr_indexes: [u8; 256],
    free_slots: [u8; 48],
    children: [MaybeUninit<N>; 48],
    free_len: u8,
    num_children: u8,
}

impl<N> InlineIndexedMapping48<N> {
    fn new() -> Self {
        let mut free_slots = [0; 48];
        for (i, slot) in free_slots.iter_mut().enumerate() {
            *slot = (47 - i) as u8;
        }

        Self {
            child_ptr_indexes: [INLINE_EMPTY_SLOT; 256],
            free_slots,
            children: [const { MaybeUninit::uninit() }; 48],
            free_len: 48,
            num_children: 0,
        }
    }
}

impl<N> Default for InlineIndexedMapping48<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N> NodeMapping<N, 48> for InlineIndexedMapping48<N> {
    fn add_child(&mut self, key: u8, node: N) {
        debug_assert_eq!(self.child_ptr_indexes[key as usize], INLINE_EMPTY_SLOT);
        debug_assert!(self.free_len > 0);

        self.free_len -= 1;
        let pos = self.free_slots[self.free_len as usize] as usize;
        self.child_ptr_indexes[key as usize] = pos as u8;
        self.children[pos].write(node);
        self.num_children += 1;
    }

    fn seek_child(&self, key: u8) -> Option<&N> {
        let pos = self.child_ptr_indexes[key as usize];
        if pos == INLINE_EMPTY_SLOT {
            None
        } else {
            // SAFETY: a live index entry points at an initialized child slot.
            Some(unsafe { self.children[pos as usize].assume_init_ref() })
        }
    }

    fn seek_child_mut(&mut self, key: u8) -> Option<&mut N> {
        let pos = self.child_ptr_indexes[key as usize];
        if pos == INLINE_EMPTY_SLOT {
            None
        } else {
            // SAFETY: a live index entry points at an initialized child slot.
            Some(unsafe { self.children[pos as usize].assume_init_mut() })
        }
    }

    fn delete_child(&mut self, key: u8) -> Option<N> {
        let pos = self.child_ptr_indexes[key as usize];
        if pos == INLINE_EMPTY_SLOT {
            return None;
        }

        let pos = pos as usize;
        self.child_ptr_indexes[key as usize] = INLINE_EMPTY_SLOT;
        self.num_children -= 1;
        self.free_slots[self.free_len as usize] = pos as u8;
        self.free_len += 1;

        // SAFETY: `pos` was looked up from a live index entry.
        Some(unsafe { self.children[pos].assume_init_read() })
    }

    fn num_children(&self) -> usize {
        self.num_children as usize
    }
}

impl<N> Drop for InlineIndexedMapping48<N> {
    fn drop(&mut self) {
        let mut dropped = [false; 48];
        for &slot in &self.child_ptr_indexes {
            if slot == INLINE_EMPTY_SLOT {
                continue;
            }

            let slot = slot as usize;
            if dropped[slot] {
                continue;
            }
            dropped[slot] = true;
            // SAFETY: any live index entry points at an initialized child slot.
            unsafe { self.children[slot].assume_init_drop() };
        }
    }
}

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

struct FilledOccupancyMappingSetContext<const WIDTH: usize, const OCCUPANCY: usize, MappingType> {
    mapping_set: Vec<(MappingType, Vec<u8>, Vec<u8>)>,
}

struct EmptyOccupancyMappingSetContext<const WIDTH: usize, const OCCUPANCY: usize, MappingType> {
    mapping_set: Vec<(MappingType, Vec<u8>, Vec<u8>)>,
}

impl<const WIDTH: usize, const OCCUPANCY: usize, MappingType> BenchContext
    for FilledOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            mapping_set: make_mapping_sets_with_occupancy_and_misses::<WIDTH, OCCUPANCY, MappingType>(
                num_chunks, true,
            ),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const OCCUPANCY: usize, MappingType> BenchContext
    for EmptyOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            mapping_set: make_mapping_sets_with_occupancy_and_misses::<WIDTH, OCCUPANCY, MappingType>(
                num_chunks, false,
            ),
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

fn bench_seek_child_with_occupancy<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    ctx: &mut FilledOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    for (mapping, child_set, _) in ctx.mapping_set.iter_mut().take(chunk_size) {
        for &child in child_set.iter() {
            black_box(mapping.seek_child(child));
        }
    }
}

fn bench_seek_child_miss_with_occupancy<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    ctx: &mut FilledOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    for (mapping, _, miss_keys) in ctx.mapping_set.iter_mut().take(chunk_size) {
        for &child in miss_keys.iter() {
            black_box(mapping.seek_child(child));
        }
    }
}

fn bench_add_child_with_occupancy<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    ctx: &mut EmptyOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    for (mapping, child_set, _) in ctx.mapping_set.iter_mut().take(chunk_size) {
        for &child in child_set.iter() {
            mapping.add_child(child, 0u64);
        }
        black_box(mapping);
    }
}

fn bench_del_child_with_occupancy<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    ctx: &mut FilledOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    for (mapping, child_set, _) in ctx.mapping_set.iter_mut().take(chunk_size) {
        for &child in child_set.iter() {
            black_box(mapping.delete_child(child));
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
    make_mapping_sets_with_occupancy::<WIDTH, WIDTH, MappingType>(num_chunks, prefill)
}

fn make_mapping_sets_with_occupancy<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    num_chunks: usize,
    prefill: bool,
) -> Vec<(MappingType, Vec<u8>)>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    make_mapping_sets_with_occupancy_and_misses::<WIDTH, OCCUPANCY, MappingType>(
        num_chunks, prefill,
    )
    .into_iter()
    .map(|(mapping, hits, _misses)| (mapping, hits))
    .collect()
}

fn make_mapping_sets_with_occupancy_and_misses<
    const WIDTH: usize,
    const OCCUPANCY: usize,
    MappingType,
>(
    num_chunks: usize,
    prefill: bool,
) -> Vec<(MappingType, Vec<u8>, Vec<u8>)>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    debug_assert!(OCCUPANCY <= WIDTH);
    let mut mapping_set = Vec::with_capacity(num_chunks);
    for chunk_idx in 0..num_chunks {
        let child_set = make_child_set::<OCCUPANCY>(chunk_idx as u64);
        let miss_keys = make_miss_set(&child_set, OCCUPANCY);
        let mut mapping = MappingType::default();
        if prefill {
            for &child in &child_set {
                mapping.add_child(child, 0u64);
            }
        }
        mapping_set.push((mapping, child_set, miss_keys));
    }
    mapping_set
}

fn make_child_set<const WIDTH: usize>(seed: u64) -> Vec<u8> {
    let mut keys: Vec<u8> = (0..=u8::MAX).collect();
    let mut rng = SmallRng::seed_from_u64(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    keys.shuffle(&mut rng);
    keys.truncate(WIDTH);
    keys
}

fn make_miss_set(present_keys: &[u8], count: usize) -> Vec<u8> {
    let mut present = [false; 256];
    for &key in present_keys {
        present[key as usize] = true;
    }

    let mut misses = Vec::with_capacity(count);
    for key in 0..=u8::MAX {
        if !present[key as usize] {
            misses.push(key);
            if misses.len() == count {
                break;
            }
        }
    }
    misses
}

fn miss_probe_count(occupancy: usize) -> usize {
    occupancy.min(256 - occupancy)
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

fn register_seek_child_density_bench<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    runner: &BenchmarkRunner,
    name: &'static str,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    runner.group::<FilledOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>>(
        "seek_child_density",
        |g| {
            g.throughput(Throughput::per_operation(OCCUPANCY as u64, "probes"))
                .bench(
                    name,
                    bench_seek_child_with_occupancy::<WIDTH, OCCUPANCY, MappingType>,
                );
        },
    );
}

fn register_seek_child_density_miss_bench<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    runner: &BenchmarkRunner,
    name: &'static str,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    runner.group::<FilledOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>>(
        "seek_child_density_miss",
        |g| {
            g.throughput(Throughput::per_operation(
                miss_probe_count(OCCUPANCY) as u64,
                "probes",
            ))
            .bench(
                name,
                bench_seek_child_miss_with_occupancy::<WIDTH, OCCUPANCY, MappingType>,
            );
        },
    );
}

fn register_add_child_density_bench<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    runner: &BenchmarkRunner,
    name: &'static str,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    runner.group::<EmptyOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>>(
        "add_child_density",
        |g| {
            g.throughput(Throughput::per_operation(OCCUPANCY as u64, "children"))
                .bench(
                    name,
                    bench_add_child_with_occupancy::<WIDTH, OCCUPANCY, MappingType>,
                );
        },
    );
}

fn register_del_child_density_bench<const WIDTH: usize, const OCCUPANCY: usize, MappingType>(
    runner: &BenchmarkRunner,
    name: &'static str,
) where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    runner.group::<FilledOccupancyMappingSetContext<WIDTH, OCCUPANCY, MappingType>>(
        "del_child_density",
        |g| {
            g.throughput(Throughput::per_operation(OCCUPANCY as u64, "children"))
                .bench(
                    name,
                    bench_del_child_with_occupancy::<WIDTH, OCCUPANCY, MappingType>,
                );
        },
    );
}

fn register_seek_child_density_benches(runner: &BenchmarkRunner) {
    register_seek_child_density_bench::<48, 48, IndexedMapping<u64, 48, Bitset64<1>>>(
        runner,
        "indexed48_64x1_occ48",
    );
    register_seek_child_density_bench::<48, 48, InlineIndexedMapping48<u64>>(
        runner,
        "indexed48_inline_occ48",
    );
    register_seek_child_density_bench::<256, 48, DirectMapping<u64>>(runner, "direct_occ48");
    register_seek_child_density_bench::<256, 64, DirectMapping<u64>>(runner, "direct_occ64");
    register_seek_child_density_bench::<256, 96, DirectMapping<u64>>(runner, "direct_occ96");
    register_seek_child_density_bench::<256, 128, DirectMapping<u64>>(runner, "direct_occ128");
    register_seek_child_density_bench::<256, 192, DirectMapping<u64>>(runner, "direct_occ192");
    register_seek_child_density_bench::<256, 256, DirectMapping<u64>>(runner, "direct_occ256");
}

fn register_seek_child_density_miss_benches(runner: &BenchmarkRunner) {
    register_seek_child_density_miss_bench::<48, 48, IndexedMapping<u64, 48, Bitset64<1>>>(
        runner,
        "indexed48_64x1_occ48_miss",
    );
    register_seek_child_density_miss_bench::<48, 48, InlineIndexedMapping48<u64>>(
        runner,
        "indexed48_inline_occ48_miss",
    );
    register_seek_child_density_miss_bench::<256, 48, DirectMapping<u64>>(
        runner,
        "direct_occ48_miss",
    );
    register_seek_child_density_miss_bench::<256, 64, DirectMapping<u64>>(
        runner,
        "direct_occ64_miss",
    );
    register_seek_child_density_miss_bench::<256, 96, DirectMapping<u64>>(
        runner,
        "direct_occ96_miss",
    );
    register_seek_child_density_miss_bench::<256, 128, DirectMapping<u64>>(
        runner,
        "direct_occ128_miss",
    );
    register_seek_child_density_miss_bench::<256, 192, DirectMapping<u64>>(
        runner,
        "direct_occ192_miss",
    );
    register_seek_child_density_miss_bench::<256, 208, DirectMapping<u64>>(
        runner,
        "direct_occ208_miss",
    );
}

fn register_add_child_density_benches(runner: &BenchmarkRunner) {
    register_add_child_density_bench::<48, 48, IndexedMapping<u64, 48, Bitset64<1>>>(
        runner,
        "indexed48_64x1_occ48",
    );
    register_add_child_density_bench::<48, 48, InlineIndexedMapping48<u64>>(
        runner,
        "indexed48_inline_occ48",
    );
    register_add_child_density_bench::<256, 48, DirectMapping<u64>>(runner, "direct_occ48");
    register_add_child_density_bench::<256, 64, DirectMapping<u64>>(runner, "direct_occ64");
    register_add_child_density_bench::<256, 96, DirectMapping<u64>>(runner, "direct_occ96");
    register_add_child_density_bench::<256, 128, DirectMapping<u64>>(runner, "direct_occ128");
    register_add_child_density_bench::<256, 192, DirectMapping<u64>>(runner, "direct_occ192");
    register_add_child_density_bench::<256, 208, DirectMapping<u64>>(runner, "direct_occ208");
}

fn register_del_child_density_benches(runner: &BenchmarkRunner) {
    register_del_child_density_bench::<48, 48, IndexedMapping<u64, 48, Bitset64<1>>>(
        runner,
        "indexed48_64x1_occ48",
    );
    register_del_child_density_bench::<48, 48, InlineIndexedMapping48<u64>>(
        runner,
        "indexed48_inline_occ48",
    );
    register_del_child_density_bench::<256, 48, DirectMapping<u64>>(runner, "direct_occ48");
    register_del_child_density_bench::<256, 64, DirectMapping<u64>>(runner, "direct_occ64");
    register_del_child_density_bench::<256, 96, DirectMapping<u64>>(runner, "direct_occ96");
    register_del_child_density_bench::<256, 128, DirectMapping<u64>>(runner, "direct_occ128");
    register_del_child_density_bench::<256, 192, DirectMapping<u64>>(runner, "direct_occ192");
    register_del_child_density_bench::<256, 208, DirectMapping<u64>>(runner, "direct_occ208");
}

benchmark_main!(|runner| {
    runner.set_runtime(runtime_options());

    // micromeasure's context type is fixed per group, so these families are
    // registered as individual cases under the same logical group names.
    register_grow_node_benches(runner);
    register_add_child_benches(runner);
    register_del_child_benches(runner);
    register_seek_child_benches(runner);
    register_seek_child_density_benches(runner);
    register_seek_child_density_miss_benches(runner);
    register_add_child_density_benches(runner);
    register_del_child_density_benches(runner);
});
