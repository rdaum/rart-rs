use iai_callgrind::{black_box, main};
use paste::paste;
use rand::{thread_rng, Rng};

use rart::keys::array_key::ArrayKey;
use rart::tree::AdaptiveRadixTree;
use rart::TreeTrait;

const TREE_SIZES: [u64; 4] = [1 << 14, 1 << 16, 1 << 18, 1 << 20];

#[export_name = "tree_setup"]
fn setup_tree(n: usize) -> AdaptiveRadixTree<ArrayKey<16>, u64> {
    let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();

    for i in 0..TREE_SIZES[n] {
        tree.insert(i, i);
    }
    tree
}

#[inline(never)]
fn benchmark_seq_insert(n: usize) {
    let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();

    for i in 0..TREE_SIZES[n] {
        black_box(tree.insert(i, i));
    }
}

#[inline(never)]
fn benchmark_rnd_insert(n: usize) {
    let mut tree = AdaptiveRadixTree::<ArrayKey<16>, _>::new();

    let mut rng = thread_rng();
    for _ in 0..TREE_SIZES[n] {
        black_box(tree.insert(rng.gen_range(0..TREE_SIZES[n]), 0));
    }
}

#[inline(never)]
fn benchmark_seq_get(n: usize) {
    let tree = black_box(setup_tree(n));

    for i in 0..TREE_SIZES[n] {
        black_box(tree.get(i)).unwrap();
    }
}

#[inline(never)]
fn benchmark_rnd_get(tree_size: usize) {
    let tree = black_box(setup_tree(tree_size));

    let mut rng = thread_rng();
    for _ in 0..1_000_000 {
        black_box(tree.get(rng.gen_range(0..TREE_SIZES[tree_size])).unwrap());
    }
}

// Simple macro to produce 4 versions of each benchmark function
macro_rules! mk_benchmark {
    ($name:ident  { $($n:literal),* }) => {
        paste! {
            $(
            #[inline(never)]

               fn [<$name _ $n>]() {
                $name($n)
            } )*
        }
    };
}

mk_benchmark!(benchmark_seq_insert { 0, 1, 2, 3 });
mk_benchmark!(benchmark_rnd_insert { 0, 1, 2, 3 });
mk_benchmark!(benchmark_seq_get { 0, 1, 2, 3 });
mk_benchmark!(benchmark_rnd_get { 0, 1, 2, 3 });

main!(
    callgrind_args = "toggle-collect=tree_setup";
    functions = benchmark_seq_get_0, benchmark_seq_get_1, benchmark_seq_get_2, benchmark_seq_get_3,
                benchmark_rnd_get_0, benchmark_rnd_get_1, benchmark_rnd_get_2, benchmark_rnd_get_3,
                benchmark_seq_insert_0, benchmark_seq_insert_1, benchmark_seq_insert_2, benchmark_seq_insert_3,
                benchmark_rnd_insert_0, benchmark_rnd_insert_1, benchmark_rnd_insert_2, benchmark_rnd_insert_3
);
