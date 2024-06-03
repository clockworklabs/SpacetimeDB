use std::{io, marker::PhantomData, mem, vec};

use itertools::Itertools;
use log::{debug, info, trace, warn};

use crate::{
    error,
    payload::Decoder,
    repo::{self, Repo},
    segment::{self, FileLike, Transaction, Writer},
    Commit, Encode, Options,
};

#[derive(Debug)]
pub struct Generic<R: Repo, T> {
    pub(crate) repo: R,
    pub(crate) head: Writer<R::Segment>,
    tail: Vec<u64>,
    opts: Options,
    _record: PhantomData<T>,
}

impl<R: Repo, T> Generic<R, T> {
    pub fn open(repo: R, opts: Options) -> io::Result<Self> {
        let mut tail = repo.existing_offsets()?;
        if !tail.is_empty() {
            debug!("segments: {tail:?}");
        }
        let head = if let Some(last) = tail.pop() {
            debug!("resuming last segment: {last}");
            repo::resume_segment_writer(&repo, opts, last)?.or_else(|meta| {
                tail.push(meta.tx_range.start);
                repo::create_segment_writer(&repo, opts, meta.tx_range.end)
            })?
        } else {
            debug!("starting fresh log");
            repo::create_segment_writer(&repo, opts, 0)?
        };

        Ok(Self {
            repo,
            head,
            tail,
            opts,
            _record: PhantomData,
        })
    }

    /// Write the currently buffered data to disk and rotate segments as
    /// necessary.
    ///
    /// Note that this does not imply that the data is durable, call
    /// [`Self::sync`] to flush OS buffers.
    ///
    /// # Errors
    ///
    /// If an error occurs writing the data, the current [`Commit`] buffer is
    /// retained, but a new segment is created. Retrying in case of an `Err`
    /// return value thus will write the current data to that new segment.
    ///
    /// If this fails, however, the next attempt to create a new segment will
    /// fail with [`io::ErrorKind::AlreadyExists`]. Encountering this error kind
    /// this means that something is seriously wrong underlying storage, and the
    /// caller should stop writing to the log.
    pub fn commit(&mut self) -> io::Result<usize> {
        let writer = &mut self.head;
        let sz = writer.commit.encoded_len();
        // If the segment is empty, but the commit exceeds the max size,
        // we got a huge commit which needs to be written even if that
        // results in a huge segment.
        let should_rotate = !writer.is_empty() && writer.len() + sz as u64 > self.opts.max_segment_size;
        let writer = if should_rotate {
            if let Err(e) = writer.fsync() {
                warn!("Failed to fsync segment: {e}");
            }
            self.start_new_segment()?
        } else {
            writer
        };

        if let Err(e) = writer.commit() {
            warn!("Commit failed: {e}");
            self.start_new_segment()?;
            Err(e)
        } else {
            Ok(sz)
        }
    }

    pub fn sync(&self) -> io::Result<()> {
        self.head.fsync()
    }

    /// The last transaction offset written to disk, or `None` if nothing has
    /// been written yet.
    ///
    /// Note that this does not imply durability: [`Self::sync`] may not have
    /// been called at this offset.
    pub fn max_committed_offset(&self) -> Option<u64> {
        // Naming is hard: the segment's `next_tx_offset` indicates how many
        // txs are already in the log (it's the next commit's min-tx-offset).
        // If the value is zero, however, the initial commit hasn't been
        // committed yet.
        self.head.next_tx_offset().checked_sub(1)
    }

    // Helper to obtain a list of the segment offsets which include transaction
    // offset `offset`.
    //
    // The returned `Vec` is sorted in **ascending** order, such that the first
    // element is the segment which contains `offset`.
    //
    // The offset of `self.head` is always included, regardless of how many
    // entries it actually contains.
    fn segment_offsets_from(&self, offset: u64) -> Vec<u64> {
        if offset >= self.head.min_tx_offset {
            vec![self.head.min_tx_offset]
        } else {
            let mut offs = Vec::with_capacity(self.tail.len() + 1);
            if let Some(pos) = self.tail.iter().rposition(|off| off <= &offset) {
                offs.extend_from_slice(&self.tail[pos..]);
                offs.push(self.head.min_tx_offset);
            }

            offs
        }
    }

