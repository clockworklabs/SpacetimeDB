//! `std` I/O benchmarks using plain bytes as payload (i.e. no serialization)

use std::{ops::Range, path::PathBuf};

use criterion::{
    black_box, criterion_group, criterion_main, measurement::Measurement, Bencher, BenchmarkGroup, Criterion,
};
use once_cell::sync::Lazy;
use rand::prelude::*;
use spacetimedb_commitlog::{Commitlog, Decoder, Encode};
use spacetimedb_sats::buffer::{BufReader, BufWriter};
use tempfile::{tempdir_in, TempDir};

#[derive(Debug)]
struct Payload(Vec<u8>);

impl Encode for Payload {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        writer.put_u64(self.0.len() as u64);
        writer.put_slice(&self.0[..]);
    }
}

impl Encode for &Payload {
    fn encode_record<W: BufWriter>(&self, writer: &mut W) {
        Encode::encode_record(*self, writer)
    }
}

struct PayloadDecoder;

impl Decoder for PayloadDecoder {
    type Error = anyhow::Error;
    type Record = Payload;

    fn decode_record<'a, R: BufReader<'a>>(
        &self,
        _version: u8,
        _tx_offset: u64,
        reader: &mut R,
    ) -> Result<Self::Record, Self::Error> {
        let len = reader.get_u64()?;
        let data = reader.get_slice(len as usize)?;

        Ok(Payload(data.to_vec()))
    }
}

/// Generate `num` random byte payloads with a length in the range `len_range`.
fn gen_payloads(num: usize, len_range: Range<usize>) -> Vec<Payload> {
    let mut rng = rand::thread_rng();
    let mut payloads = Vec::with_capacity(num);
    for _ in 0..num {
        let len = rng.gen_range(len_range.clone());
        let mut payload = Vec::with_capacity(len);
        rng.fill(&mut payload[..]);
        payloads.push(Payload(payload));
    }

    payloads
}

fn bench_function<F, M>(group: &mut BenchmarkGroup<M>, x: usize, f: F)
where
    F: FnMut(&mut Bencher<M>),
    M: Measurement,
{
    group
        .sample_size(10)
        .sampling_mode(criterion::SamplingMode::Flat)
        .throughput(criterion::Throughput::Elements(x as u64))
        .bench_function(x.to_string(), f);
}

static DEFAULT_PAYLOADS: Lazy<Vec<Payload>> = Lazy::new(|| gen_payloads(100, 16..1024));
const NUM_APPEND_TXS: usize = 5_000;
const NUM_TRAVERSE_TXS: usize = 10_000;

fn tempdir() -> TempDir {
    tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

fn append(c: &mut Criterion) {
    let mut group = c.benchmark_group("append");

    let payloads = Lazy::force(&DEFAULT_PAYLOADS);
    let tmp = tempdir();

    let go = |path: PathBuf| {
        let clog = Commitlog::open(path, Default::default()).unwrap();
        for _ in 0..NUM_APPEND_TXS {
            for payload in payloads {
                clog.append(payload).unwrap();
            }
            clog.flush_and_sync().unwrap();
        }
    };
    bench_function(&mut group, NUM_APPEND_TXS, |b| b.iter(|| go(tmp.path().to_path_buf())));
}

fn traverse(c: &mut Criterion) {
    let mut group = c.benchmark_group("traverse");

    let payloads = Lazy::force(&DEFAULT_PAYLOADS);
    let tmp = tempdir();
    {
        let clog = Commitlog::open(tmp.path(), Default::default()).unwrap();

        for _ in 0..NUM_TRAVERSE_TXS {
            for payload in payloads {
                clog.append(payload).unwrap();
            }
            clog.flush_and_sync().unwrap();
        }
    }

    let go = |path: PathBuf| {
        let clog: Commitlog<Payload> = Commitlog::open(path, Default::default()).unwrap();
        let decoder = PayloadDecoder;

        for tx in clog.transactions_from(0, &decoder) {
            black_box(tx.unwrap());
        }
    };
    bench_function(&mut group, NUM_TRAVERSE_TXS, |b| {
        b.iter(|| go(tmp.path().to_path_buf()))
    });
}

#[cfg(target_os = "linux")]
mod io_uring {

    use std::rc::Rc;

    use super::*;

    use criterion::async_executor::AsyncExecutor;
    use futures::{pin_mut, stream::FuturesOrdered, Future, StreamExt as _};
    use spacetimedb_commitlog::io_uring;

    struct Turing(tokio_uring::Runtime);

    impl Default for Turing {
        fn default() -> Self {
            let rt = tokio_uring::Runtime::new(&tokio_uring::builder()).expect("could not create tokio-uring runtime");
            Self(rt)
        }
    }

    impl AsyncExecutor for Turing {
        fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
            self.0.block_on(future)
        }
    }

    pub fn append(c: &mut Criterion) {
        let mut group = c.benchmark_group("async/append");

        let payloads = Lazy::force(&DEFAULT_PAYLOADS);
        let tmp = tempdir();

        let go = |path: PathBuf| async move {
            let clog = Rc::new(io_uring::Commitlog::open(path, Default::default()).await.unwrap());
            {
                for _ in 0..NUM_APPEND_TXS {
                    let tasks = payloads
                        .iter()
                        .map(|payload| {
                            let clog = clog.clone();
                            async move { clog.append(payload).await }
                        })
                        .collect::<FuturesOrdered<_>>();
                    pin_mut!(tasks);
                    while let Some(res) = tasks.next().await {
                        res.unwrap();
                    }
                    clog.flush_and_sync().await.unwrap();
                }
            }
            Rc::into_inner(clog).unwrap().close().await.unwrap();
        };

        bench_function(&mut group, NUM_APPEND_TXS, move |b| {
            b.to_async(Turing::default()).iter(|| go(tmp.path().to_path_buf()))
        });
    }

    pub fn traverse(c: &mut Criterion) {
        let mut group = c.benchmark_group("async/traverse");

        let payloads = Lazy::force(&DEFAULT_PAYLOADS);
        let tmp = tempdir();
        {
            tokio_uring::start(async {
                let clog = io_uring::Commitlog::open(tmp.path(), Default::default()).await.unwrap();
                for _ in 0..NUM_TRAVERSE_TXS {
                    for payload in payloads {
                        clog.append(payload).await.unwrap();
                    }
                    clog.flush_and_sync().await.unwrap();
                }
                clog.close().await.unwrap();
            });
        }

        let go = |path: PathBuf| async move {
            let clog = io_uring::Commitlog::<Payload>::open(path, Default::default())
                .await
                .unwrap();
            {
                let txs = clog.transactions_from(0, &PayloadDecoder);
                pin_mut!(txs);
                while let Some(tx) = txs.next().await {
                    black_box(tx.unwrap());
                }
            }
            clog.close().await.unwrap();
        };

        bench_function(&mut group, NUM_TRAVERSE_TXS, |b| {
            b.to_async(Turing::default()).iter(|| go(tmp.path().to_path_buf()))
        });
    }
}

#[cfg(target_os = "linux")]
criterion_group!(benches, append, io_uring::append, traverse, io_uring::traverse);
#[cfg(not(target_os = "linux"))]
criterion_group!(benches, append, traverse);

criterion_main!(benches);
