use std::{
    io::{self, SeekFrom},
    ops::RangeBounds,
};

use async_stream::try_stream;
use bytes::{Buf as _, Bytes};
use futures::Stream;
use log::{debug, trace, warn};
use tokio::{
    io::{AsyncBufRead, AsyncReadExt as _, AsyncSeek, AsyncSeekExt as _},
    task::spawn_blocking,
};
use tokio_util::io::SyncIoBridge;

use crate::{
    commit,
    repo::Repo,
    segment::{self, seek_to_offset, CHECKSUM_LEN},
};

use super::{
    common::{read_exact, CommitBuf},
    IntoAsyncSegment, RangeFromMaybeToInclusive,
};

/// Stream the `range` of transaction offsets from the commitlog in `repo` as
/// raw commitlog data.
///
/// The stream contains segment headers as they are encountered scanning the
/// `range`.
///
/// Only whole [`commit::StoredCommit`]s are yielded, so a `range` that doesn't
/// fall on commit boundaries may yield extra data.
///
/// Only the headers of the source commitlog are inspected (in order to be able
/// to satisfy the `range` predicate), so no guarantees are made about the
/// integrity of the log.
///
/// If the commitlog is empty, that is does not contain any commits, the
/// returned stream yields nothing.
pub fn commits<R>(repo: R, range: impl RangeBounds<u64>) -> impl Stream<Item = io::Result<Bytes>>
where
    R: Repo + Send + 'static,
    R::Segment: IntoAsyncSegment,
{
    let mut range = RangeFromMaybeToInclusive::from_range_bounds(range);
    let retain = move |segments: Vec<_>| retain_range(&segments, range);
    try_stream! {
        let segments = repo.existing_offsets().map(retain)?;
        for segment_offset in segments {
            if range.start < segment_offset {
                range.start = segment_offset;
            }
            trace!("segment: segment={} start={}", segment_offset, range.start);

            let segment = spawn_blocking({
                let repo = repo.clone();
                move || repo.open_segment(segment_offset)
            })
            .await
            .unwrap()?
            .into_async_reader();

            for await chunk in read_segment(repo.clone(), segment, segment_offset, range) {
                yield chunk.inspect_err(|e| warn!("error reading segment {}: {}", segment_offset, e))?;
            }
        }
    }
}

fn read_segment(
    repo: impl Repo + Send + 'static,
    mut segment: impl AsyncBufRead + AsyncSeek + Unpin + Send + 'static,
    segment_start: u64,
    range: RangeFromMaybeToInclusive,
) -> impl Stream<Item = io::Result<Bytes>> {
    try_stream! {
        trace!("reading segment {segment_start}");
        let (segment_header, segment_header_bytes) = {
            let mut buf = [0u8; segment::Header::LEN];
            segment.read_exact(&mut buf).await?;
            let header = segment::Header::decode(&buf[..])?;
            (header, Bytes::from_owner(buf))
        };
        let mut send_segment_header = Some(segment_header_bytes);

        // Try to seek to the starting offset
        // if it doesn't fall on the segment boundary.
        if range.start > segment_start {
            // Don't send a segment header if we're not reading from the start.
            send_segment_header = None;
            segment = spawn_blocking(move || {
                let mut segment = SyncIoBridge::new(segment);
                if let Ok(offset_index) = repo.get_offset_index(segment_start) {
                    debug!("seek_to_offset segment={} start={}", segment_start, range.start);
                    seek_to_offset(&mut segment, &offset_index, range.start)
                        .inspect_err(|e| {
                            warn!(
                                "error seeking to offset {} in segment {}: {}",
                                range.start, segment_start, e
                            )
                        })
                        .ok();
                }
                segment.into_inner()
            })
            .await
            .unwrap();
        }

        let checksum_len = CHECKSUM_LEN[segment_header.checksum_algorithm as usize];
        let mut commit_buf = CommitBuf::default();
        loop {
            if read_exact(&mut segment, &mut commit_buf.header).await?.is_eof() {
                trace!("eof reading commit header");
                break;
            }
            let Some(hdr) = commit::Header::decode(&commit_buf.header[..])? else {
                warn!("all-zeroes commit header");
                break;
            };
            // Skip the commit if we're not at `range.start`.
            if hdr.min_tx_offset < range.start {
                segment.seek(SeekFrom::Current(hdr.len as i64 + checksum_len as i64)).await?;
            // Stop if we're past the range end.
            } else if range.end.is_some_and(|end| hdr.min_tx_offset > end) {
                break
            } else {
                commit_buf.body.resize(hdr.len as usize + checksum_len, 0);
                segment.read_exact(&mut commit_buf.body).await?;

                // Send segment header if not sent already.
                if let Some(header_bytes) = send_segment_header.take() {
                    trace!("sending segment header");
                    yield header_bytes;
                }

                trace!("sending commit {}", hdr.min_tx_offset);
                yield commit_buf.as_buf().copy_to_bytes(commit_buf.filled_len());
            }
        }
    }
}

/// Given a list of (segment) offsets, retain those which fall into the `range`.
pub fn retain_range(offsets: &[u64], range: RangeFromMaybeToInclusive) -> Vec<u64> {
    if range.is_empty() {
        return vec![];
    }
    offsets
        .iter()
        .zip(offsets.iter().skip(1).chain([&u64::MAX]))
        .filter_map(|(&start, &end)| {
            let in_start = range.start >= start && range.start < end;
            (in_start || range.contains(&start)).then_some(start)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn retain_range(offsets: &[u64], range: impl RangeBounds<u64>) -> Vec<u64> {
        super::retain_range(offsets, RangeFromMaybeToInclusive::from_range_bounds(range))
    }

    #[test]
    fn test_slice_segments_on_boundary() {
        let offsets = vec![0, 10, 20, 30];

        for (i, start) in offsets.iter().enumerate() {
            let retained = retain_range(&offsets, start..);
            assert_eq!(&retained, &offsets[i..]);
        }
    }

    #[test]
    fn test_slice_segments_between_boundary() {
        let offsets = vec![0, 10, 20, 30];

        let ranges = vec![5, 11, 29];
        for (i, start) in ranges.into_iter().enumerate() {
            let retained = retain_range(&offsets, start..);
            assert_eq!(&retained, &offsets[i..]);
        }
    }

    #[test]
    fn test_slice_segments_with_upper_bound() {
        let offsets = vec![0, 10, 20, 30];
        let retained = retain_range(&offsets, 11..29);
        assert_eq!(&retained, &[10, 20]);
    }

    proptest! {
        #[test]
        fn prop_offset_at_or_after_last_segment_yields_last(start in 30u64..) {
            let offsets = vec![0, 10, 20, 30];
            let retained = retain_range(&offsets, start..);
            prop_assert_eq!(&retained, &[30]);
        }

        #[test]
        fn prop_empty_input_gives_empty_output(start in any::<u64>()) {
            let retained = retain_range(&[], start..);
            prop_assert_eq!(&retained, &[] as &[u64]);
        }

        #[test]
        fn prop_empty_range_retains_nothing(start in any::<u64>()) {
            let offsets = vec![0, 10, 20, 30];
            let range = start..start;
            prop_assert!(range.is_empty(), "expected range to be empty: {range:?}");
            let retained = retain_range(&offsets, range);
            prop_assert_eq!(&retained, &[] as &[u64]);
        }

        #[test]
        fn prop_offset_at_or_after_last_with_upper_bound_yields_last(start in 30u64..) {
            let offsets = vec![0, 10, 20, 30];
            let retained = retain_range(&offsets, start..start + 16);
            prop_assert_eq!(&retained, &[30]);
        }
    }
}
