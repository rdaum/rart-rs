/// Microbenches for the specific node mapping types and arrangements.
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
use rart::utils::u8_keys::{
    u8_keys_find_insert_position_sorted, u8_keys_find_key_position,
    u8_keys_find_key_position_sorted,
};

#[cfg(feature = "simd_keys")]
mod node4_simd_experiment {
    use simdeez::*;
    use simdeez::{prelude::*, simd_runtime_generate};

    simd_runtime_generate!(
        pub fn find_key_padded(key: u8, keys: &[u8; 16], active_mask: u32) -> Option<usize> {
            let key_cmp_vec = S::Vi8::set1(key as i8);
            let i8_keys: &[i8; 16] = unsafe { std::mem::transmute(keys) };
            let key_vec = S::Vi8::load_from_slice(i8_keys);
            let results = key_cmp_vec.cmp_eq(key_vec);
            let bitfield = results.get_mask() & active_mask;
            if bitfield != 0 {
                Some(bitfield.trailing_zeros() as usize)
            } else {
                None
            }
        }
    );
}

type KeyMapping32_32x1 = KeyedMapping<u64, 32, Bitset32<1>>;
type KeyMapping32_16x2 = KeyedMapping<u64, 32, Bitset16<2>>;
type KeyMapping32_8x4 = KeyedMapping<u64, 32, Bitset8<4>>;

type KeyMapping16_16x1 = KeyedMapping<u64, 16, Bitset16<1>>;
type KeyMapping16_8x2 = KeyedMapping<u64, 16, Bitset8<2>>;

type KeyMapping4 = KeyedMapping<u64, 4, Bitset8<1>>;

struct SortedKeyProbeContext<const WIDTH: usize, const NUM_CHILDREN: usize> {
    probes: Vec<([u8; WIDTH], u8)>,
}

struct UnsortedKeyProbeContext<const WIDTH: usize, const NUM_CHILDREN: usize, Bitset> {
    probes: Vec<([u8; WIDTH], Bitset, u8)>,
}

struct SortedKeyInsertContext<const WIDTH: usize, const NUM_CHILDREN: usize> {
    probes: Vec<([u8; WIDTH], u8)>,
}

struct SortedNodeSeekContext<const WIDTH: usize> {
    probes: Vec<(SortedKeyedMapping<u64, WIDTH>, u8)>,
}

struct SortedNodeInsertContext<const WIDTH: usize> {
    probes: Vec<(SortedKeyedMapping<u64, WIDTH>, u8)>,
}

struct SortedKeyMissProbeContext<const WIDTH: usize, const NUM_CHILDREN: usize> {
    probes: Vec<([u8; WIDTH], u8)>,
}

struct SortedKeyEdgeProbeContext<const WIDTH: usize, const NUM_CHILDREN: usize, const INDEX: usize>
{
    probes: Vec<([u8; WIDTH], u8)>,
}

struct SortedNodeSeekMissContext<const WIDTH: usize> {
    probes: Vec<(SortedKeyedMapping<u64, WIDTH>, u8)>,
}

struct SortedNodeSeekEdgeContext<const WIDTH: usize, const INDEX: usize> {
    probes: Vec<(SortedKeyedMapping<u64, WIDTH>, u8)>,
}

struct SortedKeyMixedProbeContext<
    const WIDTH: usize,
    const NUM_CHILDREN: usize,
    const MISS_INTERVAL: usize,
> {
    probes: Vec<([u8; WIDTH], Vec<u8>)>,
}

struct SortedNodeSeekMixedContext<const WIDTH: usize, const MISS_INTERVAL: usize> {
    probes: Vec<(SortedKeyedMapping<u64, WIDTH>, Vec<u8>)>,
}

