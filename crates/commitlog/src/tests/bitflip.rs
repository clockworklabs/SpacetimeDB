use std::{
    fmt,
    iter::{repeat, successors},
    num::NonZeroU8,
    rc::Rc,
};

use log::debug;
use proptest::{prelude::*, sample::select};

use crate::{
    commit, commitlog, error, payload,
    repo::{self, mem::Segment, Repo},
    segment,
    tests::helpers::{enable_logging, fill_log, mem_log},
    Commit,
};

/// The serialized length of a commit's crc.
const CRC_SIZE: usize = 4;
/// Max size in bytes of a segment.
const MAX_SEGMENT_SIZE: usize = 1024;
/// Number of commits to generate.
const NUM_COMMITS: usize = 100;
/// Size in bytes of the (dummy) transactions to generate.
const TX_SIZE: usize = 32;
/// Number of transactions to generate per commit.
const TXS_PER_COMMIT: usize = 10;

/// The size in bytes of one commit according to above parameters.
const COMMIT_SIZE: usize = Commit::FRAMING_LEN + (TX_SIZE * TXS_PER_COMMIT) + CRC_SIZE;

/// Iterator yielding the start offsets of the commits in a segment.
fn commit_boundaries() -> impl Iterator<Item = usize> {
    successors(Some(segment::Header::LEN), |n| Some(n + COMMIT_SIZE)).take_while(|&x| x <= MAX_SEGMENT_SIZE)
}

type Log = Rc<commitlog::Generic<repo::Memory, [u8; TX_SIZE]>>;

fn mk_log() -> Log {
    let mut log = mem_log::<[u8; TX_SIZE]>(MAX_SEGMENT_SIZE as _);
    fill_log(&mut log, NUM_COMMITS, repeat(TXS_PER_COMMIT));
    Rc::new(log)
}

struct Inputs {
    log: Log,
    segment: Segment,
    byte_pos: usize,
    bit_mask: u8,

    // For debugging.
    #[allow(unused)]
    segment_offset: u64,
}

impl fmt::Debug for Inputs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inputs")
            .field("byte_pos", &self.byte_pos)
            .field("bit_mask", &self.bit_mask)
            .field("segment_offset", &self.segment_offset)
            .finish()
    }
}

impl Inputs {
    fn generate() -> impl Strategy<Value = Inputs> {
        // Open + fill log.
        let log = mk_log();
        // Obtain segment offsets.
        let segment_offsets = log.repo.existing_offsets().unwrap();
        (
            // Select a random segment.
            select(segment_offsets)
                // Open the segment at that offset,
                // and generate a byte position where we want a bit to be
                // flipped.
                .prop_flat_map(move |segment_offset| {
                    let segment = log.repo.open_segment(segment_offset).unwrap();
                    let byte_pos = byte_position(segment.len());
                    (Just(log.clone()), Just(segment), Just(segment_offset), byte_pos)
                }),
            // A byte to XOR with the byte at `byte_pos`
            any::<NonZeroU8>(),
        )
            .prop_map(|((log, segment, segment_offset, byte_pos), bit_mask)| Self {
                log,
                segment,
                byte_pos,
                bit_mask: bit_mask.get(),

                segment_offset,
            })
    }
}

/// Select a random position of a byte within a segment.
///
/// The position shall not fall on any headers (segment or commit), so as to
/// reliably provoke checksum errors (and not any other errors).
fn byte_position(segment_len: usize) -> impl Strategy<Value = usize> {
    (segment::Header::LEN + commit::Header::LEN + 1..segment_len).prop_map(|mut byte_pos| {
        for x in commit_boundaries() {
            if byte_pos >= x && byte_pos < x + COMMIT_SIZE {
                byte_pos = byte_pos.max(x + commit::Header::LEN + 1);
            }
        }
        byte_pos
    })
}

proptest! {
    #[test]
    fn detect_bitflip_during_traversal(inputs in Inputs::generate()) {
        enable_logging();
        debug!("TEST RUN: {inputs:?}");

        let Inputs {
            log,
            segment,
            byte_pos,
            bit_mask,

            segment_offset:_ ,
        } = inputs;

        {
            let mut data = segment.buf_mut();
            data[byte_pos] ^= bit_mask;
        }

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
