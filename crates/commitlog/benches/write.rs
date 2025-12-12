use core::fmt;
use std::num::NonZeroU16;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput};
use spacetimedb_commitlog::{Commitlog, Options};
use spacetimedb_paths::{server::CommitLogDir, FromPathUnchecked as _};
use tempfile::tempdir_in;

mod common;
use common::Payload;

struct Params {
    payloads: Box<[Payload]>,
    txs_per_commit: NonZeroU16,
    total_appends: u64,
    fsync_every: u64,
}

impl Params {
    fn with_payloads(payloads: impl Into<Box<[Payload]>>) -> Self {
        Self {
            payloads: payloads.into(),
            txs_per_commit: NonZeroU16::new(1).unwrap(),
            total_appends: 1_000,
            fsync_every: 32,
        }
    }
}

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "n={} tx/commit={} fsync={}",
            self.total_appends, self.txs_per_commit, self.fsync_every
        )
    }
}

fn bench_append(c: &mut Criterion, label: &str, params: Params) {
    let id = BenchmarkId::from_parameter(&params);
    c.benchmark_group(label)
        .sample_size(10)
        .sampling_mode(SamplingMode::Flat)
        .throughput(Throughput::Elements(params.total_appends))
        .bench_with_input(
            id,
            &params,
            |b,
             Params {
                 payloads,
                 txs_per_commit,
                 total_appends,
                 fsync_every,
             }| {
                let tmp = tempdir_in(".").unwrap();
                let clog = Commitlog::open(
                    CommitLogDir::from_path_unchecked(tmp.path()),
                    Options {
                        max_records_in_commit: *txs_per_commit,
                        ..<_>::default()
                    },
                    None,
                )
                .unwrap();

                b.iter(|| {
                    let mut payloads = payloads.iter().cycle();
                    for i in 0..*total_appends {
                        let payload = payloads.next().unwrap();
                        let mut retry = Some(payload);
                        while let Some(txdata) = retry.take() {
                            if let Err(txdata) = clog.append(txdata) {
                                clog.flush().unwrap();
                                retry = Some(txdata);
                            }
                        }
                        if i % fsync_every == 0 {
                            clog.flush_and_sync().unwrap();
                        }
                    }
                    clog.flush_and_sync().unwrap();
                })
            },
        );
}

fn baseline(c: &mut Criterion) {
    let params = Params::with_payloads([Payload::new([b'z'; 64])]);
    bench_append(c, "baseline", params);
}

fn large_payload(c: &mut Criterion) {
    let params = Params::with_payloads([Payload::new([b'z'; 4096])]);
    bench_append(c, "large payload", params);
}

fn mixed_payloads(c: &mut Criterion) {
    let params = Params::with_payloads([
        Payload::new([b'a'; 64]),
        Payload::new([b'b'; 512]),
        Payload::new([b'c'; 1024]),
        Payload::new([b'd'; 4096]),
        Payload::new([b'e'; 8102]),
    ]);
    bench_append(c, "mixed payloads", params);
}

fn mixed_payloads_with_batching(c: &mut Criterion) {
    let params = Params {
        txs_per_commit: NonZeroU16::new(16).unwrap(),
        ..Params::with_payloads([
            Payload::new([b'a'; 64]),
            Payload::new([b'b'; 512]),
            Payload::new([b'c'; 1024]),
            Payload::new([b'd'; 4096]),
            Payload::new([b'e'; 8102]),
        ])
    };
    bench_append(c, "mixed payloads with batching", params);
}

criterion_group!(
    benches,
    baseline,
    large_payload,
    mixed_payloads,
    mixed_payloads_with_batching
);
criterion_main!(benches);
