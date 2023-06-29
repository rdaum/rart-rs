use iai_callgrind::main;
use std::collections::HashSet;

use rart::mapping::direct_mapping::DirectMapping;
use rart::mapping::indexed_mapping::IndexedMapping;
use rart::mapping::keyed_mapping::KeyedMapping;
use rart::mapping::sorted_keyed_mapping::SortedKeyedMapping;
use rart::mapping::NodeMapping;
use rart::utils::bitset::{Bitset16, Bitset64, Bitset8};

type KeyMapping16_16x1 = KeyedMapping<u64, 16, Bitset16<1>>;
type KeyMapping4 = KeyedMapping<u64, 4, Bitset8<1>>;

#[inline(never)]
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

#[export_name = "setup_seek"]
#[inline(never)]
fn setup_seek_bench<const WIDTH: usize, MappingType>(iters: u64) -> Vec<(MappingType, Vec<u8>)>
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    let _mapping_set = make_mapping_sets::<WIDTH, MappingType>(iters);
    let mut mapping_set = make_mapping_sets::<WIDTH, MappingType>(iters);
    for (ref mut mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }
    mapping_set
}

#[inline(never)]
fn benched_seek_child<const WIDTH: usize, MappingType>(iters: u64)
where
    MappingType: NodeMapping<u64, WIDTH> + Default,
{
    let mut mapping_set = make_mapping_sets::<WIDTH, MappingType>(iters);
    // Fill all the sets.
    for (ref mut mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.add_child(*child, 0u64);
        }
    }

    for (ref mut mapping, child_set) in &mut mapping_set {
        for child in child_set {
            mapping.seek_child(*child);
        }
    }
}

#[inline(never)]
fn bench_node_256_seek() {
    benched_seek_child::<256, DirectMapping<u64>>(1 << 18);
}

#[inline(never)]
fn bench_node_48_seek() {
    benched_seek_child::<48, IndexedMapping<u64, 48, Bitset64<1>>>(1 << 18);
}

#[inline(never)]
fn bench_node_16_seek() {
    benched_seek_child::<16, KeyMapping16_16x1>(1 << 18);
}

#[inline(never)]
fn bench_node_16_sorted_seek() {
    benched_seek_child::<16, SortedKeyedMapping<u64, 16>>(1 << 18);
}

#[inline(never)]
fn bench_node_4_seek() {
    benched_seek_child::<4, KeyMapping4>(1 << 18);
}

main!(
    callgrind_args = "toggle-collect=setup_sets,setup_seek";
    functions = bench_node_256_seek, bench_node_48_seek, bench_node_16_seek, bench_node_16_sorted_seek, bench_node_4_seek
);