struct Node4SearchExperimentContext {
    probes: Vec<([u8; 16], Vec<u8>)>,
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

impl<const WIDTH: usize, const NUM_CHILDREN: usize> BenchContext
    for SortedKeyProbeContext<WIDTH, NUM_CHILDREN>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_key_probes::<WIDTH, NUM_CHILDREN>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const NUM_CHILDREN: usize, Bitset> BenchContext
    for UnsortedKeyProbeContext<WIDTH, NUM_CHILDREN, Bitset>
where
    Bitset: BitsetTrait + Default,
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_unsorted_key_probes::<WIDTH, NUM_CHILDREN, Bitset>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const NUM_CHILDREN: usize> BenchContext
    for SortedKeyInsertContext<WIDTH, NUM_CHILDREN>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_insert_probes::<WIDTH, NUM_CHILDREN>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize> BenchContext for SortedNodeSeekContext<WIDTH> {
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_node_seek_probes::<WIDTH>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize> BenchContext for SortedNodeInsertContext<WIDTH> {
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_node_insert_probes::<WIDTH>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const NUM_CHILDREN: usize> BenchContext
    for SortedKeyMissProbeContext<WIDTH, NUM_CHILDREN>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_key_miss_probes::<WIDTH, NUM_CHILDREN>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const NUM_CHILDREN: usize, const INDEX: usize> BenchContext
    for SortedKeyEdgeProbeContext<WIDTH, NUM_CHILDREN, INDEX>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_key_edge_probes::<WIDTH, NUM_CHILDREN, INDEX>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize> BenchContext for SortedNodeSeekMissContext<WIDTH> {
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_node_seek_miss_probes::<WIDTH>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const INDEX: usize> BenchContext
    for SortedNodeSeekEdgeContext<WIDTH, INDEX>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_node_seek_edge_probes::<WIDTH, INDEX>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const NUM_CHILDREN: usize, const MISS_INTERVAL: usize> BenchContext
    for SortedKeyMixedProbeContext<WIDTH, NUM_CHILDREN, MISS_INTERVAL>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_key_mixed_probes::<WIDTH, NUM_CHILDREN, MISS_INTERVAL>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl<const WIDTH: usize, const MISS_INTERVAL: usize> BenchContext
    for SortedNodeSeekMixedContext<WIDTH, MISS_INTERVAL>
{
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_sorted_node_seek_mixed_probes::<WIDTH, MISS_INTERVAL>(num_chunks),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(microbench_chunk_size())
    }
}

impl BenchContext for Node4SearchExperimentContext {
    fn prepare(num_chunks: usize) -> Self {
        Self {
            probes: make_node4_search_experiment_probes(num_chunks),
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

fn bench_sorted_key_seek<const WIDTH: usize, const NUM_CHILDREN: usize>(
    ctx: &mut SortedKeyProbeContext<WIDTH, NUM_CHILDREN>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(u8_keys_find_key_position_sorted::<WIDTH>(
            *probe,
            keys,
            NUM_CHILDREN,
        ));
    }
}

fn bench_unsorted_key_seek<const WIDTH: usize, const NUM_CHILDREN: usize, Bitset>(
    ctx: &mut UnsortedKeyProbeContext<WIDTH, NUM_CHILDREN, Bitset>,
    chunk_size: usize,
    _chunk_num: usize,
) where
    Bitset: BitsetTrait + Default,
{
    for (keys, bitset, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(u8_keys_find_key_position::<WIDTH, Bitset>(
            *probe, keys, bitset,
        ));
    }
}

fn bench_sorted_key_insert_position<const WIDTH: usize, const NUM_CHILDREN: usize>(
    ctx: &mut SortedKeyInsertContext<WIDTH, NUM_CHILDREN>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(u8_keys_find_insert_position_sorted::<WIDTH>(
            *probe,
            keys,
            NUM_CHILDREN,
        ));
    }
}

fn bench_sorted_key_seek_miss<const WIDTH: usize, const NUM_CHILDREN: usize>(
    ctx: &mut SortedKeyMissProbeContext<WIDTH, NUM_CHILDREN>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(u8_keys_find_key_position_sorted::<WIDTH>(
            *probe,
            keys,
            NUM_CHILDREN,
        ));
    }
}

fn bench_sorted_key_seek_edge<const WIDTH: usize, const NUM_CHILDREN: usize, const INDEX: usize>(
    ctx: &mut SortedKeyEdgeProbeContext<WIDTH, NUM_CHILDREN, INDEX>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(u8_keys_find_key_position_sorted::<WIDTH>(
            *probe,
            keys,
            NUM_CHILDREN,
        ));
    }
}

fn bench_sorted_node_seek<const WIDTH: usize>(
    ctx: &mut SortedNodeSeekContext<WIDTH>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (mapping, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(mapping.seek_child(*probe));
    }
}

fn bench_sorted_node_seek_miss<const WIDTH: usize>(
    ctx: &mut SortedNodeSeekMissContext<WIDTH>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (mapping, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(mapping.seek_child(*probe));
    }
}

fn bench_sorted_node_seek_edge<const WIDTH: usize, const INDEX: usize>(
    ctx: &mut SortedNodeSeekEdgeContext<WIDTH, INDEX>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (mapping, probe) in ctx.probes.iter().take(chunk_size) {
        black_box(mapping.seek_child(*probe));
    }
}

fn bench_sorted_key_seek_mixed<
    const WIDTH: usize,
    const NUM_CHILDREN: usize,
    const MISS_INTERVAL: usize,
>(
    ctx: &mut SortedKeyMixedProbeContext<WIDTH, NUM_CHILDREN, MISS_INTERVAL>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probes) in ctx.probes.iter().take(chunk_size) {
        for &probe in probes {
            black_box(u8_keys_find_key_position_sorted::<WIDTH>(
                probe,
                keys,
                NUM_CHILDREN,
            ));
        }
    }
}

fn bench_sorted_node_seek_mixed<const WIDTH: usize, const MISS_INTERVAL: usize>(
    ctx: &mut SortedNodeSeekMixedContext<WIDTH, MISS_INTERVAL>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (mapping, probes) in ctx.probes.iter().take(chunk_size) {
        for &probe in probes {
            black_box(mapping.seek_child(probe));
        }
    }
}

fn node4_find_key_linear(key: u8, keys: &[u8; 16]) -> Option<usize> {
    keys[..4].iter().position(|&candidate| candidate == key)
}

fn node4_find_key_unrolled(key: u8, keys: &[u8; 16]) -> Option<usize> {
    if keys[0] == key {
        Some(0)
    } else if keys[1] == key {
        Some(1)
    } else if keys[2] == key {
        Some(2)
    } else if keys[3] == key {
        Some(3)
    } else {
        None
    }
}

fn bench_node4_search_linear(
    ctx: &mut Node4SearchExperimentContext,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probes) in ctx.probes.iter().take(chunk_size) {
        for &probe in probes {
            black_box(node4_find_key_linear(probe, keys));
        }
    }
}

