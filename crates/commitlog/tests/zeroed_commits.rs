use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::Path;

use spacetimedb_commitlog::payload::ArrayDecoder;
use spacetimedb_commitlog::repo::Fs;
use spacetimedb_commitlog::segment::Header as SegmentHeader;
use spacetimedb_commitlog::{Commit, Commitlog, Options};
use spacetimedb_paths::server::CommitLogDir;
use spacetimedb_paths::FromPathUnchecked;

const PAYLOAD_LEN: usize = 370;
const COMMIT_LEN: usize = Commit::FRAMING_LEN + PAYLOAD_LEN;

fn commit_bytes(tx_offset: u64, byte: u8) -> Vec<u8> {
    let commit = Commit {
        min_tx_offset: tx_offset,
        epoch: Commit::DEFAULT_EPOCH,
        n: 1,
        records: vec![byte; PAYLOAD_LEN],
    };
    let mut out = Vec::with_capacity(commit.encoded_len());
    commit.write(&mut out).unwrap();
    out
}

fn read_file(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn decode_commit_at(bytes: &[u8], offset: usize) -> Commit {
    let mut slice = &bytes[offset..];
    Commit::decode(&mut slice).unwrap().unwrap()
}

fn assert_zero_header_decode_error(bytes: &[u8], offset: usize) {
    let err = Commit::decode(&mut &bytes[offset..]).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

fn write_initial_commit(root: CommitLogDir) -> Result<(Fs, std::path::PathBuf), Box<dyn Error>> {
    let clog = Commitlog::open(root.clone(), Options::default(), None)?;
    clog.commit([(0, [0x11; PAYLOAD_LEN])])?;
    assert_eq!(clog.flush_and_sync()?, Some(0));

    let repo = Fs::new(root, None)?;
    let path = repo.segment_path(0).0;
    Ok((repo, path))
}

#[test]
fn stale_file_offset_writes_zero_filled_gap_after_truncate() -> Result<(), Box<dyn Error>> {
    assert_eq!(COMMIT_LEN, 396, "test payload should model observed commit size");

    let temp = tempfile::tempdir()?;
    let root = CommitLogDir::from_path_unchecked(temp.path());
    let (_repo, segment_path) = write_initial_commit(root)?;

    let first_commit_start = SegmentHeader::LEN;
    let first_commit_end = first_commit_start + COMMIT_LEN;
    let lost_commit_start = first_commit_end;
    let later_commit_start = lost_commit_start + COMMIT_LEN;

    let mut stale = OpenOptions::new().read(true).write(true).open(&segment_path)?;
    assert_eq!(stale.seek(SeekFrom::End(0))?, first_commit_end as u64);

    stale.write_all(&commit_bytes(1, 0x22))?;
    stale.sync_data()?;
    assert_eq!(stale.stream_position()?, later_commit_start as u64);

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&segment_path)?
        .set_len(first_commit_end as u64)?;

    stale.write_all(&commit_bytes(2, 0x33))?;
    stale.sync_data()?;

    let bytes = read_file(&segment_path)?;
    let zero_gap = &bytes[lost_commit_start..later_commit_start];
    assert!(
        zero_gap.iter().all(|byte| *byte == 0),
        "truncated range should read back as zeroes"
    );

    let first = decode_commit_at(&bytes, first_commit_start);
    let later = decode_commit_at(&bytes, later_commit_start);
    assert_eq!(first.min_tx_offset, 0);
    assert_eq!(later.min_tx_offset, 2);
    assert_zero_header_decode_error(&bytes, lost_commit_start);

    println!(
        "segment={} first_commit={}..{} zero_gap={}..{} len={} later_commit_start={} file_len={}",
        segment_path.display(),
        first_commit_start,
        first_commit_end,
        lost_commit_start,
        later_commit_start,
        zero_gap.len(),
        later_commit_start,
        bytes.len()
    );

    Ok(())
}

#[test]
fn zeroed_commit_reproduction_two_commit_sized_gaps() -> Result<(), Box<dyn Error>> {
    assert_eq!(COMMIT_LEN, 396, "test payload should model observed commit size");

    let temp = tempfile::tempdir()?;
    let root = CommitLogDir::from_path_unchecked(temp.path());
    let (_repo, segment_path) = write_initial_commit(root)?;

    let first_commit_start = SegmentHeader::LEN;
    let first_commit_end = first_commit_start + COMMIT_LEN;
    let first_lost_start = first_commit_end;
    let second_lost_start = first_lost_start + COMMIT_LEN;
    let later_commit_start = second_lost_start + COMMIT_LEN;

    let mut stale = OpenOptions::new().read(true).write(true).open(&segment_path)?;
    assert_eq!(stale.seek(SeekFrom::End(0))?, first_commit_end as u64);

    stale.write_all(&commit_bytes(1, 0x22))?;
    stale.write_all(&commit_bytes(2, 0x33))?;
    stale.sync_data()?;
    assert_eq!(stale.stream_position()?, later_commit_start as u64);

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&segment_path)?
        .set_len(first_commit_end as u64)?;

    stale.write_all(&commit_bytes(3, 0x44))?;
    stale.sync_data()?;

    let bytes = read_file(&segment_path)?;
    let first_zero_gap = &bytes[first_lost_start..second_lost_start];
    let second_zero_gap = &bytes[second_lost_start..later_commit_start];
    assert!(first_zero_gap.iter().all(|byte| *byte == 0));
    assert!(second_zero_gap.iter().all(|byte| *byte == 0));

    let first = decode_commit_at(&bytes, first_commit_start);
    let later = decode_commit_at(&bytes, later_commit_start);
    assert_eq!(first.min_tx_offset, 0);
    assert_eq!(later.min_tx_offset, 3);
    assert_zero_header_decode_error(&bytes, first_lost_start);

    println!(
        "segment={} first_commit={}..{} zero_gaps=[{}..{}, {}..{}] gap_lens=[{}, {}] later_commit_start={} file_len={}",
        segment_path.display(),
        first_commit_start,
        first_commit_end,
        first_lost_start,
        second_lost_start,
        second_lost_start,
        later_commit_start,
        first_zero_gap.len(),
        second_zero_gap.len(),
        later_commit_start,
        bytes.len()
    );

    Ok(())
}

