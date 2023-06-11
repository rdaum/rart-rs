use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use rand::{thread_rng, Rng};

use rart::utils::fillvector::{FVIndex, FillVector};

// A workload where a pile of items are inserted, then a pile of random items are deleted, then a
// pile of items are inserted, etc. Meant to simulate something like how a real world application
// might use a container like this.
pub fn fv_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("fb_insert");
    group.throughput(Throughput::Elements(1));
    group.bench_function("fv_insert", |b| {
        let mut fv = FillVector::<usize>::new();

        b.iter(|| {
            let mut rng = thread_rng();
            let _idx = fv.add(|_i| 123);
            // 1 in 5 chance of deleting an item from somewhere in the middle of the vector.
            // No guarantee the item we're trying to free is actually there, but we'll take
            // our chances.
            if rng.gen_range(0..5) == 0 {
                let idx = rng.gen_range(0..fv.size());
                fv.free(FVIndex(idx as u32));
            }
        });
    });
    group.finish();
}

criterion_group!(fv_benches, fv_insert);
criterion_main!(fv_benches);