fn bench_node4_search_unrolled(
    ctx: &mut Node4SearchExperimentContext,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probes) in ctx.probes.iter().take(chunk_size) {
        for &probe in probes {
            black_box(node4_find_key_unrolled(probe, keys));
        }
    }
}

#[cfg(feature = "simd_keys")]
fn bench_node4_search_simd_padded(
    ctx: &mut Node4SearchExperimentContext,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (keys, probes) in ctx.probes.iter().take(chunk_size) {
        for &probe in probes {
            black_box(node4_simd_experiment::find_key_padded(probe, keys, 0b1111));
        }
    }
}

fn bench_sorted_node_insert<const WIDTH: usize>(
    ctx: &mut SortedNodeInsertContext<WIDTH>,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for (mapping, probe) in ctx.probes.iter_mut().take(chunk_size) {
        mapping.add_child(*probe, 0);
        black_box(mapping.num_children());
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

fn make_sorted_keys<const WIDTH: usize, const NUM_CHILDREN: usize>(seed: u64) -> [u8; WIDTH] {
    let mut keys = [255; WIDTH];
    let mut child_set = make_child_set::<NUM_CHILDREN>(seed);
    child_set.sort_unstable();
    for (dst, src) in keys.iter_mut().zip(child_set.into_iter()) {
        *dst = src;
    }
    keys
}

fn make_unsorted_keys<const WIDTH: usize, const NUM_CHILDREN: usize>(seed: u64) -> [u8; WIDTH] {
    let mut keys = [255; WIDTH];
    let child_set = make_child_set::<NUM_CHILDREN>(seed);
    for (dst, src) in keys.iter_mut().zip(child_set.into_iter()) {
        *dst = src;
    }
    keys
}

fn make_full_bitset<const WIDTH: usize, Bitset>() -> Bitset
where
    Bitset: BitsetTrait + Default,
{
    let mut bitset = Bitset::default();
    for idx in 0..WIDTH {
        bitset.set(idx);
    }
    bitset
}

fn present_key_at(keys: &[u8], index: usize) -> u8 {
    keys[index]
}

fn absent_key(keys: &[u8]) -> u8 {
    for candidate in 0..=u8::MAX {
        if !keys.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("there is always at least one absent key for these microbenches")
}

fn make_sorted_key_probes<const WIDTH: usize, const NUM_CHILDREN: usize>(
    num_chunks: usize,
) -> Vec<([u8; WIDTH], u8)> {
    let target_index = NUM_CHILDREN / 2;
    (0..num_chunks)
        .map(|chunk_idx| {
            let keys = make_sorted_keys::<WIDTH, NUM_CHILDREN>(chunk_idx as u64);
            let probe = present_key_at(&keys[..NUM_CHILDREN], target_index);
            (keys, probe)
        })
        .collect()
}

fn make_sorted_key_miss_probes<const WIDTH: usize, const NUM_CHILDREN: usize>(
    num_chunks: usize,
) -> Vec<([u8; WIDTH], u8)> {
    (0..num_chunks)
        .map(|chunk_idx| {
            let keys = make_sorted_keys::<WIDTH, NUM_CHILDREN>(chunk_idx as u64);
            let probe = absent_key(&keys[..NUM_CHILDREN]);
            (keys, probe)
        })
        .collect()
}

fn make_sorted_key_edge_probes<
    const WIDTH: usize,
    const NUM_CHILDREN: usize,
    const INDEX: usize,
>(
    num_chunks: usize,
) -> Vec<([u8; WIDTH], u8)> {
    debug_assert!(INDEX < NUM_CHILDREN);
    (0..num_chunks)
        .map(|chunk_idx| {
            let keys = make_sorted_keys::<WIDTH, NUM_CHILDREN>(chunk_idx as u64);
            let probe = keys[INDEX];
            (keys, probe)
        })
        .collect()
}

fn make_mixed_probe_sequence<const MISS_INTERVAL: usize>(
    present_keys: &[u8],
    misses: &[u8],
) -> Vec<u8> {
    let extra_misses = if MISS_INTERVAL == 0 {
        0
    } else {
        present_keys.len().div_ceil(MISS_INTERVAL)
    };
    let mut probes = Vec::with_capacity(present_keys.len() + extra_misses);
    for (idx, &key) in present_keys.iter().enumerate() {
        probes.push(key);
        if MISS_INTERVAL != 0 && (idx + 1) % MISS_INTERVAL == 0 {
            probes.push(misses[idx % misses.len()]);
        }
    }
    probes
}

fn make_sorted_key_mixed_probes<
    const WIDTH: usize,
    const NUM_CHILDREN: usize,
    const MISS_INTERVAL: usize,
>(
    num_chunks: usize,
) -> Vec<([u8; WIDTH], Vec<u8>)> {
    (0..num_chunks)
        .map(|chunk_idx| {
            let keys = make_sorted_keys::<WIDTH, NUM_CHILDREN>(chunk_idx as u64);
            let misses = make_miss_set(&keys[..NUM_CHILDREN], NUM_CHILDREN.max(1));
            let probes = make_mixed_probe_sequence::<MISS_INTERVAL>(&keys[..NUM_CHILDREN], &misses);
            (keys, probes)
        })
        .collect()
}

fn make_unsorted_key_probes<const WIDTH: usize, const NUM_CHILDREN: usize, Bitset>(
    num_chunks: usize,
) -> Vec<([u8; WIDTH], Bitset, u8)>
where
    Bitset: BitsetTrait + Default,
{
    let target_index = NUM_CHILDREN / 2;
    (0..num_chunks)
        .map(|chunk_idx| {
            let keys = make_unsorted_keys::<WIDTH, NUM_CHILDREN>(chunk_idx as u64);
            let probe = present_key_at(&keys[..NUM_CHILDREN], target_index);
            let bitset = make_full_bitset::<WIDTH, Bitset>();
            (keys, bitset, probe)
        })
        .collect()
}

fn make_sorted_insert_probes<const WIDTH: usize, const NUM_CHILDREN: usize>(
    num_chunks: usize,
) -> Vec<([u8; WIDTH], u8)> {
    (0..num_chunks)
        .map(|chunk_idx| {
            let keys = make_sorted_keys::<WIDTH, NUM_CHILDREN>(chunk_idx as u64);
            let probe = if NUM_CHILDREN == 0 {
                0
            } else {
                let left = keys[(NUM_CHILDREN - 1) / 2];
                let right = keys[NUM_CHILDREN / 2];
                left + ((right - left) / 2).max(1)
            };
            (keys, probe)
        })
        .collect()
}

fn make_sorted_node_seek_probes<const WIDTH: usize>(
    num_chunks: usize,
) -> Vec<(SortedKeyedMapping<u64, WIDTH>, u8)> {
    (0..num_chunks)
        .map(|chunk_idx| {
            let mut child_set = make_child_set::<WIDTH>(chunk_idx as u64);
            child_set.sort_unstable();
            let probe = child_set[WIDTH / 2];
            let mut mapping = SortedKeyedMapping::default();
            for key in child_set {
                mapping.add_child(key, key as u64);
            }
            (mapping, probe)
        })
        .collect()
}

fn make_sorted_node_seek_miss_probes<const WIDTH: usize>(
    num_chunks: usize,
) -> Vec<(SortedKeyedMapping<u64, WIDTH>, u8)> {
    (0..num_chunks)
        .map(|chunk_idx| {
            let mut child_set = make_child_set::<WIDTH>(chunk_idx as u64);
            child_set.sort_unstable();
            let probe = absent_key(&child_set);
            let mut mapping = SortedKeyedMapping::default();
            for key in child_set {
                mapping.add_child(key, key as u64);
            }
            (mapping, probe)
        })
        .collect()
}

fn make_sorted_node_seek_edge_probes<const WIDTH: usize, const INDEX: usize>(
    num_chunks: usize,
) -> Vec<(SortedKeyedMapping<u64, WIDTH>, u8)> {
    debug_assert!(INDEX < WIDTH);
    (0..num_chunks)
        .map(|chunk_idx| {
            let mut child_set = make_child_set::<WIDTH>(chunk_idx as u64);
            child_set.sort_unstable();
            let probe = child_set[INDEX];
            let mut mapping = SortedKeyedMapping::default();
            for key in child_set {
                mapping.add_child(key, key as u64);
            }
            (mapping, probe)
        })
        .collect()
}

fn make_sorted_node_seek_mixed_probes<const WIDTH: usize, const MISS_INTERVAL: usize>(
    num_chunks: usize,
) -> Vec<(SortedKeyedMapping<u64, WIDTH>, Vec<u8>)> {
    (0..num_chunks)
        .map(|chunk_idx| {
            let mut child_set = make_child_set::<WIDTH>(chunk_idx as u64);
            child_set.sort_unstable();
            let misses = make_miss_set(&child_set, WIDTH.max(1));
            let probes = make_mixed_probe_sequence::<MISS_INTERVAL>(&child_set, &misses);
            let mut mapping = SortedKeyedMapping::default();
            for key in child_set {
                mapping.add_child(key, key as u64);
            }
            (mapping, probes)
        })
        .collect()
}

fn make_node4_search_experiment_probes(num_chunks: usize) -> Vec<([u8; 16], Vec<u8>)> {
    (0..num_chunks)
        .map(|chunk_idx| {
            let mut child_set = make_child_set::<4>(chunk_idx as u64);
            child_set.sort_unstable();
            let misses = make_miss_set(&child_set, 4);
            let probes = {
                let mut probes = Vec::with_capacity(8);
                probes.extend_from_slice(&child_set);
                probes.extend_from_slice(&misses);
                probes
            };
            let mut padded = [255; 16];
            padded[..4].copy_from_slice(&child_set);
            (padded, probes)
        })
        .collect()
}

fn make_sorted_node_insert_probes<const WIDTH: usize>(
    num_chunks: usize,
) -> Vec<(SortedKeyedMapping<u64, WIDTH>, u8)> {
    let occupancy = WIDTH - 1;
    (0..num_chunks)
        .map(|chunk_idx| {
            let mut child_set = make_child_set::<WIDTH>(chunk_idx as u64);
            child_set.sort_unstable();
            let probe = absent_key(&child_set[..occupancy]);
            let mut mapping = SortedKeyedMapping::default();
            for &key in &child_set[..occupancy] {
                mapping.add_child(key, key as u64);
            }
            (mapping, probe)
        })
        .collect()
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
    register_del_child_density_bench::<256, 48, DirectMapping<u64>>(runner, "direct_occ48");
    register_del_child_density_bench::<256, 64, DirectMapping<u64>>(runner, "direct_occ64");
    register_del_child_density_bench::<256, 96, DirectMapping<u64>>(runner, "direct_occ96");
    register_del_child_density_bench::<256, 128, DirectMapping<u64>>(runner, "direct_occ128");
    register_del_child_density_bench::<256, 192, DirectMapping<u64>>(runner, "direct_occ192");
    register_del_child_density_bench::<256, 208, DirectMapping<u64>>(runner, "direct_occ208");
}

fn register_sorted_key_search_benches(runner: &BenchmarkRunner) {
    runner.group::<SortedKeyProbeContext<4, 4>>("sorted_key_search_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width4", bench_sorted_key_seek::<4, 4>);
    });
    runner.group::<SortedKeyProbeContext<16, 16>>("sorted_key_search_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width16", bench_sorted_key_seek::<16, 16>);
    });
    runner.group::<SortedKeyProbeContext<32, 32>>("sorted_key_search_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width32", bench_sorted_key_seek::<32, 32>);
    });
}

