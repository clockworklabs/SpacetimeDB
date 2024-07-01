use std::num::NonZeroU16;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput};

mod common;
use common::Payload;
use spacetimedb_commitlog::{fs, Commitlog, Options};

use crate::common::tempdir;

fn append(c: &mut Criterion) {
    let mut group = c.benchmark_group("append");

    let payloads = {
        let mut rng = rand::thread_rng();
        let mut payloads = Vec::with_capacity(100);
        payloads.extend((0..100).map(|_| Payload::random(&mut rng, 16..1024)));
        payloads
    };

    const TXS_PER_COMMIT: [u16; 3] = [1, 32, 128];

    for direct_io in [false, true] {
        for max_records_in_commit in TXS_PER_COMMIT {
            let id =
                BenchmarkId::from_parameter(format!("direct-io={} tx/commit={}", direct_io, max_records_in_commit));
            let max_records_in_commit = NonZeroU16::new(max_records_in_commit).unwrap();

            group
                .sample_size(10)
                .sampling_mode(SamplingMode::Flat)
                .throughput(Throughput::Elements(1000))
                .bench_with_input(
                    id,
                    &(direct_io, max_records_in_commit, &payloads),
                    |b, &(direct_io, max_records_in_commit, payloads)| {
                        let tmp = tempdir().unwrap();
                        let clog = Commitlog::open(
                            tmp.path(),
                            Options {
                                max_records_in_commit,
                                fs_options: fs::Options {
                                    direct_io,
                                    sync_io: false,
                                },
                                ..Default::default()
                            },
                        )
                        .unwrap();

                        b.iter(|| {
                            for i in 0..1000 {
                                for payload in payloads {
                                    let mut retry = Some(payload);
                                    while let Some(txdata) = retry.take() {
                                        if let Err(txdata) = clog.append(txdata) {
                                            clog.flush().unwrap();
                                            retry = Some(txdata);
                                        }
                                    }
                                }
                                if i % 10 == 0 {
                                    clog.flush_and_sync().unwrap();
                                }
                            }
                            clog.flush_and_sync().unwrap();
                        })
                    },
                );
        }
    }
    group.finish();
}

criterion_group!(benches, append);
criterion_main!(benches);
