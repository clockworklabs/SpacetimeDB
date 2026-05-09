use log::info;
use spacetimedb_commitlog::repo::Repo;
use spacetimedb_commitlog::tests::helpers::enable_logging;
use spacetimedb_commitlog::{commitlog, payload, repo, Commitlog, Options};
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

/// Try to generate commitlogs that will be amenable to compression -
/// random data doesn't compress well, so try and have there be repetition
fn compressible_payloads() -> impl Iterator<Item = [u8; 256]> {
    (0..4).map(|_| gen_payload()).cycle()
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

    let payloads = compressible_payloads().take(1024).collect::<Vec<_>>();
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

/// When restoring an archived commitlog, all segments are compressed and should
/// remain immutable.
///
/// Tests that this is upheld, i.e. a fresh segment is created when resuming
/// writes.
#[test]
fn all_segments_sealed() {
    enable_logging();

    let root = tempdir().unwrap();
    let path = CommitLogDir::from_path_unchecked(root.path());
    let opts = Options {
        max_segment_size: 64 * 1024,
        ..<_>::default()
    };
    let num_commits = 1024;
    let repo = repo::Fs::new(path, None).unwrap();
    {
        let mut clog = commitlog::Generic::open(&repo, opts).unwrap();
        for (i, payload) in compressible_payloads().take(num_commits).enumerate() {
            clog.commit([(i as u64, payload)]).unwrap();
        }
        clog.flush().unwrap();
        clog.sync();
    }

    let segments = repo.existing_offsets().unwrap();
    let num_segments = segments.len();

    // Compress all segments via the `repo`,
    // to not trigger the assert that the head segment cannot be compressed.
    for segment in segments {
        repo.compress_segment(segment).unwrap();
    }

    // Re-opening the commitlog should create a fresh segment at offset `num_commits`.
    let _ = commitlog::Generic::<_, [u8; 256]>::open(&repo, opts).unwrap();
    let segments = repo.existing_offsets().unwrap();
    assert_eq!(num_segments + 1, segments.len());
    assert_eq!(segments.last().copied(), Some(num_commits as u64));
}

#[test]
fn resume_empty_segment() {
    enable_logging();

    let root = tempdir().unwrap();
    let path = CommitLogDir::from_path_unchecked(root.path());
    let opts = Options {
        max_segment_size: 64 * 1024,
        ..<_>::default()
    };
    let num_commits = 1024;
    let repo = repo::Fs::new(path, None).unwrap();
    {
        let mut clog = commitlog::Generic::open(&repo, opts).unwrap();
        for (i, payload) in compressible_payloads().take(num_commits).enumerate() {
            clog.commit([(i as u64, payload)]).unwrap();
        }
        clog.flush().unwrap();
        clog.sync();
    }

    let mut segments = repo.existing_offsets().unwrap();
    while let Some(last_segment) = segments.pop() {
        repo.open_segment_writer(last_segment).unwrap().set_len(0).unwrap();

        let _ = commitlog::Generic::<_, [u8; 256]>::open(&repo, opts).unwrap();
        let segments1 = repo.existing_offsets().unwrap();
        if segments.is_empty() {
            assert_eq!([0], segments1.as_slice());
        } else {
            assert_eq!(segments, segments1);
        }
    }
}