fn register_unsorted_key_search_benches(runner: &BenchmarkRunner) {
    runner.group::<UnsortedKeyProbeContext<16, 16, Bitset16<1>>>("unsorted_key_search_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width16", bench_unsorted_key_seek::<16, 16, Bitset16<1>>);
    });
    runner.group::<UnsortedKeyProbeContext<32, 32, Bitset32<1>>>("unsorted_key_search_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width32", bench_unsorted_key_seek::<32, 32, Bitset32<1>>);
    });
}

fn register_sorted_key_search_detail_benches(runner: &BenchmarkRunner) {
    runner.group::<SortedKeyEdgeProbeContext<16, 16, 0>>("sorted_key_search_front", |g| {
        g.throughput(Throughput::ops())
            .bench("width16", bench_sorted_key_seek_edge::<16, 16, 0>);
    });
    runner.group::<SortedKeyEdgeProbeContext<16, 16, 15>>("sorted_key_search_back", |g| {
        g.throughput(Throughput::ops())
            .bench("width16", bench_sorted_key_seek_edge::<16, 16, 15>);
    });
    runner.group::<SortedKeyMissProbeContext<16, 16>>("sorted_key_search_miss", |g| {
        g.throughput(Throughput::ops())
            .bench("width16", bench_sorted_key_seek_miss::<16, 16>);
    });
    runner.group::<SortedKeyMixedProbeContext<16, 16, 0>>("sorted_key_search_mixed_hits", |g| {
        g.throughput(Throughput::per_operation(16, "probes"))
            .bench("width16", bench_sorted_key_seek_mixed::<16, 16, 0>);
    });
    runner.group::<SortedKeyMixedProbeContext<16, 16, 10>>(
        "sorted_key_search_mixed_hits_90_miss_10",
        |g| {
            g.throughput(Throughput::per_operation(17, "probes"))
                .bench("width16", bench_sorted_key_seek_mixed::<16, 16, 10>);
        },
    );
    runner.group::<SortedKeyMixedProbeContext<16, 16, 2>>(
        "sorted_key_search_mixed_hits_50_miss_50",
        |g| {
            g.throughput(Throughput::per_operation(24, "probes"))
                .bench("width16", bench_sorted_key_seek_mixed::<16, 16, 2>);
        },
    );
}

