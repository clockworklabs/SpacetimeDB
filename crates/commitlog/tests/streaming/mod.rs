use std::{
    io,
    num::{NonZeroU16, NonZeroU64},
    ops::RangeBounds,
    path::{Path, PathBuf},
};

use futures::StreamExt as _;
use log::info;
use spacetimedb_commitlog::{
    repo::{self, Repo, SegmentLen},
    stream::{self, OnTrailingData, StreamWriter},
    tests::helpers::enable_logging,
    Commitlog, Options,
};
use spacetimedb_paths::{server::CommitLogDir, FromPathUnchecked as _};
use tempfile::tempdir;
use tokio::{
    fs,
    io::{AsyncBufRead, AsyncReadExt, BufReader},
    pin,
    task::spawn_blocking,
};
use tokio_stream::wrappers::ReadDirStream;
use tokio_util::io::StreamReader;

use super::random_payload;

#[tokio::test]
async fn copy_all() {
    enable_logging();

    let root = tempdir().unwrap();
    let (src, dst) = create_dirs(root.path()).await;
    fill_log(src.clone()).await;

    let writer = create_writer(dst.clone())
        .await
        .expect("failed to create stream writer");
    let reader = create_reader(&src, ..);
    pin!(reader);
    writer
        .append_all(reader, |_| ())
        .await
        .unwrap()
        .sync_all()
        .await
        .unwrap();

    assert_equal_dirs(&src, &dst).await
}

#[tokio::test]
async fn copy_ranges() {
    enable_logging();

    let root = tempdir().unwrap();
    let (src, dst) = create_dirs(root.path()).await;
    fill_log(src.clone()).await;

    let mut writer = create_writer(dst.clone())
        .await
        .expect("failed to create stream writer");

    for (start, end) in [(0, 25), (25, 50), (50, 75), (75, 101)] {
        info!("appending range {start}..{end}");
        let reader = create_reader(&src, start..end);
        pin!(reader);
        writer = writer.append_all(reader, |_| ()).await.unwrap();
    }
    writer.sync_all().await.unwrap();

    assert_equal_dirs(&src, &dst).await
}

#[tokio::test]
async fn copy_invalid_range() {
    enable_logging();

    let root = tempdir().unwrap();
    let (src, dst) = create_dirs(root.path()).await;
    fill_log(src.clone()).await;

    let mut writer = create_writer(dst.clone()).await.expect("failed to create writer");

    {
        info!("appending `..50`");
        let reader = create_reader(&src, ..50);
        pin!(reader);
        writer = writer.append_all(reader, |_| ()).await.unwrap();
        writer.sync_all().await.unwrap();
    }
    {
        info!("appending `75..`");
        let reader = create_reader(&src, 75..);
        pin!(reader);
        pretty_assertions::assert_matches!(
            writer.append_all(reader, |_| ()).await.map(drop),
            Err(e) if e.kind() == io::ErrorKind::InvalidData
        );
    }
}

#[tokio::test]
async fn trim_garbage() {
    enable_logging();

    let root = tempdir().unwrap();
    let (src, dst) = create_dirs(root.path()).await;
    fill_log(src.clone()).await;

    {
        let writer = create_writer(dst.clone())
            .await
            .expect("failed to create stream writer");
        let reader = create_reader(&src, ..);
        pin!(reader);
        writer.append_all(reader, |_| ()).await.unwrap();
        assert_equal_dirs(&src, &dst).await
    }

    // Truncate the destination log so the last commit is broken.
    spawn_blocking({
        let repo = repo(&dst);
        move || {
            let last_segment_offset = repo.existing_offsets().unwrap().pop().unwrap();
            let mut segment = repo.open_segment_writer(last_segment_offset).unwrap();
            let len = segment.segment_len().unwrap();
            segment.set_len(len - 128).unwrap();
        }
    })
    .await
    .unwrap();
    // The default is to return an error.
    pretty_assertions::assert_matches!(
        create_writer(dst.clone()).await.map(drop),
        Err(e) if e.kind() == io::ErrorKind::InvalidData
    );

    // With `Trim`, we can retry from commit 99.
    let writer = spawn_blocking({
        let path = dst.clone();
        move || StreamWriter::create(repo(&path), default_options(), OnTrailingData::Trim)
    })
    .await
    .unwrap()
    .expect("failed to create stream writer");
    let reader = create_reader(&src, 99..);
    pin!(reader);
    writer
        .append_all(reader, |_| ())
        .await
        .unwrap()
        .sync_all()
        .await
        .unwrap();

    assert_equal_dirs(&src, &dst).await
}

async fn assert_equal_dirs(src: &Path, dst: &Path) {
    let mut src_dir = fs::read_dir(src).await.map(ReadDirStream::new).unwrap();
    let mut buf_a = vec![];
    let mut buf_b = vec![];
    while let Some(entry) = src_dir.next().await.map(Result::unwrap) {
        if entry.file_type().await.unwrap().is_file() {
            let src_path = entry.path();
            let dst_path = dst.join(src_path.file_name().unwrap());

            let mut src_file = fs::File::open(&src_path).await.unwrap();
            let mut dst_file = fs::File::open(&dst_path).await.unwrap();

            src_file.read_to_end(&mut buf_a).await.unwrap();
            dst_file.read_to_end(&mut buf_b).await.unwrap();

            assert_eq!(buf_a, buf_b, "{} and {} differ", src_path.display(), dst_path.display());
        }
        buf_a.clear();
        buf_b.clear();
    }
}

fn default_options() -> Options {
    Options {
        max_segment_size: 8 * 1024,
        max_records_in_commit: NonZeroU16::MIN,
        // Write an index entry for every commit.
        offset_index_interval_bytes: NonZeroU64::new(256).unwrap(),
        offset_index_require_segment_fsync: false,
        ..Options::default()
    }
}

async fn fill_log(path: PathBuf) {
    spawn_blocking(move || {
        let clog = Commitlog::open(CommitLogDir::from_path_unchecked(path), default_options(), None).unwrap();
        let payload = random_payload::gen_payload();
        for _ in 0..100 {
            clog.append_maybe_flush(payload).unwrap();
        }
        clog.flush_and_sync().unwrap();
    })
    .await
    .unwrap();
}

async fn create_writer(path: PathBuf) -> io::Result<StreamWriter<repo::Fs>> {
    spawn_blocking(move || StreamWriter::create(repo(&path), default_options(), OnTrailingData::Error))
        .await
        .unwrap()
}

fn repo(at: &Path) -> repo::Fs {
    repo::Fs::new(CommitLogDir::from_path_unchecked(at), None).unwrap()
}

fn create_reader(path: &Path, range: impl RangeBounds<u64>) -> impl AsyncBufRead {
    BufReader::new(StreamReader::new(stream::commits(
        repo::Fs::new(CommitLogDir::from_path_unchecked(path), None).unwrap(),
        range,
    )))
}

async fn create_dirs(root: &Path) -> (PathBuf, PathBuf) {
    let src = root.join("a");
    let dst = root.join("b");
    fs::create_dir(&src).await.unwrap();
    fs::create_dir(&dst).await.unwrap();

    (src, dst)
}
