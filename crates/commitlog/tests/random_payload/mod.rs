use std::num::NonZeroU16;

use rand::Rng;
use spacetimedb_commitlog::{payload, Commitlog, Options};
use spacetimedb_paths::server::CommitLogDir;
use tempfile::tempdir;

fn gen_payload() -> [u8; 256] {
    let mut rng = rand::thread_rng();
    let mut buf = [0u8; 256];
    rng.fill(&mut buf);
    buf
}

#[test]
fn smoke() {
    let root = tempdir().unwrap();
    let clog = Commitlog::open(
        CommitLogDir(root.path().into()),
        Options {
            max_segment_size: 8 * 1024,
            max_records_in_commit: NonZeroU16::MIN,
            ..Options::default()
        },
    )
    .unwrap();

    let n_txs = 500;
    let payload = gen_payload();
    for _ in 0..n_txs {
        clog.append_maybe_flush(payload).unwrap();
    }
    let committed_offset = clog.flush_and_sync().unwrap();

    assert_eq!(n_txs - 1, committed_offset.unwrap() as usize);
    assert_eq!(
        n_txs,
        clog.transactions(&payload::ArrayDecoder).map(Result::unwrap).count()
    );
    // We set max_records_in_commit to 1, so n_commits == n_txs
    assert_eq!(n_txs, clog.commits().map(Result::unwrap).count());
}

#[test]
fn resets() {
    let root = tempdir().unwrap();
    let mut clog = Commitlog::open(
        CommitLogDir(root.path().into()),
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