fn register_sorted_key_insert_benches(runner: &BenchmarkRunner) {
    runner.group::<SortedKeyInsertContext<4, 3>>("sorted_key_insert_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width4_num3", bench_sorted_key_insert_position::<4, 3>);
    });
    runner.group::<SortedKeyInsertContext<16, 15>>("sorted_key_insert_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width16_num15", bench_sorted_key_insert_position::<16, 15>);
    });
    runner.group::<SortedKeyInsertContext<32, 31>>("sorted_key_insert_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("width32_num31", bench_sorted_key_insert_position::<32, 31>);
    });
}

fn register_sorted_node_seek_benches(runner: &BenchmarkRunner) {
    runner.group::<SortedNodeSeekContext<4>>("sorted_node_seek_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("node4", bench_sorted_node_seek::<4>);
    });
    runner.group::<SortedNodeSeekContext<16>>("sorted_node_seek_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("node16", bench_sorted_node_seek::<16>);
    });
}

fn register_sorted_node_seek_detail_benches(runner: &BenchmarkRunner) {
    runner.group::<SortedNodeSeekEdgeContext<16, 0>>("sorted_node_seek_front", |g| {
        g.throughput(Throughput::ops())
            .bench("node16", bench_sorted_node_seek_edge::<16, 0>);
    });
    runner.group::<SortedNodeSeekEdgeContext<16, 15>>("sorted_node_seek_back", |g| {
        g.throughput(Throughput::ops())
            .bench("node16", bench_sorted_node_seek_edge::<16, 15>);
    });
    runner.group::<SortedNodeSeekMissContext<16>>("sorted_node_seek_miss", |g| {
        g.throughput(Throughput::ops())
            .bench("node16", bench_sorted_node_seek_miss::<16>);
    });
    runner.group::<SortedNodeSeekMixedContext<16, 0>>("sorted_node_seek_mixed_hits", |g| {
        g.throughput(Throughput::per_operation(16, "probes"))
            .bench("node16", bench_sorted_node_seek_mixed::<16, 0>);
    });
    runner.group::<SortedNodeSeekMixedContext<16, 10>>(
        "sorted_node_seek_mixed_hits_90_miss_10",
        |g| {
            g.throughput(Throughput::per_operation(17, "probes"))
                .bench("node16", bench_sorted_node_seek_mixed::<16, 10>);
        },
    );
    runner.group::<SortedNodeSeekMixedContext<16, 2>>(
        "sorted_node_seek_mixed_hits_50_miss_50",
        |g| {
            g.throughput(Throughput::per_operation(24, "probes"))
                .bench("node16", bench_sorted_node_seek_mixed::<16, 2>);
        },
    );
}

