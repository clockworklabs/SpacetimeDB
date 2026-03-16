use log::info;
use spacetimedb_commitlog::tests::helpers::enable_logging;
use spacetimedb_commitlog::{payload, Commitlog, Options};
use spacetimedb_paths::server::CommitLogDir;
use spacetimedb_paths::FromPathUnchecked;
use tempfile::tempdir;

pub fn gen_payload() -> [u8; 256] {
    rand::random()
}

#[test]
fn smoke() {
    let root = tempdir().unwrap();
    let clog = Commitlog::open(
        CommitLogDir::from_path_unchecked(root.path()),
        Options {
            max_segment_size: 8 * 1024,
            ..Options::default()
        },
        None,
    )
    .unwrap();

    let n_txs = 500;
    let payload = gen_payload();
    for i in 0..n_txs {
        clog.commit([(i, payload)]).unwrap();
    }
    let committed_offset = clog.flush_and_sync().unwrap();

    assert_eq!(n_txs - 1, committed_offset.unwrap());
    assert_eq!(
        n_txs as usize,
        clog.transactions(&payload::ArrayDecoder).map(Result::unwrap).count()
    );
    // We set max_records_in_commit to 1, so n_commits == n_txs
    assert_eq!(n_txs as usize, clog.commits().map(Result::unwrap).count());
}

#[test]
fn resets() {
    let root = tempdir().unwrap();
    let mut clog = Commitlog::open(
        CommitLogDir::from_path_unchecked(root.path()),
        Options {
            max_segment_size: 512,
            ..Options::default()
        },
        None,
    )
    .unwrap();

    let payload = gen_payload();
    for i in 0..50 {
        clog.commit([(i, payload)]).unwrap();
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

#[test]
fn compression() {
    enable_logging();

    let root = tempdir().unwrap();
    let clog = Commitlog::open(
        CommitLogDir::from_path_unchecked(root.path()),
        Options {
            max_segment_size: 8 * 1024,
            ..Options::default()
        },
        None,
    )
    .unwrap();

    // try to generate commitlogs that will be amenable to compression -
    // random data doesn't compress well, so try and have there be repetition
    let payloads = (0..4).map(|_| gen_payload()).cycle().take(1024).collect::<Vec<_>>();
    for (i, payload) in payloads.iter().enumerate() {
        clog.commit([(i as u64, *payload)]).unwrap();
    }
    clog.flush_and_sync().unwrap();

    let uncompressed_size = clog.size_on_disk().unwrap();

    let segments = clog.existing_segment_offsets().unwrap();
    let segments_to_compress = &segments[..segments.len() / 2];
    info!("segments: {segments:?} compressing: {segments_to_compress:?}");
    clog.compress_segments(segments_to_compress).unwrap();

    let compressed_size = clog.size_on_disk().unwrap();
    assert!(compressed_size.total_bytes < uncompressed_size.total_bytes);

    assert!(clog
        .transactions(&payload::ArrayDecoder)
        .map(Result::unwrap)
        .enumerate()
        .all(|(i, x)| x.offset == i as u64 && x.txdata == payloads[i]));
}
