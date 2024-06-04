use std::{
    cmp,
    io::{Read, Seek, SeekFrom, Write},
    iter::{repeat, successors},
    rc::Rc,
};

use proptest::{prelude::*, sample::select};

use crate::{
    commit, commitlog, error, payload,
    repo::{self, mem::Segment, Repo},
    segment,
    tests::helpers::{enable_logging, fill_log, mem_log},
    Commit,
};

const NUM_COMMITS: usize = 100;
const TX_SIZE: usize = 32;
const TXS_PER_COMMIT: usize = 10;
const COMMIT_SIZE: usize = Commit::FRAMING_LEN + (TX_SIZE * TXS_PER_COMMIT);

fn mk_log() -> commitlog::Generic<repo::Memory, [u8; TX_SIZE]> {
    let mut log = mem_log::<[u8; TX_SIZE]>(1024);
    fill_log(&mut log, NUM_COMMITS, repeat(TXS_PER_COMMIT));
    log
}

type Log = Rc<commitlog::Generic<repo::Memory, [u8; TX_SIZE]>>;

#[derive(Debug)]
struct Inputs {
    log: Log,
    segment: Segment,
    byte_pos: usize,
    bit_mask: u8,
}

impl Inputs {
    fn generate() -> impl Strategy<Value = Inputs> {
        // hey proptest, `prop_compose` doesn't make this any more pleasant.
        // how about, you know, do-notation?
        (
            // Open + fill log, obtain segment offsets.
            Just({
                let log = mk_log();
                let segment_offsets = log.repo.existing_offsets().unwrap();
                (Rc::new(log), segment_offsets)
            })
            // Select a random segment offset.
            .prop_flat_map(|(log, segment_offsets)| (Just(log), select(segment_offsets)))
            // Open the segment at that offset.
            .prop_map(|(log, offset)| {
                let segment = log.repo.open_segment(offset).unwrap();
                (log, segment)
            })
            // Generate a byte position where we want a bit to be flipped.
            // The offset shall be past the segment + first commit headers,
            // so as to reliably provoke checksum errors (and not any other
            // errors).
            .prop_flat_map(|(log, segment)| {
                let byte_pos = segment::Header::LEN + commit::Header::LEN..segment.len();
                (Just(log), Just(segment), byte_pos)
            }),
            // A byte to XOR with the byte at `byte_pos`
            any::<u8>(),
        )
            .prop_map(|((log, segment, byte_pos), bit_mask)| Self {
                log,
                segment,
                byte_pos,
                bit_mask,
            })
    }
}

proptest! {
    #[test]
    fn detect_bitflip_during_traversal(inputs in Inputs::generate()) {
        enable_logging();

        let Inputs { log, mut segment, mut byte_pos, bit_mask } = inputs;

        // Make sure we don't touch the commit header, so we're sure that the
        // error will be a checksum mismatch. If we'd match on any out-of-order
        // error, we might be missing error conditions we hadn't thought of.
        for x in successors(Some(0), |n| Some(n * COMMIT_SIZE)).take(NUM_COMMITS) {
            if byte_pos >= x && byte_pos < x + COMMIT_SIZE {
                byte_pos = cmp::max(x + commit::Header::LEN + 1, byte_pos);
            }
        }

        segment.seek(SeekFrom::Start(byte_pos as u64)).unwrap();
        let mut buf = [0; 1];
        segment.read_exact(&mut buf).unwrap();
        buf[0] ^= bit_mask;
        segment.seek(SeekFrom::Current(-1)).unwrap();
        segment.write_all(&buf).unwrap();

        let first_err = log
            .transactions_from(0, &payload::ArrayDecoder)
            .find_map(Result::err)
            .expect("unexpected success");
        let unexpected = match first_err {
            payload::ArrayDecodeError::Traversal(error::Traversal::OutOfOrder {
                prev_error: Some(prev_error),
                ..
            }) if matches!(*prev_error, error::Traversal::Checksum { .. }) => None,
            payload::ArrayDecodeError::Traversal(error::Traversal::Checksum { .. }) => None,
            e => Some(e),
        };
        assert!(unexpected.is_none(), "unexpected error: {unexpected:?}");
    }
}
