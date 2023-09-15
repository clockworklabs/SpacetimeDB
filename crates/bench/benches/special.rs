use criterion::{criterion_group, criterion_main, Criterion};

fn criterion_benchmark(_c: &mut Criterion) {

    /*
    let runtime = start_runtime();

    c.bench_function("meta/criterion_async_bench", |b| {
        b.to_async(&runtime).iter(|| async move {});
    });
    c.bench_function("meta/criterion_async_block_on", |b| {
        b.iter(|| runtime.block_on(async move {}));
    });
    */
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