    pub fn commits_from(&self, offset: u64) -> Commits<R> {
        let offsets = self.segment_offsets_from(offset);
        let last_offset = offsets.first().cloned().unwrap_or(offset);
        let segments = Segments {
            offs: offsets.into_iter(),
            repo: self.repo.clone(),
            max_log_format_version: self.opts.log_format_version,
        };
        Commits {
            inner: None,
            segments,
            last_offset,
            last_error: None,
        }
    }

    pub fn reset(mut self) -> io::Result<Self> {
        info!("hard reset");

        self.tail.reserve(1);
        self.tail.push(self.head.min_tx_offset);
        for segment in self.tail.iter().rev() {
            self.repo.remove_segment(*segment)?;
        }

        Self::open(self.repo.clone(), self.opts)
    }

    pub fn reset_to(mut self, offset: u64) -> io::Result<Self> {
        info!("reset to {offset}");

        self.tail.reserve(1);
        self.tail.push(self.head.min_tx_offset);
        for segment in self.tail.iter().rev() {
            let segment = *segment;
            if segment > offset {
                // Segment is outside the offset, so remove it wholesale.
                self.repo.remove_segment(segment)?;
            } else {
                // Read commit-wise until we find the byte offset.
                let reader = repo::open_segment_reader(&self.repo, self.opts.log_format_version, segment)?;
                let commits = reader.commits();

                let mut bytes_read = 0;
                let mut commits_read = 0;
                for commit in commits {
                    let commit = commit?;
                    commits_read += 1;
                    if commit.min_tx_offset > offset {
                        break;
                    }
                    bytes_read += commit.encoded_len() as u64;
                }

                if commits_read == 0 {
                    // Segment is empty, just remove it.
                    self.repo.remove_segment(segment)?;
                } else {
                    let byte_offset = segment::Header::LEN as u64 + bytes_read;
                    self.repo.open_segment(segment)?.ftruncate(byte_offset)?;
                }
            }
        }

        Self::open(self.repo.clone(), self.opts)
    }

    fn start_new_segment(&mut self) -> io::Result<&mut Writer<R::Segment>> {
        debug!(
            "starting new segment offset={} prev-offset={}",
            self.head.next_tx_offset(),
            self.head.min_tx_offset()
        );
        let new = repo::create_segment_writer(&self.repo, self.opts, self.head.next_tx_offset())?;
        let old = mem::replace(&mut self.head, new);
        self.tail.push(old.min_tx_offset());
        self.head.commit = old.commit;

        Ok(&mut self.head)
    }
}

impl<R: Repo, T: Encode> Generic<R, T> {
    pub fn append(&mut self, record: T) -> Result<(), T> {
        self.head.append(record)
    }

    pub fn transactions_from<'a, D>(
        &self,
        offset: u64,
        deserializer: &'a D,
    ) -> impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a
    where
        D: Decoder<Record = T>,
        D::Error: From<error::Traversal>,
        R: 'a,
        T: 'a,
    {
        transactions_from_internal(
            self.commits_from(offset).with_log_format_version(),
            offset,
            deserializer,
        )
    }

    pub fn fold_transactions_from<D>(&self, offset: u64, decoder: D) -> Result<(), D::Error>
    where
        D: Decoder,
        D::Error: From<error::Traversal>,
    {
        fold_transactions_internal(self.commits_from(offset).with_log_format_version(), decoder, offset)
    }
}

impl<R: Repo, T> Drop for Generic<R, T> {
    fn drop(&mut self) {
        if let Err(e) = self.commit() {
            warn!("Failed to commit on drop: {e}");
        }
        if let Err(e) = self.head.fsync() {
            warn!("Failed to fsync on drop: {e}");
        }
    }
}

pub fn commits_from<R: Repo>(repo: R, max_log_format_version: u8, offset: u64) -> io::Result<Commits<R>> {
    let mut offsets = repo.existing_offsets()?;
    if let Some(pos) = offsets.iter().rposition(|&off| off <= offset) {
        offsets = offsets.split_off(pos);
    }
    let last_offset = offsets.first().cloned().unwrap_or(offset);
    let segments = Segments {
        offs: offsets.into_iter(),
        repo,
        max_log_format_version,
    };
    Ok(Commits {
        inner: None,
        segments,
        last_offset,
        last_error: None,
    })
}

