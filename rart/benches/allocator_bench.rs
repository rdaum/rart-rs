//! Compares `AdaptiveRadixTree` node storage in the global allocator against a bump arena.
//!
//! Three allocator variants are measured:
//!
//! - `global`: the default `Global` allocator.
//! - `bump_ref`: a bump arena reached through `&Bump`. Every `Box<T, &Bump>` stores the
//!   reference, so nodes are wider than with `Global`.
//! - `bump_tls`: the same arena reached through a zero-sized, thread-local handle, so nodes
//!   keep the `Global` layout. The gap between `bump_ref` and `bump_tls` is the cost of the
//!   per-box allocator handle.
//!
//! The arenas are rewound between iterations rather than re-created, so chunk allocation and
//! first-touch page faults are not part of the measurement.

use std::alloc::{AllocError, Allocator, Global, Layout};
use std::cell::{Cell, RefCell};
use std::hint::black_box;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rart::{AdaptiveRadixTree, ArrayKey};

type Key = ArrayKey<16>;
type Tree<A> = AdaptiveRadixTree<Key, u64, A>;

const BUILD_SIZES: [usize; 2] = [1 << 12, 1 << 17];
const GET_SIZES: [usize; 2] = [1 << 17, 1 << 20];
const CHUNK_SIZE: usize = 1 << 20;
const CHUNK_ALIGN: usize = 64;

struct Chunk {
    ptr: NonNull<u8>,
    layout: Layout,
}

/// A minimal chunked bump arena. Deallocation is a no-op; memory is reclaimed by `reset` or
/// when the arena is dropped.
struct Bump {
    chunks: RefCell<Vec<Chunk>>,
    next_chunk: Cell<usize>,
    ptr: Cell<*mut u8>,
    remaining: Cell<usize>,
}

impl Bump {
    const fn new() -> Self {
        Self {
            chunks: RefCell::new(Vec::new()),
            next_chunk: Cell::new(0),
            ptr: Cell::new(std::ptr::null_mut()),
            remaining: Cell::new(0),
        }
    }

    /// Rewind the arena so its chunks are reused.
    ///
    /// # Safety
    /// No block previously handed out by this arena may be used afterwards.
    unsafe fn reset(&self) {
        self.next_chunk.set(0);
        self.ptr.set(std::ptr::null_mut());
        self.remaining.set(0);
    }

    #[inline]
    fn alloc(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Ok(NonNull::slice_from_raw_parts(layout.dangling_ptr(), 0));
        }

        let ptr = self.ptr.get();
        let pad = ptr.align_offset(layout.align());
        let needed = pad.checked_add(layout.size()).ok_or(AllocError)?;
        if needed > self.remaining.get() {
            return self.alloc_slow(layout);
        }

        // SAFETY: `pad + size <= remaining`, so both offsets stay inside the current chunk,
        // and a non-zero `remaining` means `ptr` is non-null.
        unsafe {
            let start = ptr.add(pad);
            self.ptr.set(start.add(layout.size()));
            self.remaining.set(self.remaining.get() - needed);
            Ok(NonNull::slice_from_raw_parts(
                NonNull::new_unchecked(start),
                layout.size(),
            ))
        }
    }

    #[cold]
    fn alloc_slow(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // Chunks are `CHUNK_ALIGN`-aligned, so a request with larger alignment may need padding.
        let worst_case = layout
            .size()
            .checked_add(layout.align())
            .ok_or(AllocError)?;

        loop {
            let mut chunks = self.chunks.borrow_mut();
            let index = self.next_chunk.get();
            if index == chunks.len() {
                let chunk_layout = Layout::from_size_align(worst_case.max(CHUNK_SIZE), CHUNK_ALIGN)
                    .map_err(|_| AllocError)?;
                let ptr = Global.allocate(chunk_layout)?.cast::<u8>();
                chunks.push(Chunk {
                    ptr,
                    layout: chunk_layout,
                });
            }

            let chunk = &chunks[index];
            self.next_chunk.set(index + 1);
            // A reused chunk that is too small for this request is skipped.
            if chunk.layout.size() >= worst_case {
                self.ptr.set(chunk.ptr.as_ptr());
                self.remaining.set(chunk.layout.size());
                drop(chunks);
                return self.alloc(layout);
            }
        }
    }
}

