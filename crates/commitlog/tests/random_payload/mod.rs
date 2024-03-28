use std::{num::NonZeroU16, time::Instant};

use log::info;
use rand::Rng;
use spacetimedb_commitlog::{payload, Commitlog, Options};
use tempfile::tempdir_in;

use crate::{enable_logging, tempdir};

fn gen_payload() -> [u8; 256] {
    let mut rng = rand::thread_rng();
    let mut buf = [0u8; 256];
    rng.fill(&mut buf);
    buf
}

#[test]
fn smoke() {
    enable_logging(log::LevelFilter::Info);

    let n_txs = 10_000;

    let root = tempdir_in(tempdir()).unwrap();
    let clog = Commitlog::open(
        root.path(),
        Options {
            max_segment_size: 8 * 1024,
            max_records_in_commit: NonZeroU16::MIN,
            ..Options::default()
        },
    )
    .unwrap();

    let payload = gen_payload();

    let start = Instant::now();
    for _ in 0..n_txs {
        clog.append_maybe_flush(payload).unwrap();
    }
    let committed_offset = clog.flush_and_sync().unwrap();
    let elapsed = start.elapsed();
    info!("wrote {} txs in {}ms", n_txs, elapsed.as_millis(),);

    let start = Instant::now();
    let n = clog.transactions(&payload::ArrayDecoder).map(Result::unwrap).count();
    let elapsed = start.elapsed();
    info!("read {} txs in {}ms", n, elapsed.as_millis());

    assert_eq!(n_txs - 1, committed_offset.unwrap() as usize);
    assert_eq!(n_txs, n);
    // We set max_records_in_commit to 1, so n_commits == n_txs
    assert_eq!(n_txs, clog.commits().map(Result::unwrap).count());
}

#[test]
fn resets() {
    enable_logging(log::LevelFilter::Info);

    let root = tempdir_in(tempdir()).unwrap();
    let mut clog = Commitlog::open(
        root.path(),
        Options {
            max_segment_size: 512,
            max_records_in_commit: NonZeroU16::MIN,
            ..Options::default()
        },
    )
    .unwrap();

    let payload = gen_payload();
    for _ in 0..50 {
        clog.append_maybe_flush(payload).unwrap();
    }
    clog.flush_and_sync().unwrap();

    for offset in (0..50).rev() {
        clog = clog.reset_to(offset).unwrap();
        assert_eq!(
            offset,
            clog.transactions(&payload::ArrayDecoder)
                .map(Result::unwrap)
                .last()
                .unwrap()
                .offset
        );
        // We're counting from zero, so offset + 1 is the # of txs.
        assert_eq!(
            offset + 1,
            clog.transactions(&payload::ArrayDecoder).map(Result::unwrap).count() as u64
        );
    }
}