pub fn transactions_from<'a, R, D, T>(
    repo: R,
    max_log_format_version: u8,
    offset: u64,
    de: &'a D,
) -> io::Result<impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a>
where
    R: Repo + 'a,
    D: Decoder<Record = T>,
    D::Error: From<error::Traversal>,
    T: 'a,
{
    commits_from(repo, max_log_format_version, offset)
        .map(|commits| transactions_from_internal(commits.with_log_format_version(), offset, de))
}

pub fn fold_transactions_from<R, D>(repo: R, max_log_format_version: u8, offset: u64, de: D) -> Result<(), D::Error>
where
    R: Repo,
    D: Decoder,
    D::Error: From<error::Traversal> + From<io::Error>,
{
    let commits = commits_from(repo, max_log_format_version, offset)?;
    fold_transactions_internal(commits.with_log_format_version(), de, offset)
}

fn transactions_from_internal<'a, R, D, T>(
    commits: CommitsWithVersion<R>,
    offset: u64,
    de: &'a D,
) -> impl Iterator<Item = Result<Transaction<T>, D::Error>> + 'a
where
    R: Repo + 'a,
    D: Decoder<Record = T>,
    D::Error: From<error::Traversal>,
    T: 'a,
{
    commits
        .map(|x| x.map_err(Into::into))
        .map_ok(move |(version, commit)| commit.into_transactions(version, de))
        .flatten_ok()
        .flatten_ok()
        .skip_while(move |x| x.as_ref().map(|tx| tx.offset < offset).unwrap_or(false))
}

fn fold_transactions_internal<R, D>(mut commits: CommitsWithVersion<R>, de: D, from: u64) -> Result<(), D::Error>
where
    R: Repo,
    D: Decoder,
    D::Error: From<error::Traversal>,
{
    while let Some(commit) = commits.next() {
        let (version, commit) = match commit {
            Ok(version_and_commit) => version_and_commit,
            Err(e) => {
                // Ignore it if the very last commit in the log is broken.
                // The next `append` will fix the log, but the `decoder`
                // has no way to tell whether we're at the end or not.
                // This is unlike the consumer of an iterator, which can
                // perform below check itself.
                if commits.next().is_none() {
                    return Ok(());
                }

                return Err(e.into());
            }
        };
        trace!("commit {} n={} version={}", commit.min_tx_offset, commit.n, version);

        let max_tx_offset = commit.min_tx_offset + commit.n as u64;
        if max_tx_offset <= from {
            continue;
        }

        let records = &mut commit.records.as_slice();
        for n in 0..commit.n {
            let tx_offset = commit.min_tx_offset + n as u64;
            if tx_offset < from {
                // TODO(perf): replace with `de.skip_record`, after implementing that.
                de.decode_record(version, tx_offset, records)?;
            } else {
                de.consume_record(version, tx_offset, records)?;
            }
        }
    }

    Ok(())
}

pub struct Segments<R> {
    repo: R,
    offs: vec::IntoIter<u64>,
    max_log_format_version: u8,
}

impl<R: Repo> Iterator for Segments<R> {
    type Item = io::Result<segment::Reader<R::Segment>>;

    fn next(&mut self) -> Option<Self::Item> {
        let off = self.offs.next()?;
        debug!("iter segment {off}");
        Some(repo::open_segment_reader(&self.repo, self.max_log_format_version, off))
    }
}

pub struct Commits<R: Repo> {
    inner: Option<segment::Commits<R::Segment>>,
    segments: Segments<R>,
    last_offset: u64,
    last_error: Option<error::Traversal>,
}

impl<R: Repo> Commits<R> {
    fn current_segment_header(&self) -> Option<&segment::Header> {
        self.inner.as_ref().map(|segment| &segment.header)
    }

    /// Turn `self` into an iterator which pairs the log format version of the
    /// current segment with the [`Commit`].
    pub fn with_log_format_version(self) -> CommitsWithVersion<R> {
        CommitsWithVersion { inner: self }
    }
}

