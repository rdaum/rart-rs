//! Internal microbenchmarks for VersionedAdaptiveRadixTree.
//!
//! These are intentionally separate from the higher-level Criterion comparison
//! benches. They focus on snapshot and copy-on-write behavior, plus concurrent
//! access patterns over shared structure.

use std::sync::Arc;
use std::time::Duration;

use micromeasure::{
    BenchContext, BenchmarkMainOptions, BenchmarkRuntimeOptions, ConcurrentBenchContext,
    ConcurrentBenchControl, ConcurrentWorker, ConcurrentWorkerResult, Throughput, benchmark_main,
    black_box,
};
use rart::{ArrayKey, VersionedAdaptiveRadixTree};

type Tree = VersionedAdaptiveRadixTree<ArrayKey<16>, usize>;

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

fn options() -> BenchmarkMainOptions {
    BenchmarkMainOptions {
        suite: Some("rart-versioned-tree".to_string()),
        runtime: runtime_options(),
        ..BenchmarkMainOptions::default()
    }
}

fn versioned_tree_base_size() -> usize {
    if full_bench_profile() {
        1 << 14
    } else {
        1 << 10
    }
}

fn single_thread_chunk_size() -> usize {
    if full_bench_profile() { 1024 } else { 128 }
}

fn concurrent_sample_duration() -> Duration {
    if full_bench_profile() {
        Duration::from_millis(100)
    } else {
        Duration::from_millis(40)
    }
}

fn build_base_tree(size: usize) -> Tree {
    let mut tree = Tree::new();
    for i in 0..size {
        tree.insert(i, i);
    }
    tree
}

struct SnapshotOnlyContext {
    tree: Tree,
}

impl BenchContext for SnapshotOnlyContext {
    fn prepare(_num_chunks: usize) -> Self {
        Self {
            tree: build_base_tree(versioned_tree_base_size()),
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(single_thread_chunk_size())
    }
}

fn bench_snapshot_only(ctx: &mut SnapshotOnlyContext, chunk_size: usize, _chunk_num: usize) {
    for _ in 0..chunk_size {
        let snapshot = ctx.tree.snapshot();
        black_box(snapshot);
    }
}

struct SnapshotInsertContext {
    tree: Tree,
    next_key: usize,
}

impl BenchContext for SnapshotInsertContext {
    fn prepare(_num_chunks: usize) -> Self {
        let base_size = versioned_tree_base_size();
        Self {
            tree: build_base_tree(base_size),
            next_key: base_size * 8,
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(single_thread_chunk_size())
    }
}

fn bench_snapshot_then_insert(
    ctx: &mut SnapshotInsertContext,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for _ in 0..chunk_size {
        let mut snapshot = ctx.tree.snapshot();
        let key = ctx.next_key;
        ctx.next_key += 1;
        black_box(snapshot.insert(key, key));
        black_box(snapshot);
    }
}

struct SnapshotRemoveContext {
    tree: Tree,
    remove_keys: Vec<usize>,
    cursor: usize,
}

impl BenchContext for SnapshotRemoveContext {
    fn prepare(_num_chunks: usize) -> Self {
        let base_size = versioned_tree_base_size();
        Self {
            tree: build_base_tree(base_size),
            remove_keys: (0..base_size).collect(),
            cursor: 0,
        }
    }

