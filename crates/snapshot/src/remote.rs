use std::{
    future::Future,
    io,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures::{stream, StreamExt as _, TryStreamExt};
use spacetimedb_fs_utils::{compression::ZSTD_MAGIC_BYTES, dir_trie::DirTrie, lockfile::Lockfile};
use spacetimedb_lib::bsatn;
use spacetimedb_paths::server::{SnapshotDirPath, SnapshotsPath};
use spacetimedb_sats::Serialize;
use spacetimedb_table::{blob_store::BlobHash, page::Page};
use tempfile::NamedTempFile;
use tokio::{
    fs,
    io::{AsyncBufRead, AsyncBufReadExt as _, AsyncReadExt as _, AsyncWrite, AsyncWriteExt, BufReader, BufWriter},
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
    spawn_blocking(|| SnapshotFetcher::create(provider, snapshots_dir, snapshot))
        .await
        .unwrap()?
        .run()
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

struct SnapshotFetcher<P> {
    snapshot: Snapshot,
    dir: SnapshotDirPath,
    object_repo: Arc<DirTrie>,
    parent_repo: Option<Arc<DirTrie>>,
    provider: P,

    stats: StatsInner,

    // NOTE: This should remain the last declared field,
    // so that the lock file is dropped last when `self` is dropped.
    #[allow(unused)]
    lock: Lockfile,
}

impl<P: BlobProvider> SnapshotFetcher<P> {
    fn create(provider: P, snapshots_dir: SnapshotsPath, snapshot: Snapshot) -> Result<Self> {
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
            stats: <_>::default(),
            lock,
        })
    }

    async fn run(self) -> Result<Stats> {
        let snapshot_bsatn = serialize_bsatn(ObjectType::Snapshot, &self.snapshot)?;
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
            let existing_bsatn = serialize_bsatn(ObjectType::Snapshot, &existing)?;
            let existing_hash = blake3::hash(&existing_bsatn);

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
        atomically(snapshot_file_path.0, |out| async {
            let mut out = BufWriter::new(out);
            out.write_all(snapshot_hash.as_bytes()).await?;
            out.write_all(&snapshot_bsatn).await?;
            out.flush().await?;
            out.into_inner().sync_all().await?;

            Ok(())
        })
        .await?;

        Ok(self.stats.into())
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
            .collect::<Vec<_>>();
        stream::iter(tasks)
            .map(Ok)
            .try_for_each_concurrent(8, |task| task)
            .await
    }

    async fn fetch_pages(&self) -> Result<()> {
        let tasks = self
            .snapshot
            .tables
            .iter()
            .flat_map(|entry| entry.pages.iter().copied().map(|hash| self.fetch_page(hash)))
            .collect::<Vec<_>>();
        stream::iter(tasks)
            .map(Ok)
            .try_for_each_concurrent(8, |task| task)
            .await
    }

    async fn fetch_blob(&self, hash: blake3::Hash) -> Result<()> {
        let Some(dst_path) = self.object_file_path(hash).await? else {
            return Ok(());
        };
        atomically(dst_path, |out| async move {
            let mut out = BufWriter::new(out);
            let mut src = self.provider.blob_reader(hash).await?;
            let compressed = src.fill_buf().await?.starts_with(&ZSTD_MAGIC_BYTES);

            // Consume the blob reader,
            // write its contents to `out`,
            // and compute the content hash on the fly.
            let mut hasher = blake3::Hasher::new();
            let computed_hash = if !compressed {
                // If the input is uncompressed, just update the hasher as we go.
                let mut out = InspectWriter::new(out, |chunk| {
                    hasher.update(chunk);
                });
                tokio::io::copy_buf(&mut src, &mut out).await?;
                out.flush().await?;
                out.into_inner().into_inner().sync_all().await?;

                hasher.finalize()
            } else {
                // If the input is compressed, send a copy of all received
                // chunks to a separate task that decompresses the stream and
                // computes the hash from the decompressed bytes.
                let (mut zstd, tx) = zstd_reader()?;
                let decompressor = tokio::spawn(async move {
                    let mut hasher = AsyncHasher::from(hasher);
                    tokio::io::copy_buf(&mut zstd, &mut hasher).await?;
                    Ok::<_, io::Error>(hasher.hash())
                });

                let mut buf = BytesMut::new();
                let mut src = InspectReader::new(src, |chunk| {
                    buf.extend_from_slice(chunk);
                    tx.send(Ok(buf.split().freeze())).ok();
                });
                tokio::io::copy(&mut src, &mut out).await?;
                out.flush().await?;
                out.into_inner().sync_all().await?;

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

            Ok(())
        })
        .await
        .inspect(|()| {
            self.stats.wrote_object();
        })
    }

    async fn fetch_page(&self, hash: blake3::Hash) -> Result<()> {
        let Some(dst_path) = self.object_file_path(hash).await? else {
            return Ok(());
        };
        atomically(dst_path, |out| async {
            let mut src = self.provider.blob_reader(hash).await?;
            let compressed = src.fill_buf().await?.starts_with(&ZSTD_MAGIC_BYTES);

            // To compute the page hash, we need to bsatn deserialize it.
            // As bsatn doesn't support streaming deserialization yet,
            // we need to keep a copy of the input bytes,
            // while also writing them to `out`.
            let page_buf = if !compressed {
                // If the input is uncompressed, just copy all bytes to a buffer.
                let mut page_buf = Vec::with_capacity(u16::MAX as usize + 1);
                let mut out = InspectWriter::new(BufWriter::new(out), |chunk| {
                    page_buf.extend_from_slice(chunk);
                });
                tokio::io::copy_buf(&mut src, &mut out).await?;
                out.flush().await?;
                out.into_inner().into_inner().sync_all().await?;

                page_buf
            } else {
                // If the input is compressed, send all received chunks to a
                // separate task that decompresses the stream and returns
                // the uncompressed bytes.
                let (mut zstd, tx) = zstd_reader()?;
                let decompressor = tokio::spawn(async move {
                    let mut page_buf = Vec::with_capacity(u16::MAX as usize + 1);
                    zstd.read_to_end(&mut page_buf).await?;
                    Ok::<_, io::Error>(page_buf)
                });

                let mut out = InspectWriter::new(BufWriter::new(out), |chunk| {
                    let bytes = Bytes::copy_from_slice(chunk);
                    tx.send(Ok(bytes)).ok();
                });
                tokio::io::copy_buf(&mut src, &mut out).await?;
                out.flush().await?;
                out.into_inner().into_inner().sync_all().await?;

                drop(tx);
                decompressor.await.unwrap()?
            };

            self.verify_page(hash, &page_buf)
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
    async fn object_file_path(&self, hash: blake3::Hash) -> Result<Option<PathBuf>> {
        let path = self.object_repo.file_path(hash.as_bytes());
        if fs::try_exists(&path).await? {
            self.stats.skipped_object();
            return Ok(None);
        }

        if self.try_hardlink(hash).await? {
            self.stats.hardlinked_object();
            return Ok(None);
        }

        Ok(Some(path))
    }

    async fn try_hardlink(&self, hash: blake3::Hash) -> Result<bool> {
        let Some(parent) = self.parent_repo.as_ref() else {
            return Ok(false);
        };

        let object_repo = Arc::clone(&self.object_repo);
        let parent_repo = Arc::clone(parent);
        spawn_blocking(move || object_repo.try_hardlink_from(&parent_repo, hash.as_bytes()))
            .await
            .unwrap()
            .map_err(Into::into)
    }

    fn verify_page(&self, expected_hash: blake3::Hash, buf: &[u8]) -> Result<()> {
        let page = bsatn::from_slice::<Box<Page>>(buf).map_err(|cause| SnapshotError::Deserialize {
            ty: ObjectType::Page(expected_hash),
            source_repo: self.dir.0.clone(),
            cause,
        })?;
        let computed_hash = page.content_hash();
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

type ZstdReader = AsyncZstdReader<'static, BufReader<StreamReader<UnboundedReceiverStream<io::Result<Bytes>>, Bytes>>>;

fn zstd_reader() -> io::Result<(ZstdReader, mpsc::UnboundedSender<io::Result<Bytes>>)> {
    let (tx, rx) = mpsc::unbounded_channel::<io::Result<Bytes>>();
    let reader = StreamReader::new(UnboundedReceiverStream::new(rx));
    let zstd = AsyncZstdReader::builder_tokio(reader).build()?;

    Ok((zstd, tx))
}

struct AsyncHasher {
    inner: blake3::Hasher,
}

impl AsyncHasher {
    pub fn hash(&self) -> blake3::Hash {
        self.inner.finalize()
    }
}

impl From<blake3::Hasher> for AsyncHasher {
    fn from(inner: blake3::Hasher) -> Self {
        Self { inner }
    }
}

impl AsyncWrite for AsyncHasher {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.get_mut().inner.update(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

async fn atomically<F, Fut>(file_path: PathBuf, f: F) -> Result<()>
where
    F: FnOnce(fs::File) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let dir = file_path.parent().expect("file not in a directory").to_owned();
    fs::create_dir_all(&dir).await?;
    let (tmp_file, tmp_out) = spawn_blocking(move || {
        let tmp = NamedTempFile::new_in(dir)?;
        let out = tmp.reopen()?;
        Ok::<_, io::Error>((tmp, out))
    })
    .await
    .unwrap()?;

    f(fs::File::from_std(tmp_out)).await?;

    spawn_blocking(|| tmp_file.persist(file_path))
        .await
        .unwrap()
        .map_err(|e| e.error)?;

    Ok(())
}

fn serialize_bsatn(ty: ObjectType, value: &impl Serialize) -> Result<Vec<u8>> {
    bsatn::to_vec(value).map_err(|cause| SnapshotError::Serialize { ty, cause })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pretty_assertions::assert_matches;
    use spacetimedb_lib::{bsatn, Identity};
    use spacetimedb_paths::{server::SnapshotsPath, FromPathUnchecked};
    use spacetimedb_table::{
        blob_store::NullBlobStore,
        indexes::{PageOffset, Size},
        layout::row_size_for_type,
        page::Page,
        var_len::AlignedVarLenOffsets,
    };
    use tempfile::tempdir;
    use zstd_framed::AsyncZstdWriter;

    use super::{BlobProvider, SnapshotFetcher};
    use crate::{Snapshot, SnapshotError, CURRENT_MODULE_ABI_VERSION, CURRENT_SNAPSHOT_VERSION, MAGIC};

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
        let sf = SnapshotFetcher::create(zeroes_provider(), dir, DUMMY_SNAPSHOT).unwrap();

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

        let sf = SnapshotFetcher::create(const_provider(blob_zstd), dir, DUMMY_SNAPSHOT).unwrap();

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

        let sf = SnapshotFetcher::create(const_provider(page_blob), dir, DUMMY_SNAPSHOT).unwrap();

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

        let sf = SnapshotFetcher::create(const_provider(page_zstd), dir, DUMMY_SNAPSHOT).unwrap();

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
