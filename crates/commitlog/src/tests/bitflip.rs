use std::{
    cmp,
    io::{Read, Seek, SeekFrom, Write},
    iter::{repeat, successors},
};

use log::debug;
use proptest::bits::u64;
use rand::prelude::*;

use crate::{
    commit, error, payload,
    repo::Repo,
    segment,
    tests::helpers::{enable_logging, fill_log, mem_log},
    Commit,
};

#[test]
fn traversal() {
    enable_logging();

    const NUM_COMMITS: usize = 100;
    const TX_SIZE: usize = 32;
    const TXS_PER_COMMIT: usize = 10;
    const COMMIT_SIZE: usize = Commit::FRAMING_LEN + (TX_SIZE * TXS_PER_COMMIT);

    let mut log = mem_log::<[u8; TX_SIZE]>(1024);
    fill_log(&mut log, NUM_COMMITS, repeat(TXS_PER_COMMIT));

    {
        // TODO: Allow supplying a seed, though env or whatever
        let mut rng = thread_rng();

        let segments = log.repo.existing_offsets().unwrap();
        debug!("segments={segments:?}");
        let segment_offset = segments.choose(&mut rng).copied().unwrap();
        let mut segment = log.repo.open_segment(segment_offset).unwrap();

        // Make sure we don't touch the commit header, so we're sure that the
        // error will be a checksum mismatch. If we'd match on any out-of-order
        // error, we might be missing error conditions we hadn't thought of.
        let mut pos = rng.gen_range(segment::Header::LEN + commit::Header::LEN..segment.len());
        for x in successors(Some(0), |n| Some(n * COMMIT_SIZE)).take(NUM_COMMITS) {
            if pos >= x && pos < x + COMMIT_SIZE {
                pos = cmp::max(x + commit::Header::LEN + 1, pos);
            }
        }

        debug!("flipping {pos} of {} in {segment_offset}", segment.len());

        segment.seek(SeekFrom::Start(pos as u64)).unwrap();
        let mut buf = [0; 1];
        segment.read_exact(&mut buf).unwrap();
        buf[0] ^= rng.gen::<u8>();
        segment.seek(SeekFrom::Current(-1)).unwrap();
        segment.write_all(&buf).unwrap();
    }

    let first_err = log
        .transactions_from(0, payload::ArrayDecoder)
        .find_map(Result::err)
        .expect("unexpected success");
    let unexpected = match first_err {
        error::Traversal::OutOfOrder {
            prev_error: Some(prev_error),
            ..
        } if matches!(*prev_error, error::Traversal::Checksum { .. }) => None,
        error::Traversal::Checksum { .. } => None,
        e => Some(e),
    };
    assert!(unexpected.is_none(), "unexpected error: {unexpected:?}");
}