fn register_sorted_node_insert_benches(runner: &BenchmarkRunner) {
    runner.group::<SortedNodeInsertContext<4>>("sorted_node_insert_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("node4", bench_sorted_node_insert::<4>);
    });
    runner.group::<SortedNodeInsertContext<16>>("sorted_node_insert_mid", |g| {
        g.throughput(Throughput::ops())
            .bench("node16", bench_sorted_node_insert::<16>);
    });
}

fn register_node4_search_experiment_benches(runner: &BenchmarkRunner) {
    runner.group::<Node4SearchExperimentContext>("node4_search_experiments", |g| {
        g.throughput(Throughput::per_operation(8, "probes"))
            .bench("linear", bench_node4_search_linear);
        g.throughput(Throughput::per_operation(8, "probes"))
            .bench("unrolled", bench_node4_search_unrolled);
        #[cfg(feature = "simd_keys")]
        g.throughput(Throughput::per_operation(8, "probes"))
            .bench("simd_padded16", bench_node4_search_simd_padded);
    });
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
    register_sorted_key_search_benches(runner);
    register_unsorted_key_search_benches(runner);
    register_sorted_key_search_detail_benches(runner);
    register_sorted_key_insert_benches(runner);
    register_sorted_node_seek_benches(runner);
    register_sorted_node_seek_detail_benches(runner);
    register_sorted_node_insert_benches(runner);
    register_node4_search_experiment_benches(runner);
});
