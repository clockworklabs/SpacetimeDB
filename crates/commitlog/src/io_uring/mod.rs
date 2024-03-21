use std::{io, marker::PhantomData, mem, path::PathBuf};

use async_stream::{stream, try_stream};
use futures::Stream;
use log::{debug, warn};
use tokio::sync::RwLock;

use crate::{
    error,
    repo::{fs::segment_file_name, Repo as _},
    Commit, Decoder, Encode, Options, Transaction,
};

mod segment;

pub struct Commitlog<T> {
    inner: RwLock<Inner<T>>,
}

impl<T> Commitlog<T> {
    pub async fn open(root: impl Into<PathBuf>, opts: Options) -> io::Result<Self> {
        Inner::open(root, opts)
            .await
            .map(RwLock::new)
            .map(|inner| Self { inner })
    }

    pub async fn sync(&self) -> Option<u64> {
        let inner = self.inner.write().await;
        inner.sync().await;
        inner.max_committed_offset()
    }

    pub async fn flush(&self) -> io::Result<Option<u64>> {
        let mut inner = self.inner.write().await;
        inner.commit().await?;
        Ok(inner.max_committed_offset())
    }

    pub async fn flush_and_sync(&self) -> io::Result<Option<u64>> {
        let mut inner = self.inner.write().await;
        inner.commit().await?;
        inner.sync().await;

        Ok(inner.max_committed_offset())
    }

    pub async fn close(self) -> io::Result<()> {
        let inner = self.inner.into_inner();
        inner.close().await
    }
}

impl<T: Encode> Commitlog<T> {
    pub async fn append(&self, txdata: T) -> Result<(), T> {
        let mut inner = self.inner.write().await;
        inner.append(txdata)
    }

    pub async fn append_maybe_flush(&self, txdata: T) -> Result<(), error::Append<T>> {
        let mut inner = self.inner.write().await;

        if let Err(txdata) = inner.append(txdata) {
            if let Err(source) = inner.commit().await {
                return Err(error::Append { txdata, source });
            }
            // `inner.commit.n` must be zero at this point
            let res = inner.append(txdata);
            debug_assert!(res.is_ok(), "failed to append while holding write lock");
        }

        Ok(())
    }

    pub fn transactions_from<'a, D>(
        &'a self,
        offset: u64,
        de: &'a D,
    ) -> impl Stream<Item = Result<Transaction<T>, D::Error>> + 'a
    where
        D: Decoder<Record = T>,
        D::Error: From<error::Traversal>,
        T: 'a,
    {
        stream! {
            let inner = self.inner.read().await;
            for await tx in inner.transactions_from(offset, de) {
                yield tx
            }
        }
    }
}

struct Inner<T> {
    root: PathBuf,
    head: segment::Writer,
    tail: Vec<u64>,
    opts: Options,
    _record: PhantomData<T>,
}

impl<T> Inner<T> {
    pub async fn open(root: impl Into<PathBuf>, opts: Options) -> io::Result<Self> {
        let root: PathBuf = root.into();
        let mut tail = crate::repo::Fs::new(&root).existing_offsets()?;
        if !tail.is_empty() {
            debug!("segments: {tail:?}");
        }
        let head = if let Some(last) = tail.pop() {
            debug!("resuming last segment: {last}");
            match segment::Writer::resume(&root, opts, last).await? {
                Ok(writer) => writer,
                Err(meta) => {
                    tail.push(meta.tx_range.start);
                    segment::Writer::create(&root, opts, meta.tx_range.end).await?
                }
            }
        } else {
            debug!("starting fresh log");
            segment::Writer::create(&root, opts, 0).await?
        };

        Ok(Self {
            root,
            head,
            tail,
            opts,
            _record: PhantomData,
        })
    }

    pub async fn sync(&self) {
        if let Err(e) = self.head.fsync().await {
            panic!("Failed to fsync: {e}");
        }
    }

    pub async fn close(self) -> io::Result<()> {
        self.head.close().await
    }

    pub async fn commit(&mut self) -> io::Result<usize> {
        let writer = &mut self.head;
        let sz = writer.commit.encoded_len();

        let should_rotate = !writer.is_empty() && writer.len() + sz as u64 > self.opts.max_segment_size;
        let writer = if should_rotate {
            debug!("starting new segment");
            self.sync().await;
            self.start_new_segment().await?
        } else {
            writer
        };

        if let Err(e) = writer.commit().await {
            warn!("Commit failed: {e}");
            self.sync().await;
            self.start_new_segment().await?;
            Err(e)
        } else {
            Ok(sz)
        }
    }

    pub fn max_committed_offset(&self) -> Option<u64> {
        self.head.next_tx_offset().checked_sub(1)
    }

    async fn start_new_segment(&mut self) -> io::Result<&mut segment::Writer> {
        let new = segment::Writer::create(&self.root, self.opts, self.head.next_tx_offset()).await?;
        let mut old = mem::replace(&mut self.head, new);
        let offset = old.min_tx_offset();
        let commit = mem::take(&mut old.commit);
        old.close().await?;
        debug!("closed segment: {}", offset);
        self.tail.push(offset);
        self.head.commit = commit;

        Ok(&mut self.head)
    }

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
}

impl<T: Encode> Inner<T> {
    pub fn append(&mut self, record: T) -> Result<(), T> {
        self.head.append(record)
    }

    pub fn transactions_from<'a, D>(
        &'a self,
        offset: u64,
        decoder: &'a D,
    ) -> impl Stream<Item = Result<Transaction<T>, D::Error>> + 'a
    where
        D: Decoder<Record = T>,
        D::Error: From<error::Traversal> + 'a,
    {
        let offsets = self.segment_offsets_from(offset);
        try_stream! {
            for offset in offsets {
                let segment = segment::Reader::new(0, offset, self.root.join(segment_file_name(offset)))
                    .await
                    .map_err(error::Traversal::from)?;
                debug!("opened segment {offset}");
                let commits = segment.commits();

                for await commit in commits {
                    let commit = commit.map_err(error::Traversal::from)?;
                    debug!("read commit: {}", commit.min_tx_offset);
                    for tx in Commit::from(commit).into_transactions(0, decoder) {
                        let tx = tx?;
                        yield tx
                    }
                }
            }
        }
    }
}