impl Drop for Bump {
    fn drop(&mut self) {
        for chunk in self.chunks.get_mut().drain(..) {
            // SAFETY: each chunk was allocated from `Global` with exactly this layout.
            unsafe { Global.deallocate(chunk.ptr, chunk.layout) }
        }
    }
}

// SAFETY: blocks stay valid until `reset` (whose contract forbids later use) or drop, and the
// arena cannot be moved or dropped while `&Bump` handles borrowing it are alive.
unsafe impl Allocator for Bump {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.alloc(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

thread_local! {
    static TLS_BUMP: Bump = const { Bump::new() };
}

/// Zero-sized handle to this thread's arena. The raw-pointer marker keeps it `!Send`, so a
/// tree cannot migrate to a thread with a different arena.
#[derive(Clone, Copy)]
struct TlsBump(PhantomData<*const ()>);

impl TlsBump {
    fn new() -> Self {
        Self(PhantomData)
    }

    /// # Safety
    /// Same contract as [`Bump::reset`], for this thread's arena.
    unsafe fn reset() {
        // SAFETY: forwarded to the caller.
        TLS_BUMP.with(|bump| unsafe { bump.reset() });
    }
}

// SAFETY: every handle on a thread refers to the same arena, which lives until thread exit.
unsafe impl Allocator for TlsBump {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        TLS_BUMP.with(|bump| bump.alloc(layout))
    }

    #[inline]
    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {}
}

fn full_bench_profile() -> bool {
    std::env::var("RART_BENCH_FULL").as_deref() == Ok("1")
}

fn criterion_config() -> Criterion {
    if full_bench_profile() {
        Criterion::default()
    } else {
        Criterion::default()
            .sample_size(20)
            .warm_up_time(Duration::from_secs(1))
            .measurement_time(Duration::from_secs(2))
    }
}

/// Sequential keys: long shared prefixes and wide, densely populated nodes.
fn dense_items(size: usize) -> Vec<(Key, u64)> {
    (0..size as u64).map(|i| (i.into(), i)).collect()
}

/// Uniformly random keys: wide nodes near the root and many small nodes below.
fn sparse_items(size: usize) -> Vec<(Key, u64)> {
    let mut rng = StdRng::seed_from_u64(0xa110c);
    let mut keys: Vec<u64> = (0..size).map(|_| rng.random()).collect();
    keys.sort_unstable();
    keys.dedup();
    keys.into_iter()
        .enumerate()
        .map(|(i, key)| (key.into(), i as u64))
        .collect()
}

fn shuffled(mut items: Vec<(Key, u64)>) -> Vec<(Key, u64)> {
    let mut rng = StdRng::seed_from_u64(0x5eed);
    items.shuffle(&mut rng);
    items
}

fn datasets(size: usize) -> [(&'static str, Vec<(Key, u64)>); 2] {
    [("dense", dense_items(size)), ("sparse", sparse_items(size))]
}

fn build_incremental<A: Allocator + Clone>(items: &[(Key, u64)], alloc: A) -> Tree<A> {
    let mut tree = AdaptiveRadixTree::new_in(alloc);
    for &(key, value) in items {
        tree.insert_k(&key, value);
    }
    tree
}

fn build_bulk<A: Allocator + Clone>(items: &[(Key, u64)], alloc: A) -> Tree<A> {
    AdaptiveRadixTree::bulk_load_sorted_unique_in(items.iter().copied(), alloc)
}

type BuildFn<A> = fn(&[(Key, u64)], A) -> Tree<A>;
type BuildRefFn = for<'b> fn(&[(Key, u64)], &'b Bump) -> Tree<&'b Bump>;

/// What happens to the tree once it is built.
#[derive(Clone, Copy)]
enum Teardown {
    /// Exclude teardown from the measurement. Arena trees are forgotten, which is the arena
    /// usage model (values here need no drop); `Global` trees are dropped by Criterion
    /// outside the timed region.
    Excluded,
    /// Drop the tree inside the timed region.
    Timed,
}

fn bench_build_variants(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    dataset: &str,
    items: &[(Key, u64)],
    teardown: Teardown,
    build_global: BuildFn<Global>,
    build_ref: BuildRefFn,
    build_tls: BuildFn<TlsBump>,
) {
    let size = items.len();
    group.throughput(Throughput::Elements(size as u64));

    group.bench_function(
        BenchmarkId::new(format!("global/{dataset}"), size),
        |b| match teardown {
            Teardown::Excluded => b.iter_batched(
                || (),
                |_| build_global(items, Global),
                BatchSize::PerIteration,
            ),
            Teardown::Timed => b.iter(|| build_global(items, Global).len()),
        },
    );

    let bump = Bump::new();
    group.bench_function(BenchmarkId::new(format!("bump_ref/{dataset}"), size), |b| {
        b.iter_batched(
            // SAFETY: the previous iteration's tree was forgotten or dropped.
            || unsafe { bump.reset() },
            |_| {
                let tree = build_ref(items, &bump);
                let len = tree.len();
                match teardown {
                    Teardown::Excluded => std::mem::forget(tree),
                    Teardown::Timed => drop(tree),
                }
                len
            },
            BatchSize::PerIteration,
        )
    });

    group.bench_function(BenchmarkId::new(format!("bump_tls/{dataset}"), size), |b| {
        b.iter_batched(
            // SAFETY: the previous iteration's tree was forgotten or dropped.
            || unsafe { TlsBump::reset() },
            |_| {
                let tree = build_tls(items, TlsBump::new());
                let len = tree.len();
                match teardown {
                    Teardown::Excluded => std::mem::forget(tree),
                    Teardown::Timed => drop(tree),
                }
                len
            },
            BatchSize::PerIteration,
        )
    });
}

pub fn build_incremental_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_build_incremental");
    for size in BUILD_SIZES {
        for (dataset, items) in datasets(size) {
            let items = shuffled(items);
            bench_build_variants(
                &mut group,
                dataset,
                &items,
                Teardown::Excluded,
                build_incremental,
                |items, bump| build_incremental(items, bump),
                build_incremental,
            );
        }
    }
    group.finish();
}

