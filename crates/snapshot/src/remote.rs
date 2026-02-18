use std::{
    future::Future,
    io, mem,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use crossbeam_queue::ArrayQueue;
use futures::{stream, StreamExt as _, TryStreamExt as _};
use scopeguard::ScopeGuard;
use spacetimedb_fs_utils::{compression::ZSTD_MAGIC_BYTES, dir_trie::DirTrie, lockfile::Lockfile};
use spacetimedb_lib::bsatn;
use spacetimedb_paths::server::{SnapshotDirPath, SnapshotsPath};
use spacetimedb_sats::buffer::BufWriter as SatsWriter;
use spacetimedb_table::{blob_store::BlobHash, page::Page, page_pool::PagePool};
use tempfile::NamedTempFile;
use tokio::{
    fs,
    io::{AsyncBufRead, AsyncBufReadExt as _, AsyncWrite, AsyncWriteExt as _, BufReader, BufWriter},
    sync::mpsc,
    task::spawn_blocking,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_util::io::{InspectReader, InspectWriter, StreamReader};
use zstd_framed::AsyncZstdReader;

use crate::{ObjectType, Snapshot, SnapshotError, SnapshotRepository};

pub type Result<T> = std::result::Result<T, SnapshotError>;

/// A source of snapshot objects that can be obtained by `hash`.
pub trait BlobProvider: Send {
    fn blob_reader(
        &self,
        hash: blake3::Hash,
    ) -> impl Future<Output = io::Result<impl AsyncBufRead + Send + Unpin>> + Send;
}

impl BlobProvider for DirTrie {
    async fn blob_reader(&self, hash: blake3::Hash) -> io::Result<impl AsyncBufRead + Send + Unpin> {
        fs::File::open(self.file_path(hash.as_bytes()))
            .await
            .map(BufReader::new)
    }
}

impl<F, Fut, R> BlobProvider for F
where
    F: Fn(blake3::Hash) -> Fut + Send + Sync,
    Fut: Future<Output = io::Result<R>> + Send,
    R: AsyncBufRead + Send + Unpin,
{
    fn blob_reader(
        &self,
        hash: blake3::Hash,
    ) -> impl Future<Output = io::Result<impl AsyncBufRead + Send + Unpin>> + Send {
        (self)(hash)
    }
}

impl<T: BlobProvider + Send + Sync> BlobProvider for Arc<T> {
    fn blob_reader(
        &self,
        hash: blake3::Hash,
    ) -> impl Future<Output = io::Result<impl AsyncBufRead + Send + Unpin>> + Send {
        (**self).blob_reader(hash)
    }
}

/// Counters tracking how [`synchronize_snapshot`] handled objects.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    /// Number of new objects written to disk.
    pub objects_written: u64,
    /// Number of objects hardlinked to the previous snapshot.
    pub objects_hardlinked: u64,
    /// Number of objects skipped due to them already existing in the snapshot's
    /// object repository.
    pub objects_skipped: u64,
}

impl From<StatsInner> for Stats {
    fn from(inner: StatsInner) -> Self {
        Self {
            objects_written: inner.objects_written.load(Ordering::Relaxed),
            objects_hardlinked: inner.objects_hardlinked.load(Ordering::Relaxed),
            objects_skipped: inner.objects_skipped.load(Ordering::Relaxed),
        }
    }
}

/// Given [`Snapshot`] metadata, obtained separately, fetches all objects
/// referenced from it from a remote source `provider`, and stores them in the
/// local `snapshots_dir`.
///
/// The function tries to avoid work where possible. Namely:
///
/// - If there is a parent snapshot locally and an object can be found in its
///   object store, the object is hardlinked to the existing object instead of
///   being fetched.
/// - If an object referenced from `snapshot` can be found in the object store
///   of the corresponding local snapshot, the object is not fetched.
///   **NOTE** that the hash of the existing local object is not verified.
///
/// It will, however, proceed if the snapshot file already exists at the target
/// path and hashes to the same value as the given `snapshot`. This can be
/// useful to "repair" snapshots transferred by other methods.
///
/// Fetched objects are verified against the hashes from the `snapshot`, before
/// being moved into place in the object store.
///
/// If successful, the `snapshot` is written to the designated location in the
/// `snapshots_dir`.
///
/// # Cancellation
///
/// The function is **not** cancel safe in the same way as [`spawn_blocking`]
/// (which it makes use of internally) is not cancel safe.
///
/// It is, however, safe to retry a failed [`synchronize_snapshot`] run.
pub async fn synchronize_snapshot(
    provider: impl BlobProvider + 'static,
    snapshots_dir: SnapshotsPath,
    snapshot: Snapshot,
) -> Result<Stats> {
    run_fetcher(provider, snapshots_dir, snapshot, false).await
}

/// Verifies the integrity of the objects referenced from [`Snapshot`],
/// in constant memory.
///
/// Like [`synchronize_snapshot`], but doesn't modify the local storage.
/// Usually, a local [`BlobProvider`] like [`DirTrie`] should be provided.
pub async fn verify_snapshot(
    provider: impl BlobProvider + 'static,
    snapshots_dir: SnapshotsPath,
    snapshot: Snapshot,
) -> Result<()> {
    run_fetcher(provider, snapshots_dir, snapshot, true).await.map(drop)
}

async fn run_fetcher(
    provider: impl BlobProvider + 'static,
    snapshots_dir: SnapshotsPath,
    snapshot: Snapshot,
    dry_run: bool,
) -> Result<Stats> {
    spawn_blocking(|| {
        SnapshotFetcher::create(
            provider,
            snapshots_dir,
            snapshot,
            PagePool::new(Some(PAGE_POOL_SIZE)),
            BufPool::new(BUF_POOL_SIZE),
        )
    })
    .await
    .unwrap()?
    .run(dry_run)
    .await
}

#[derive(Default)]
struct StatsInner {
    objects_written: AtomicU64,
    objects_hardlinked: AtomicU64,
    objects_skipped: AtomicU64,
}

impl StatsInner {
    fn wrote_object(&self) {
        Self::inc(&self.objects_written);
    }

    fn hardlinked_object(&self) {
        Self::inc(&self.objects_hardlinked);
    }

    fn skipped_object(&self) {
        Self::inc(&self.objects_skipped)
    }

    fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Limits the number of futures that concurrently fetch and process objects.
///
/// Note that this applies to blobs and pages separately, so the total
/// concurrency limit is `2*FETCH_CONCURRENCY`.
const FETCH_CONCURRENCY: usize = 8;
/// Size of a [`Page`], in bytes.
const PAGE_SIZE: usize = size_of::<Page>(); // 64 KiB
/// Max size of the [`PagePool`], in bytes.
///
/// We only ever retain at most `FETCH_CONCURRENCY` pages in memory at the same
/// time, thus the required size of the pool is `FETCH_CONCURRENCY * PAGE_SIZE`.
const PAGE_POOL_SIZE: usize = FETCH_CONCURRENCY * PAGE_SIZE;
/// Max size of the [`BufPool`], in number of buffers.
///
/// We use the pooled buffers to:
///
/// - hold raw, decompressed page data
/// - "tee" compressed blob data to a hasher task
/// - "tee" compressed page data to a decompressor task
///
/// Therefore, the maximum size we need is `3 * FETCH_CONCURRENCY`.
const BUF_POOL_SIZE: usize = 3 * FETCH_CONCURRENCY;

/// Creates a [`PagePool`] suitable for a one-off [`SnapshotFetcher::run`].
///
/// When many fetchers are active in parallel, sharing a larger pool between
/// them is likely beneficial.
pub fn default_page_pool() -> PagePool {
    PagePool::new(Some(PAGE_POOL_SIZE))
}

pub struct SnapshotFetcher<P> {
    snapshot: Snapshot,
    dir: SnapshotDirPath,
    object_repo: Arc<DirTrie>,
    parent_repo: Option<Arc<DirTrie>>,
    provider: P,

    /// Re-usable memory for deserialized pages.
    page_pool: PagePool,
    /// Re-usable memory for raw (un-deserialized) pages.
    buf_pool: BufPool,

    dry_run: bool,
    stats: StatsInner,

    // NOTE: This should remain the last declared field,
    // so that the lock file is dropped last when `self` is dropped.
    #[allow(unused)]
    lock: Lockfile,
}

impl<P: BlobProvider> SnapshotFetcher<P> {
    pub fn create(
        provider: P,
        snapshots_dir: SnapshotsPath,
        snapshot: Snapshot,
        page_pool: PagePool,
        buf_pool: BufPool,
    ) -> Result<Self> {
        let snapshot_repo = SnapshotRepository::open(snapshots_dir, snapshot.database_identity, snapshot.replica_id)?;
        let snapshot_dir = snapshot_repo.snapshot_dir_path(snapshot.tx_offset);
        let lock = Lockfile::for_file(&snapshot_dir)?;
        std::fs::create_dir_all(&snapshot_dir)?;

        let object_repo = SnapshotRepository::object_repo(&snapshot_dir)?;
        let parent_offset = snapshot_repo.latest_snapshot_older_than(snapshot.tx_offset)?;
        // The parent offset must always be smaller than `snapshot`'s offset,
        // because we locked `snapshot_dir`, so this snapshot is not selected.
        debug_assert!(
            parent_offset.is_none() || parent_offset.is_some_and(|offset| offset < snapshot.tx_offset),
            "invalid parent offset"
        );
        let parent_repo = parent_offset
            .map(|offset| {
                let path = snapshot_repo.snapshot_dir_path(offset);
                SnapshotRepository::object_repo(&path)
            })
            .transpose()?;

        Ok(Self {
            snapshot,
            dir: snapshot_dir,
            object_repo: Arc::new(object_repo),
            parent_repo: parent_repo.map(Arc::new),
            provider,
            page_pool,
            buf_pool,
            dry_run: false,
            stats: <_>::default(),
            lock,
        })
    }

    /// Run the snapshot fetcher, returning [`Stats`] of what it did.
    ///
    /// If `dry_run` is `true`, no modifications will be made to the object
    /// repository. This is useful for verifying the integrity of a snapshot.
    pub async fn run(&mut self, dry_run: bool) -> Result<Stats> {
        self.dry_run = dry_run;

        let snapshot_bsatn = {
            let mut buf = Vec::new();
            serialize_snapshot(&mut buf, &self.snapshot)?;
            buf
        };
        let snapshot_hash = blake3::hash(&snapshot_bsatn);
        let snapshot_file_path = self.dir.snapshot_file(self.snapshot.tx_offset);
        // If the snapshot file already exists at the target path,
        // check that it is valid and that it hashes to `snapshot_hash`.
        if fs::try_exists(&snapshot_file_path).await? {
            let (existing, _) = spawn_blocking({
                let snapshot_file_path = snapshot_file_path.clone();
                move || Snapshot::read_from_file(&snapshot_file_path)
            })
            .await
            .unwrap()?;
            let existing_hash = {
                let mut hasher = Hasher::default();
                serialize_snapshot(&mut hasher, &existing)?;
                hasher.hash()
            };

            if existing_hash != snapshot_hash {
                return Err(SnapshotError::HashMismatch {
                    ty: ObjectType::Snapshot,
                    expected: *snapshot_hash.as_bytes(),
                    computed: *existing_hash.as_bytes(),
                    source_repo: snapshot_file_path.0.clone(),
                });
            }
        }

        // Get all the objects.
        tokio::try_join!(self.fetch_blobs(), self.fetch_pages())?;

        // Success. Write out the snapshot file.
        atomically((!self.dry_run).then_some(snapshot_file_path.0), |out| async {
            let mut out = BufWriter::new(out);
            out.write_all(snapshot_hash.as_bytes()).await?;
            out.write_all(&snapshot_bsatn).await?;
            out.flush().await?;

            Ok(out.into_inner())
        })
        .await?;

        Ok(mem::take(&mut self.stats).into())
    }

    async fn fetch_blobs(&self) -> Result<()> {
        let tasks = self
            .snapshot
            .blobs
            .iter()
            .map(|entry| {
                let hash = blake3::Hash::from_bytes(entry.hash.data);
                self.fetch_blob(hash)
            })
            .collect::<Box<[_]>>();
        stream::iter(tasks)
            .map(Ok)
            .try_for_each_concurrent(FETCH_CONCURRENCY, |task| task)
            .await
    }

    async fn fetch_pages(&self) -> Result<()> {
        let tasks = self
            .snapshot
            .tables
            .iter()
            .flat_map(|entry| entry.pages.iter().copied().map(|hash| self.fetch_page(hash)))
            .collect::<Box<[_]>>();
        stream::iter(tasks)
            .map(Ok)
            .try_for_each_concurrent(FETCH_CONCURRENCY, |task| task)
            .await
    }

    async fn fetch_blob(&self, hash: blake3::Hash) -> Result<()> {
        let Some(dst_path) = self
            .object_file_path(ObjectType::Blob(BlobHash { data: *hash.as_bytes() }))
            .await?
        else {
            return Ok(());
        };
        atomically((!self.dry_run).then_some(dst_path), |out| async move {
            let mut out = BufWriter::new(out);
            let mut src = self.provider.blob_reader(hash).await?;
            let compressed = src.fill_buf().await?.starts_with(&ZSTD_MAGIC_BYTES);

            // Consume the blob reader,
            // write its contents to `out`,
            // and compute the content hash on the fly.
            let mut hasher = Hasher::default();
            let computed_hash = if !compressed {
                // If the input is uncompressed, just update the hasher as we go.
                let mut writer = InspectWriter::new(&mut out, |chunk| {
                    hasher.update(chunk);
                });
                tokio::io::copy_buf(&mut src, &mut writer).await?;
                writer.flush().await?;

                hasher.hash()
            } else {
                // If the input is compressed, send a copy of all received
                // chunks to a separate task that decompresses the stream and
                // computes the hash from the decompressed bytes.
                let (mut zstd, tx) = zstd_reader()?;
                let decompressor = tokio::spawn(async move {
                    tokio::io::copy_buf(&mut zstd, &mut hasher).await?;
                    Ok::<_, io::Error>(hasher.hash())
                });

                let mut buf = self.buf_pool.get();
                let mut src = InspectReader::new(src, |chunk| {
                    buf.extend_from_slice(chunk);
                    tx.send(buf.split().freeze()).ok();
                });
                tokio::io::copy(&mut src, &mut out).await?;
                out.flush().await?;

                drop(tx);
                decompressor.await.unwrap()?
            };
            if computed_hash != hash {
                return Err(SnapshotError::HashMismatch {
                    ty: ObjectType::Blob(BlobHash { data: *hash.as_bytes() }),
                    expected: *hash.as_bytes(),
                    computed: *computed_hash.as_bytes(),
                    source_repo: self.dir.0.clone(),
                });
            }

            Ok(out.into_inner())
        })
        .await
        .inspect(|()| {
            self.stats.wrote_object();
        })
    }

    async fn fetch_page(&self, hash: blake3::Hash) -> Result<()> {
        let Some(dst_path) = self.object_file_path(ObjectType::Page(hash)).await? else {
            return Ok(());
        };
        atomically((!self.dry_run).then_some(dst_path), |out| async {
            let mut out = BufWriter::new(out);
            let mut src = self.provider.blob_reader(hash).await?;
            let compressed = src.fill_buf().await?.starts_with(&ZSTD_MAGIC_BYTES);

            // To compute the page hash, we need to bsatn deserialize it.
            // As bsatn doesn't support streaming deserialization yet,
            // we need to keep a copy of the input bytes,
            // while also writing them to `out`.
            let page_bytes = if !compressed {
                // If the input is uncompressed, just copy all bytes to a buffer.
                let mut page_buf = self.buf_pool.get();
                let mut writer = InspectWriter::new(&mut out, |chunk| {
                    page_buf.extend_from_slice(chunk);
                });
                tokio::io::copy_buf(&mut src, &mut writer).await?;
                writer.flush().await?;

                page_buf.split().freeze()
            } else {
                // If the input is compressed, send all received chunks to a
                // separate task that decompresses the stream and returns
                // the uncompressed bytes.
                let (mut zstd, tx) = zstd_reader()?;
                let buf_pool = self.buf_pool.clone();
                let decompressor = tokio::spawn(async move {
                    let mut page_buf = buf_pool.get();
                    tokio::io::copy_buf(&mut zstd, &mut AsyncBufWriter(&mut page_buf)).await?;
                    Ok::<_, io::Error>(page_buf.split().freeze())
                });

                let mut buf = self.buf_pool.get();
                let mut writer = InspectWriter::new(&mut out, |chunk| {
                    buf.extend_from_slice(chunk);
                    tx.send(buf.split().freeze()).ok();
                });
                tokio::io::copy_buf(&mut src, &mut writer).await?;
                writer.flush().await?;

                drop(tx);
                decompressor.await.unwrap()?
            };

            self.verify_page(hash, &page_bytes)?;

            Ok(out.into_inner())
        })
        .await
        .inspect(|()| {
            self.stats.wrote_object();
        })
    }

    /// Get the path of object `hash` in the target object repo.
    ///
    /// Returns `None` if the file already exists, or
    /// we have a parent repo, and the object exists there.
    ///
    /// In the latter case, a hardlink will be created.
    /// `self.stats` is updated in either case.
    ///
    /// In dry-run mode, `Some(path)` is returned
    /// if the file exists in either the target or the parent repo,
    /// in order to force hash verification.
    /// If it does not exist, an error is returned.
    async fn object_file_path(&self, ty: ObjectType) -> Result<Option<PathBuf>> {
        let hash = match ty {
            ObjectType::Blob(hash) => blake3::Hash::from_bytes(hash.data),
            ObjectType::Page(hash) => hash,
            ObjectType::Snapshot => unreachable!("invalid argument"),
        };
        let path = self.object_repo.file_path(hash.as_bytes());
        if fs::try_exists(&path).await? {
            if self.dry_run {
                return Ok(Some(path));
            }

            self.stats.skipped_object();
            return Ok(None);
        }

        if self.try_hardlink(hash).await? {
            if self.dry_run {
                return Ok(Some(path));
            }

            self.stats.hardlinked_object();
            return Ok(None);
        }

        if self.dry_run {
            return Err(SnapshotError::ReadObject {
                ty,
                source_repo: self.object_repo.root().to_owned(),
                cause: io::Error::new(io::ErrorKind::NotFound, format!("missing object {}", path.display())),
            });
        }

        Ok(Some(path))
    }

    async fn try_hardlink(&self, hash: blake3::Hash) -> Result<bool> {
        let Some(parent) = self.parent_repo.as_ref() else {
            return Ok(false);
        };

        let object_repo = Arc::clone(&self.object_repo);
        let parent_repo = Arc::clone(parent);
        if !self.dry_run {
            spawn_blocking(move || object_repo.try_hardlink_from(&parent_repo, hash.as_bytes()))
                .await
                .unwrap()
                .map_err(Into::into)
        } else {
            let src_file = parent_repo.file_path(hash.as_bytes());
            let meta = tokio::fs::metadata(src_file).await?;
            Ok(meta.is_file())
        }
    }

    fn verify_page(&self, expected_hash: blake3::Hash, buf: &[u8]) -> Result<()> {
        let page = self
            .page_pool
            .take_deserialize_from(buf)
            .map_err(|cause| SnapshotError::Deserialize {
                ty: ObjectType::Page(expected_hash),
                source_repo: self.dir.0.clone(),
                cause,
            })?;
        let computed_hash = page.content_hash();
        self.page_pool.put(page);

        if computed_hash != expected_hash {
            return Err(SnapshotError::HashMismatch {
                ty: ObjectType::Blob(BlobHash {
                    data: *expected_hash.as_bytes(),
                }),
                expected: *expected_hash.as_bytes(),
                computed: *computed_hash.as_bytes(),
                source_repo: self.dir.0.clone(),
            });
        }

        Ok(())
    }
}

/// Create an [`AsyncZstdReader`] that incrementally decompresses
/// the data fed to it via the returned [`mpsc::UnboundedSender`].
///
/// The reader implements [`tokio::io::AsyncRead`] and will indicate EOF
/// once the sender is dropped and all remaining data in the channel has been
/// consumed.
fn zstd_reader() -> io::Result<(
    AsyncZstdReader<'static, impl AsyncBufRead>,
    mpsc::UnboundedSender<Bytes>,
)> {
    let (tx, rx) = mpsc::unbounded_channel::<Bytes>();
    let reader = StreamReader::new(UnboundedReceiverStream::new(rx).map(Ok::<_, io::Error>));
    let zstd = AsyncZstdReader::builder_tokio(reader).build()?;

    Ok((zstd, tx))
}

/// Newtype around [`blake3::Hasher`]
/// that implements [`AsyncWrite`] and [`SatsWriter`].
#[derive(Default)]
struct Hasher {
    inner: blake3::Hasher,
}

impl Hasher {
    pub fn hash(&self) -> blake3::Hash {
        self.inner.finalize()
    }

    pub fn update(&mut self, input: &[u8]) {
        self.inner.update(input);
    }
}

impl AsyncWrite for Hasher {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.get_mut().update(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl SatsWriter for Hasher {
    fn put_slice(&mut self, slice: &[u8]) {
        self.update(slice);
    }
}

/// The [`AsyncWrite`] created by [`atomically`].
///
/// Either a temporary file that is being renamed atomically if and when the
/// closure returns successfully,
/// or a [`tokio::io::Sink`] that discards all data written to it (used for
/// [`verify_snapshot`]).
enum AtomicWriter {
    File(fs::File),
    Null(tokio::io::Sink),
}

impl AtomicWriter {
    async fn sync_all(&self) -> io::Result<()> {
        if let Self::File(file) = self {
            file.sync_all().await?;
        }

        Ok(())
    }
}

impl AsyncWrite for AtomicWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::result::Result<usize, io::Error>> {
        match self.get_mut() {
            Self::File(file) => Pin::new(file).poll_write(cx, buf),
            Self::Null(sink) => Pin::new(sink).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), io::Error>> {
        match self.get_mut() {
            Self::File(file) => Pin::new(file).poll_flush(cx),
            Self::Null(sink) => Pin::new(sink).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), io::Error>> {
        match self.get_mut() {
            Self::File(file) => Pin::new(file).poll_shutdown(cx),
            Self::Null(sink) => Pin::new(sink).poll_shutdown(cx),
        }
    }
}

async fn atomically<F, Fut>(file_path: Option<PathBuf>, f: F) -> Result<()>
where
    F: FnOnce(AtomicWriter) -> Fut,
    Fut: Future<Output = Result<AtomicWriter>>,
{
    match file_path {
        Some(file_path) => {
            let dir = file_path.parent().expect("file not in a directory").to_owned();
            fs::create_dir_all(&dir).await?;
            let (tmp_file, tmp_out) = spawn_blocking(move || {
                let tmp = NamedTempFile::new_in(dir)?;
                let out = tmp.reopen()?;
                Ok::<_, io::Error>((tmp, out))
            })
            .await
            .unwrap()?;

            let mut file = AtomicWriter::File(fs::File::from_std(tmp_out));
            file = f(file).await?;
            file.sync_all().await?;

            spawn_blocking(|| tmp_file.persist(file_path))
                .await
                .unwrap()
                .map_err(|e| e.error)?;
        }

        None => {
            f(AtomicWriter::Null(tokio::io::sink())).await?;
        }
    }

    Ok(())
}

fn serialize_snapshot(w: &mut impl SatsWriter, value: &Snapshot) -> Result<()> {
    bsatn::to_writer(w, value).map_err(|cause| SnapshotError::Serialize {
        ty: ObjectType::Snapshot,
        cause,
    })
}

/// Pool of [`BytesMut`] buffers, each with page-sized capacity.
///
/// The [`Default`] impl creates a pool suitable a one-off [`SnapshotFetcher::run`].
/// When many fetchers are active in parallel, sharing a larger pool between
/// them is likely beneficial.
#[derive(Clone)]
pub struct BufPool {
    inner: Arc<ArrayQueue<BytesMut>>,
}

impl Default for BufPool {
    fn default() -> Self {
        Self::new(BUF_POOL_SIZE)
    }
}

impl BufPool {
    /// Creates a new pool capable of holding a maximum of `cap` buffers.
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Arc::new(ArrayQueue::new(cap)),
        }
    }

    /// Get a buffer from the pool, or create a new one.
    ///
    /// The buffer is returned to the pool when the returned [`ScopeGuard`]
    /// goes out of scope.
    pub fn get(&self) -> ScopeGuard<BytesMut, impl FnOnce(BytesMut) + use<>> {
        let this = self.clone();
        scopeguard::guard(
            this.inner.pop().unwrap_or_else(|| BytesMut::with_capacity(PAGE_SIZE)),
            move |buf| this.put(buf),
        )
    }

    /// Put `buf` back into the pool, or drop it if the pool is full.
    pub fn put(&self, buf: BytesMut) {
        let _ = self.inner.push(buf);
    }
}

/// Makes a [`BytesMut`] [`AsyncWrite`].
struct AsyncBufWriter<'a>(&'a mut BytesMut);

impl AsyncWrite for AsyncBufWriter<'_> {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.get_mut().0.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::{default_page_pool, BlobProvider, BufPool, SnapshotFetcher};
    use crate::{Snapshot, SnapshotError, CURRENT_MODULE_ABI_VERSION, CURRENT_SNAPSHOT_VERSION, MAGIC};
    use pretty_assertions::assert_matches;
    use spacetimedb_lib::Identity;
    use spacetimedb_paths::{server::SnapshotsPath, FromPathUnchecked};
    use spacetimedb_sats::bsatn;
    use spacetimedb_sats::layout::{row_size_for_type, Size};
    use spacetimedb_table::{
        blob_store::NullBlobStore, indexes::PageOffset, page::Page, var_len::AlignedVarLenOffsets,
    };
    use std::io::Cursor;
    use tempfile::tempdir;
    use zstd_framed::AsyncZstdWriter;

    const ZEROES: &[u8] = &[0; 32];
    const DUMMY_SNAPSHOT: Snapshot = Snapshot {
        magic: MAGIC,
        version: CURRENT_SNAPSHOT_VERSION,
        database_identity: Identity::ZERO,
        replica_id: 1,
        module_abi_version: CURRENT_MODULE_ABI_VERSION,
        tx_offset: 1_000_001,
        blobs: vec![],
        tables: vec![],
    };

    /// [`BlobProvider`] that serves only zeroes.
    fn zeroes_provider() -> impl BlobProvider {
        |_hash| async { Ok(Box::new(ZEROES)) }
    }

    /// [`BlobProvider`] that serves only the given `data`.
    fn const_provider(data: Vec<u8>) -> impl BlobProvider {
        move |_hash| {
            let data = Cursor::new(data.clone());
            async move { Ok(data) }
        }
    }

    #[tokio::test]
    async fn verifies_hash_of_uncompressed_blob() {
        let tmp = tempdir().unwrap();
        let dir = SnapshotsPath::from_path_unchecked(tmp.path());

        let blob_hash = blake3::hash(&[1; 32]);
        let sf = SnapshotFetcher::create(
            zeroes_provider(),
            dir,
            DUMMY_SNAPSHOT,
            default_page_pool(),
            BufPool::default(),
        )
        .unwrap();

        sf.fetch_blob(blake3::hash(ZEROES)).await.unwrap();
        assert_matches!(sf.fetch_blob(blob_hash).await, Err(SnapshotError::HashMismatch { .. }));
    }

    #[tokio::test]
    async fn verifies_hash_of_compressed_blob() {
        let tmp = tempdir().unwrap();
        let dir = SnapshotsPath::from_path_unchecked(tmp.path());

        let blob_data = [1; 1024];
        let blob_hash = blake3::hash(&blob_data);
        let mut blob_zstd = Vec::new();
        compress(&mut blob_data.as_slice(), &mut blob_zstd).await;

        let sf = SnapshotFetcher::create(
            const_provider(blob_zstd),
            dir,
            DUMMY_SNAPSHOT,
            default_page_pool(),
            BufPool::default(),
        )
        .unwrap();

        sf.fetch_blob(blob_hash).await.unwrap();
        assert_matches!(
            sf.fetch_blob(blake3::hash(ZEROES)).await,
            Err(SnapshotError::HashMismatch { .. })
        );
    }

    #[tokio::test]
    async fn verifies_hash_of_uncompressed_page() {
        let tmp = tempdir().unwrap();
        let dir = SnapshotsPath::from_path_unchecked(tmp.path());

        let mut page = Page::new(u64_row_size());
        for val in 0..64 {
            insert_u64(&mut page, val);
        }
        let page_hash = page_hash_save_get(&mut page);
        let page_blob = bsatn::to_vec(&page).unwrap();

        let sf = SnapshotFetcher::create(
            const_provider(page_blob),
            dir,
            DUMMY_SNAPSHOT,
            default_page_pool(),
            BufPool::default(),
        )
        .unwrap();

        sf.fetch_page(page_hash).await.unwrap();
        assert_matches!(
            sf.fetch_page(blake3::hash(ZEROES)).await,
            Err(SnapshotError::HashMismatch { .. })
        );
    }

    #[tokio::test]
    async fn verifies_hash_of_compressed_page() {
        let tmp = tempdir().unwrap();
        let dir = SnapshotsPath::from_path_unchecked(tmp.path());

        let mut page = Page::new(u64_row_size());
        for val in 0..64 {
            insert_u64(&mut page, val);
        }
        let page_hash = page_hash_save_get(&mut page);
        let page_blob = bsatn::to_vec(&page).unwrap();
        let mut page_zstd = Vec::new();
        compress(&mut page_blob.as_slice(), &mut page_zstd).await;

        let sf = SnapshotFetcher::create(
            const_provider(page_zstd),
            dir,
            DUMMY_SNAPSHOT,
            default_page_pool(),
            BufPool::default(),
        )
        .unwrap();

        sf.fetch_page(page_hash).await.unwrap();
        pretty_assertions::assert_matches!(
            sf.fetch_page(blake3::hash(ZEROES)).await,
            Err(SnapshotError::HashMismatch { .. })
        );
    }

    async fn compress(input: &mut &[u8], output: &mut Vec<u8>) {
        let mut zstd = AsyncZstdWriter::builder(output).with_seek_table(256).build().unwrap();
        tokio::io::copy(input, &mut zstd).await.unwrap();
    }

    fn u64_row_size() -> Size {
        let fixed_row_size = row_size_for_type::<u64>();
        assert_eq!(fixed_row_size.len(), 8);
        fixed_row_size
    }

    fn insert_u64(page: &mut Page, val: u64) -> PageOffset {
        let val_slice = val.to_le_bytes();
        unsafe { page.insert_row(&val_slice, &[] as &[&[u8]], u64_var_len_visitor(), &mut NullBlobStore) }
            .expect("Failed to insert first row")
    }

    const U64_VL_VISITOR: AlignedVarLenOffsets<'_> = AlignedVarLenOffsets::from_offsets(&[]);
    fn u64_var_len_visitor() -> &'static AlignedVarLenOffsets<'static> {
        &U64_VL_VISITOR
    }

    fn page_hash_save_get(page: &mut Page) -> blake3::Hash {
        page.save_content_hash();
        page.content_hash()
    }
}