impl<R: Repo> Iterator for Commits<R> {
    type Item = Result<Commit, error::Traversal>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(commits) = self.inner.as_mut() {
            if let Some(commit) = commits.next() {
                match commit {
                    Ok(commit) => {
                        let prev_error = self.last_error.take();
                        if commit.min_tx_offset != self.last_offset {
                            warn!("out-of-order: commit={:?} last-error={:?}", commit, self.last_error);
                            return Some(Err(error::Traversal::OutOfOrder {
                                expected_offset: self.last_offset,
                                actual_offset: commit.min_tx_offset,
                                prev_error: prev_error.map(Box::new),
                            }));
                        }
                        self.last_offset = commit.tx_range().end;
                        return Some(Ok(commit));
                    }
                    Err(e) => {
                        warn!("error reading next commit: {e}");
                        // Fall through to peek at next segment.
                        //
                        // If this is just a partial write at the end of a
                        // segment, we may be able to obtain a commit with the
                        // right offset from the next segment.
                        //
                        // However, the error here may be more helpful and would
                        // be clobbered by `OutOfOrder`, and so we store it
                        // until we recurse below.
                        let last_error = if e.kind() == io::ErrorKind::InvalidData && e.get_ref().is_some() {
                            e.into_inner()
                                .unwrap()
                                .downcast::<error::ChecksumMismatch>()
                                .map(|source| error::Traversal::Checksum {
                                    offset: self.last_offset,
                                    source: *source,
                                })
                                .unwrap_or_else(|e| io::Error::new(io::ErrorKind::InvalidData, e).into())
                        } else {
                            error::Traversal::from(e)
                        };
                        self.last_error = Some(last_error);
                    }
                }
            }
        }

        match self.segments.next() {
            // When there is no more data, the last commit being bad is an error
            None => self.last_error.take().map(Err),
            Some(segment) => segment.map_or_else(
                |e| Some(Err(e.into())),
                |segment| {
                    self.inner = Some(segment.commits());
                    self.next()
                },
            ),
        }
    }
}

pub struct CommitsWithVersion<R: Repo> {
    inner: Commits<R>,
}