    fn chunk_size() -> Option<usize> {
        Some(single_thread_chunk_size())
    }
}

fn bench_snapshot_then_remove(
    ctx: &mut SnapshotRemoveContext,
    chunk_size: usize,
    _chunk_num: usize,
) {
    for _ in 0..chunk_size {
        let mut snapshot = ctx.tree.snapshot();
        let key = ctx.remove_keys[ctx.cursor % ctx.remove_keys.len()];
        ctx.cursor += 1;
        black_box(snapshot.remove(key));
        black_box(snapshot);
    }
}

struct SharedVersionedTreeContext {
    base: Arc<Tree>,
    lookup_span: usize,
    write_start: usize,
}

impl ConcurrentBenchContext for SharedVersionedTreeContext {
    fn prepare(_num_threads: usize) -> Self {
        let base_size = versioned_tree_base_size();
        Self {
            base: Arc::new(build_base_tree(base_size)),
            lookup_span: base_size,
            write_start: base_size * 8,
        }
    }
}

fn shared_snapshot_reader(
    ctx: &SharedVersionedTreeContext,
    control: &ConcurrentBenchControl,
) -> ConcurrentWorkerResult {
    let mut operations = 0_u64;
    let mut cursor = control.thread_index() * 131;

    while !control.should_stop() {
        let key = cursor % ctx.lookup_span;
        black_box(ctx.base.get(key));
        cursor = cursor.wrapping_add(1);
        operations = operations.wrapping_add(1);
    }

    ConcurrentWorkerResult::operations(operations)
}

fn snapshot_insert_writer(
    ctx: &SharedVersionedTreeContext,
    control: &ConcurrentBenchControl,
) -> ConcurrentWorkerResult {
    let mut operations = 0_u64;
    let mut local_key = ctx.write_start
        + (control.thread_index() * 1_000_000)
        + (control.role_thread_index() * 10_000);

    while !control.should_stop() {
        let mut snapshot = ctx.base.snapshot();
        black_box(snapshot.insert(local_key, local_key));
        black_box(snapshot.get(local_key));
        black_box(snapshot);
        local_key = local_key.wrapping_add(1);
        operations = operations.wrapping_add(1);
    }

    ConcurrentWorkerResult::operations(operations)
}

fn snapshot_remove_writer(
    ctx: &SharedVersionedTreeContext,
    control: &ConcurrentBenchControl,
) -> ConcurrentWorkerResult {
    let mut operations = 0_u64;
    let mut local_key = control.thread_index() * 257;

    while !control.should_stop() {
        let mut snapshot = ctx.base.snapshot();
        let key = local_key % ctx.lookup_span;
        black_box(snapshot.remove(key));
        black_box(snapshot);
        local_key = local_key.wrapping_add(1);
        operations = operations.wrapping_add(1);
    }

    ConcurrentWorkerResult::operations(operations)
}

benchmark_main!(options(), |runner| {
    let read_heavy_workers = [
        ConcurrentWorker {
            name: "shared_reader",
            threads: 3,
            run: shared_snapshot_reader,
        },
        ConcurrentWorker {
            name: "snapshot_insert_writer",
            threads: 1,
            run: snapshot_insert_writer,
        },
    ];

    let write_heavy_workers = [
        ConcurrentWorker {
            name: "snapshot_insert_writer",
            threads: 2,
            run: snapshot_insert_writer,
        },
        ConcurrentWorker {
            name: "snapshot_remove_writer",
            threads: 2,
            run: snapshot_remove_writer,
        },
    ];

    runner.group::<SnapshotOnlyContext>("versioned_tree_snapshot", |g| {
        g.throughput(Throughput::ops())
            .bench("snapshot_only", bench_snapshot_only);
    });

    runner.group::<SnapshotInsertContext>("versioned_tree_cow", |g| {
        g.throughput(Throughput::ops())
            .bench("snapshot_then_insert", bench_snapshot_then_insert);
    });

    runner.group::<SnapshotRemoveContext>("versioned_tree_cow", |g| {
        g.throughput(Throughput::ops())
            .bench("snapshot_then_remove", bench_snapshot_then_remove);
    });

    runner.concurrent_group::<SharedVersionedTreeContext>("versioned_tree_concurrent", |g| {
        g.sample_duration(concurrent_sample_duration())
            .throughput(Throughput::ops())
            .bench(
                "shared_readers_vs_snapshot_insert_writer",
                &read_heavy_workers,
            );
    });

    runner.concurrent_group::<SharedVersionedTreeContext>("versioned_tree_concurrent", |g| {
        g.sample_duration(concurrent_sample_duration())
            .throughput(Throughput::ops())
            .bench(
                "snapshot_insert_vs_snapshot_remove_writers",
                &write_heavy_workers,
            );
    });
});