#[test]
fn commitlog_traversal_stops_at_zeroed_commit_header() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let root = CommitLogDir::from_path_unchecked(temp.path());
    let (_repo, segment_path) = write_initial_commit(root.clone())?;
    let first_commit_end = SegmentHeader::LEN + COMMIT_LEN;
    let later_commit_start = first_commit_end + COMMIT_LEN;

    let mut stale = OpenOptions::new().read(true).write(true).open(&segment_path)?;
    stale.seek(SeekFrom::End(0))?;
    stale.write_all(&commit_bytes(1, 0x22))?;
    stale.sync_data()?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&segment_path)?
        .set_len(first_commit_end as u64)?;
    stale.write_all(&commit_bytes(2, 0x33))?;
    stale.sync_data()?;

    let txs = spacetimedb_commitlog::transactions(root, &ArrayDecoder::<PAYLOAD_LEN>)?.collect::<Result<Vec<_>, _>>();

    assert!(
        txs.is_err(),
        "read-only traversal should report the zeroed v1 commit header as corrupt"
    );

    println!(
        "segment={} traversal_tx_count={} zero_header_offset={} later_commit_start={}",
        segment_path.display(),
        "error",
        first_commit_end,
        later_commit_start
    );

    Ok(())
}

#[test]
fn simulated_zeroed_storage_overwrite_produces_same_raw_layout() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let root = CommitLogDir::from_path_unchecked(temp.path());
    let (_repo, segment_path) = write_initial_commit(root)?;

    let first_commit_start = SegmentHeader::LEN;
    let first_commit_end = first_commit_start + COMMIT_LEN;
    let overwritten_commit_start = first_commit_end;
    let later_commit_start = overwritten_commit_start + COMMIT_LEN;

    let mut file = OpenOptions::new().read(true).write(true).open(&segment_path)?;
    assert_eq!(file.seek(SeekFrom::End(0))?, first_commit_end as u64);
    file.write_all(&commit_bytes(1, 0x22))?;
    file.write_all(&commit_bytes(2, 0x33))?;
    file.sync_data()?;

    file.seek(SeekFrom::Start(overwritten_commit_start as u64))?;
    file.write_all(&vec![0; COMMIT_LEN])?;
    file.sync_data()?;

    let bytes = read_file(&segment_path)?;
    let zeroed_commit = &bytes[overwritten_commit_start..later_commit_start];
    assert!(zeroed_commit.iter().all(|byte| *byte == 0));

    let first = decode_commit_at(&bytes, first_commit_start);
    let later = decode_commit_at(&bytes, later_commit_start);
    assert_eq!(first.min_tx_offset, 0);
    assert_eq!(later.min_tx_offset, 2);
    assert_zero_header_decode_error(&bytes, overwritten_commit_start);

    println!(
        "segment={} first_commit={}..{} zeroed_overwrite={}..{} len={} later_commit_start={} file_len={}",
        segment_path.display(),
        first_commit_start,
        first_commit_end,
        overwritten_commit_start,
        later_commit_start,
        zeroed_commit.len(),
        later_commit_start,
        bytes.len()
    );

    Ok(())
}