impl<R: Repo> Iterator for CommitsWithVersion<R> {
    type Item = Result<(u8, Commit), error::Traversal>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next()?;
        match next {
            Ok(commit) => {
                let version = self
                    .inner
                    .current_segment_header()
                    .map(|hdr| hdr.log_format_version)
                    .expect("segment header none even though segment yielded a commit");
                Some(Ok((version, commit)))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::iter::repeat;

    use super::*;
    use crate::{
        payload::ArrayDecoder,
        tests::helpers::{fill_log, mem_log},
    };

    #[test]
    fn rotate_segments_simple() {
        let mut log = mem_log::<[u8; 32]>(128);
        for _ in 0..3 {
            log.append([0; 32]).unwrap();
            log.commit().unwrap();
        }

        let offsets = log.repo.existing_offsets().unwrap();
        assert_eq!(&offsets[..offsets.len() - 1], &log.tail);
        assert_eq!(offsets[offsets.len() - 1], 2);
    }

    #[test]
    fn huge_commit() {
        let mut log = mem_log::<[u8; 32]>(32);

        log.append([0; 32]).unwrap();
        log.append([1; 32]).unwrap();
        log.commit().unwrap();
        assert!(log.head.len() > log.opts.max_segment_size);

        log.append([2; 32]).unwrap();
        log.commit().unwrap();

        assert_eq!(&log.tail, &[0]);
        assert_eq!(&log.repo.existing_offsets().unwrap(), &[0, 2]);
    }

    #[test]
    fn traverse_commits() {
        let mut log = mem_log::<[u8; 32]>(32);
        fill_log(&mut log, 10, repeat(1));

        for (i, commit) in (0..10).zip(log.commits_from(0)) {
            assert_eq!(i, commit.unwrap().min_tx_offset);
        }
    }

    #[test]
    fn traverse_commits_with_offset() {
        let mut log = mem_log::<[u8; 32]>(32);
        fill_log(&mut log, 10, repeat(1));

        for offset in 0..10 {
            for commit in log.commits_from(offset) {
                let commit = commit.unwrap();
                assert!(commit.min_tx_offset >= offset);
            }
        }
        // Nb.: the head commit is always returned,
        // because we don't know its offset upper bound
        assert_eq!(1, log.commits_from(10).count());
    }

    #[test]
    fn traverse_transactions() {
        let mut log = mem_log::<[u8; 32]>(32);
        let total_txs = fill_log(&mut log, 10, (1..=3).cycle()) as u64;

        for (i, tx) in (0..total_txs).zip(log.transactions_from(0, &ArrayDecoder)) {
            assert_eq!(i, tx.unwrap().offset);
        }
    }

    #[test]
    fn traverse_transactions_with_offset() {
        let mut log = mem_log::<[u8; 32]>(32);
        let total_txs = fill_log(&mut log, 10, (1..=3).cycle()) as u64;

        for offset in 0..total_txs {
            let mut iter = log.transactions_from(offset, &ArrayDecoder);
            assert_eq!(offset, iter.next().expect("at least one tx expected").unwrap().offset);
            for tx in iter {
                assert!(tx.unwrap().offset >= offset);
            }
        }
        assert_eq!(0, log.transactions_from(total_txs, &ArrayDecoder).count());
    }

    #[test]
    fn traverse_empty() {
        let log = mem_log::<[u8; 32]>(32);

        assert_eq!(0, log.commits_from(0).count());
        assert_eq!(0, log.commits_from(42).count());
        assert_eq!(0, log.transactions_from(0, &ArrayDecoder).count());
        assert_eq!(0, log.transactions_from(42, &ArrayDecoder).count());
    }

    #[test]
    fn reset_hard() {
        let mut log = mem_log::<[u8; 32]>(128);
        fill_log(&mut log, 50, (1..=10).cycle());

        log = log.reset().unwrap();
        assert_eq!(0, log.transactions_from(0, &ArrayDecoder).count());
    }

    #[test]
    fn reset_to_offset() {
        let mut log = mem_log::<[u8; 32]>(128);
        let total_txs = fill_log(&mut log, 50, repeat(1)) as u64;

        for offset in (0..total_txs).rev() {
            log = log.reset_to(offset).unwrap();
            assert_eq!(
                offset,
                log.transactions_from(0, &ArrayDecoder)
                    .map(Result::unwrap)
                    .last()
                    .unwrap()
                    .offset
            );
            // We're counting from zero, so offset + 1 is the # of txs.
            assert_eq!(
                offset + 1,
                log.transactions_from(0, &ArrayDecoder).map(Result::unwrap).count() as u64
            );
        }
    }

    #[test]
    fn reset_to_offset_many_txs_per_commit() {
        let mut log = mem_log::<[u8; 32]>(128);
        let total_txs = fill_log(&mut log, 50, (1..=10).cycle()) as u64;

        // No op.
        log = log.reset_to(total_txs).unwrap();
        assert_eq!(total_txs, log.transactions_from(0, &ArrayDecoder).count() as u64);

        let middle_commit = log.commits_from(0).nth(25).unwrap().unwrap();

        // Both fall into the middle commit, which should be retained.
        log = log.reset_to(middle_commit.min_tx_offset + 1).unwrap();
        assert_eq!(
            middle_commit.tx_range().end,
            log.transactions_from(0, &ArrayDecoder).count() as u64
        );
        log = log.reset_to(middle_commit.min_tx_offset).unwrap();
        assert_eq!(
            middle_commit.tx_range().end,
            log.transactions_from(0, &ArrayDecoder).count() as u64
        );

        // Offset falls into 2nd commit.
        // 1st commit (1 tx) + 2nd commit (2 txs) = 3
        log = log.reset_to(1).unwrap();
        assert_eq!(3, log.transactions_from(0, &ArrayDecoder).count() as u64);

        // Offset falls into 1st commit.
        // 1st commit (1 tx) = 1
        log = log.reset_to(0).unwrap();
        assert_eq!(1, log.transactions_from(0, &ArrayDecoder).count() as u64);
    }

    #[test]
    fn reopen() {
        let mut log = mem_log::<[u8; 32]>(1024);
        let mut total_txs = fill_log(&mut log, 100, (1..=10).cycle());
        assert_eq!(
            total_txs,
            log.transactions_from(0, &ArrayDecoder).map(Result::unwrap).count()
        );

        let mut log = Generic::<_, [u8; 32]>::open(
            log.repo.clone(),
            Options {
                max_segment_size: 1024,
                ..Options::default()
            },
        )
        .unwrap();
        total_txs += fill_log(&mut log, 100, (1..=10).cycle());

        assert_eq!(
            total_txs,
            log.transactions_from(0, &ArrayDecoder).map(Result::unwrap).count()
        );
    }
}