pub fn build_bulk_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_build_bulk");
    for size in BUILD_SIZES {
        for (dataset, items) in datasets(size) {
            bench_build_variants(
                &mut group,
                dataset,
                &items,
                Teardown::Excluded,
                build_bulk,
                |items, bump| build_bulk(items, bump),
                build_bulk,
            );
        }
    }
    group.finish();
}

pub fn build_and_drop_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_build_and_drop_incremental");
    for size in BUILD_SIZES {
        for (dataset, items) in datasets(size) {
            let items = shuffled(items);
            bench_build_variants(
                &mut group,
                dataset,
                &items,
                Teardown::Timed,
                build_incremental,
                |items, bump| build_incremental(items, bump),
                build_incremental,
            );
        }
    }
    group.finish();
}

fn sum_gets<A: Allocator + Clone>(tree: &Tree<A>, probes: &[Key]) -> u64 {
    let mut sum = 0u64;
    for key in probes {
        if let Some(value) = tree.get_k(key) {
            sum = sum.wrapping_add(*value);
        }
    }
    sum
}

pub fn get_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_get");
    for size in GET_SIZES {
        for (dataset, items) in datasets(size) {
            // Insert in random order so `Global` placement reflects an organically grown tree.
            let items = shuffled(items);
            let mut probes: Vec<Key> = items.iter().map(|(key, _)| *key).collect();
            probes.shuffle(&mut StdRng::seed_from_u64(0x9e7));
            group.throughput(Throughput::Elements(probes.len() as u64));

            let tree = build_incremental(&items, Global);
            group.bench_function(BenchmarkId::new(format!("global/{dataset}"), size), |b| {
                b.iter(|| sum_gets(black_box(&tree), &probes))
            });
            drop(tree);

            let bump = Bump::new();
            let tree = build_incremental(&items, &bump);
            group.bench_function(BenchmarkId::new(format!("bump_ref/{dataset}"), size), |b| {
                b.iter(|| sum_gets(black_box(&tree), &probes))
            });
            drop(tree);

            // SAFETY: every tree previously built in the thread-local arena is gone.
            unsafe { TlsBump::reset() };
            let tree = build_incremental(&items, TlsBump::new());
            group.bench_function(BenchmarkId::new(format!("bump_tls/{dataset}"), size), |b| {
                b.iter(|| sum_gets(black_box(&tree), &probes))
            });
            drop(tree);
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = build_incremental_bench, build_bulk_bench, build_and_drop_bench, get_bench
}
criterion_main!(benches);
