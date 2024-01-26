//! Benchmarks for the `PointerMap`.
//!
//! The [`intmap`](https://crates.io/crates/intmap) crate was evaluated
//! and showed significant (20-400%) regressions except for the 100% collisions case
//! which is statistically impossible.
//! This isn't entirely surprising, as the `nohash_hasher`
//! does no work to hash the value as it is already a hash.

use criterion::{black_box, criterion_group, criterion_main, Bencher, BenchmarkId, Criterion, Throughput};
use mem_arch_prototype::indexes::{PageIndex, PageOffset, RowHash, RowPointer, SquashedOffset};
use mem_arch_prototype::pointer_map::PointerMap;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use std::time::{Duration, Instant};

type RngMut<'r> = &'r mut ThreadRng;

fn gen_ptr(rng: RngMut<'_>) -> RowPointer {
    let page = PageIndex(rng.gen::<u64>());
    let page_offset = PageOffset(rng.gen::<u16>());
    RowPointer::new(false, page, page_offset, SquashedOffset::TX_STATE)
}

fn gen_row_hash(rng: RngMut<'_>, max_range: u64) -> RowHash {
    RowHash(rng.gen_range(0..max_range))
}

fn gen_hash_and_ptrs(rng: RngMut<'_>, max: u64, count: usize) -> impl '_ + Iterator<Item = (RowHash, RowPointer)> {
    (0..count).map(move |_| (gen_row_hash(rng, max), gen_ptr(rng)))
}

fn max_range(n: usize, collision_ratio: f64) -> u64 {
    let n = n as f64;
    let max_range = -1.0 / (-1.0 + f64::powf(1.0 - collision_ratio, 1.0 / (-1.0 + n)));
    if max_range.is_finite() {
        max_range as u64
    } else {
        u64::MAX
    }
}

fn time(body: impl FnOnce()) -> Duration {
    let start = Instant::now();
    body();
    start.elapsed()
}

fn bench_insert(c: &mut Criterion) {
    const NUM_INSERTS_PER_MAP: usize = 1000;
    let bench_insert_inner = |bench: &mut Bencher<'_, _>, collision_ratio: &f64| {
        let preload_amt = 10_000;
        let max_range = max_range(preload_amt + NUM_INSERTS_PER_MAP, *collision_ratio);

        let mut rng = thread_rng();
        let map = gen_hash_and_ptrs(&mut rng, max_range, preload_amt).collect::<PointerMap>();
        let to_insert = gen_hash_and_ptrs(&mut rng, max_range, NUM_INSERTS_PER_MAP).collect::<Vec<_>>();

        bench.iter_custom(|iters| {
            let mut total_duration = Duration::from_secs(0);
            let mut num_iters = 0;
            while num_iters < iters {
                let mut map = map.clone();
                for (hash, ptr) in to_insert.iter().copied() {
                    // Compute duration of map insertion.
                    total_duration += time(|| {
                        black_box(map.insert(black_box(hash), black_box(ptr)));
                    });

                    num_iters += 1;
                    if num_iters >= iters {
                        break;
                    }
                }
                drop(map);
            }
            total_duration
        });
    };
    let mut bench_group = c.benchmark_group("insert");
    bench_group.throughput(Throughput::Elements(1));
    let mut bench = |percent, prob: f64| {
        bench_group.bench_with_input(
            BenchmarkId::new("load/10_000/insert/1000/collisions", percent),
            &prob,
            bench_insert_inner,
        );
    };
    bench("0%", 0.00);
    bench("1%", 0.01);
    bench("10%", 0.10);
    bench("50%", 0.50);
    bench("100%", 1.0);
    bench_group.finish();
}

fn bench_pointers_for(c: &mut Criterion) {
    const NUM_GETS_PER_MAP: usize = 1000;
    let bench_insert_inner = |bench: &mut Bencher<'_, _>, collision_ratio: &f64| {
        let preload_amt = 10_000;
        let max_range = max_range(preload_amt, *collision_ratio);

        let mut rng = thread_rng();
        let mut map = PointerMap::default();
        let preloaded = gen_hash_and_ptrs(&mut rng, max_range, preload_amt).collect::<Vec<_>>();
        let queries = preloaded
            .choose_multiple(&mut rng, NUM_GETS_PER_MAP)
            .collect::<Vec<_>>();

        for (row_hash, ptr) in &preloaded {
            map.insert(*row_hash, *ptr);
        }

        bench.iter_custom(|iters| {
            let mut total_duration = Duration::from_secs(0);
            let mut num_iters = 0;
            while num_iters < iters {
                for &&(hash, _) in &queries {
                    // Compute duration of map insertion.
                    total_duration += time(|| {
                        black_box(map.pointers_for(black_box(hash)));
                    });

                    num_iters += 1;
                    if num_iters >= iters {
                        break;
                    }
                }
            }
            total_duration
        });
    };
    let mut bench_group = c.benchmark_group("pointers_for");
    bench_group.throughput(Throughput::Elements(1));
    let mut bench = |percent, prob: f64| {
        bench_group.bench_with_input(
            BenchmarkId::new("load/10_000/get/1000/collisions", percent),
            &prob,
            bench_insert_inner,
        );
    };
    bench("0%", 0.00);
    bench("1%", 0.01);
    bench("10%", 0.10);
    bench("50%", 0.50);
    bench("100%", 1.0);

    bench_group.finish();
}

criterion_group!(benches, bench_insert, bench_pointers_for);
criterion_main!(benches);
