//! Content-addressed storage for the bytes of SpacetimeDB files.
//!
//! The database records each file as a row in `st_file`, and keeps the file's bytes here,
//! under the BLAKE3 hash of its contents. File bytes never pass through the commitlog,
//! so a [`FileStore`] must make an object durable before any transaction that references it
//! is reported durable. In production the store is backed by S3; [`LocalFileStore`] keeps
//! objects in a local directory and is the durable home for files on a standalone host.
//!
//! Objects are immutable, and storing the same contents twice stores them once.
//! Nothing here knows which objects are still referenced; [`reclaim_unreferenced`] deletes
//! objects outside a set of roots that the caller computes from snapshots, live state, and
//! retained history.

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use spacetimedb_lib::Hash;
use std::collections::HashSet;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

#[cfg(feature = "s3")]
mod s3;
#[cfg(feature = "s3")]
pub use s3::{S3Config, S3FileStore};

pub type ByteStream = Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>>;

/// An object in a [`FileStore`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredObject {
    pub hash: Hash,
    pub size: u64,
    /// When the object was stored. Reclamation spares objects younger than its grace period,
    /// which protects uploads whose `st_file` row hasn't committed yet.
    pub stored_at: SystemTime,
}

/// Durable, content-addressed storage for file bytes.
#[async_trait]
pub trait FileStore: Send + Sync + 'static {
    /// Starts receiving a file. The spool hashes the bytes as they arrive.
    async fn spool(&self) -> io::Result<Spool>;

    /// Stores a spooled file under its hash, and returns once the object is durable.
    ///
    /// If an object with this hash already exists, the spool is discarded and the object's
    /// `stored_at` is refreshed. Reclamation relies on that refresh to spare an object that is
    /// about to be referenced again.
    async fn persist(&self, spooled: Spooled) -> io::Result<()>;

    /// The size of the object stored under `hash`, or `None` if there is none.
    async fn size(&self, hash: &Hash) -> io::Result<Option<u64>>;

    /// Reads `range` of the object stored under `hash`.
    ///
    /// Returns [`io::ErrorKind::NotFound`] if there is no such object.
    /// The range must lie within the object.
    async fn read(&self, hash: &Hash, range: Range<u64>) -> io::Result<ByteStream>;

    /// Lists every object in the store.
    async fn list(&self) -> io::Result<Vec<StoredObject>>;

    /// Deletes the object stored under `hash`. Deleting a missing object succeeds.
    async fn delete(&self, hash: &Hash) -> io::Result<()>;

    /// Deletes the object stored under `hash` only if it was last persisted before `cutoff`,
    /// and returns whether it did.
    ///
    /// Reclamation uses this so that persisting the same contents again, which refreshes the object,
    /// saves it even after reclamation has decided to delete it. An S3 store would make this a
    /// conditional delete. The default checks and then deletes, which leaves a small window.
    async fn delete_if_older_than(&self, hash: &Hash, cutoff: SystemTime) -> io::Result<bool> {
        let stale = self
            .list()
            .await?
            .into_iter()
            .any(|object| object.hash == *hash && object.stored_at < cutoff);
        if stale {
            self.delete(hash).await?;
        }
        Ok(stale)
    }

    /// Stores `bytes`, which must hash to `hash`, and returns once the object is durable.
    async fn persist_bytes(&self, hash: Hash, bytes: Bytes) -> io::Result<()> {
        let mut spool = self.spool().await?;
        spool.write(&bytes).await?;
        let spooled = spool.finish().await?;
        if spooled.hash() != hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected contents with hash {hash}, got {}", spooled.hash()),
            ));
        }
        self.persist(spooled).await
    }
}

/// A file being received, written to a temporary file while it is hashed.
pub struct Spool {
    file: tokio::fs::File,
    temp: TempPath,
    hasher: blake3::Hasher,
    size: u64,
}

impl Spool {
    /// Creates a spool in `dir`, which should be on the same file system as the objects it will become.
    pub async fn create_in(dir: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "spool-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = dir.join(name);
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await?;
        Ok(Self {
            file,
            temp: TempPath(Some(path)),
            hasher: blake3::Hasher::new(),
            size: 0,
        })
    }

    pub async fn write(&mut self, chunk: &[u8]) -> io::Result<()> {
        self.hasher.update(chunk);
        self.size += chunk.len() as u64;
        self.file.write_all(chunk).await
    }

    /// The number of bytes written so far.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Flushes the spooled bytes to disk and returns them with their hash.
    pub async fn finish(mut self) -> io::Result<Spooled> {
        self.file.flush().await?;
        self.file.sync_all().await?;
        Ok(Spooled {
            hash: Hash::from_byte_array(*self.hasher.finalize().as_bytes()),
            size: self.size,
            temp: self.temp,
        })
    }
}

/// A completely received file, not yet stored.
///
/// Dropping it deletes the temporary file.
pub struct Spooled {
    hash: Hash,
    size: u64,
    temp: TempPath,
}

impl Spooled {
    /// The BLAKE3 hash of the file's contents.
    pub fn hash(&self) -> Hash {
        self.hash
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn path(&self) -> &Path {
        self.temp.path()
    }

    /// Takes ownership of the temporary file, so that dropping `self` no longer deletes it.
    fn into_path(mut self) -> PathBuf {
        self.temp.0.take().expect("a spooled file always has a path")
    }
}

/// Deletes the file at its path when dropped.
struct TempPath(Option<PathBuf>);

impl TempPath {
    fn path(&self) -> &Path {
        self.0
            .as_deref()
            .expect("a temporary file always has a path until it is taken")
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if let Some(path) = self.0.take()
            && let Err(e) = std::fs::remove_file(&path)
            && e.kind() != io::ErrorKind::NotFound
        {
            log::warn!("failed to remove temporary file {}: {e}", path.display());
        }
    }
}

/// A [`FileStore`] in a local directory.
///
/// Objects live at `objects/<first two hex digits>/<remaining hex digits>`, and spools in `tmp/`.
/// An object is written to a temporary file, synced, and renamed into place, and the directory is
/// synced, so a crash never exposes a partial object under its final name.
pub struct LocalFileStore {
    objects: PathBuf,
    tmp: PathBuf,
}

impl LocalFileStore {
    /// Opens the store at `root`, creating it if needed, and deletes spools left by a previous process.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        let objects = root.join("objects");
        let tmp = root.join("tmp");
        std::fs::create_dir_all(&objects)?;
        std::fs::create_dir_all(&tmp)?;
        for entry in std::fs::read_dir(&tmp)? {
            let entry = entry?;
            if let Err(e) = std::fs::remove_file(entry.path()) {
                log::warn!("failed to remove stale spool {}: {e}", entry.path().display());
            }
        }
        Ok(Self { objects, tmp })
    }

    fn object_path(&self, hash: &Hash) -> PathBuf {
        let hex = hash.to_hex();
        let hex: &str = hex.as_ref();
        self.objects.join(&hex[..2]).join(&hex[2..])
    }
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[async_trait]
impl FileStore for LocalFileStore {
    async fn spool(&self) -> io::Result<Spool> {
        Spool::create_in(&self.tmp).await
    }

    async fn persist(&self, spooled: Spooled) -> io::Result<()> {
        let dest = self.object_path(&spooled.hash());
        if tokio::fs::try_exists(&dest).await? {
            // Refresh the object's age, so a reclamation pass that already chose it spares it.
            let dest = dest.clone();
            return tokio::task::spawn_blocking(move || {
                std::fs::File::options()
                    .write(true)
                    .open(&dest)?
                    .set_modified(SystemTime::now())
            })
            .await
            .map_err(io::Error::other)?;
        }
        let src = spooled.into_path();
        tokio::task::spawn_blocking(move || {
            let mut src = TempPath(Some(src));
            let parent = dest.parent().expect("an object path always has a parent");
            std::fs::create_dir_all(parent)?;
            std::fs::rename(src.path(), &dest)?;
            // The temporary file is gone now; don't try to delete it.
            src.0.take();
            sync_dir(parent)?;
            sync_dir(parent.parent().expect("the fan-out directory always has a parent"))
        })
        .await
        .map_err(io::Error::other)?
    }

    async fn size(&self, hash: &Hash) -> io::Result<Option<u64>> {
        match tokio::fs::metadata(self.object_path(hash)).await {
            Ok(meta) => Ok(Some(meta.len())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn read(&self, hash: &Hash, range: Range<u64>) -> io::Result<ByteStream> {
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(self.object_path(hash)).await?;
        let len = file.metadata().await?.len();
        if range.start > range.end || range.end > len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("range {range:?} is outside an object of {len} bytes"),
            ));
        }
        file.seek(io::SeekFrom::Start(range.start)).await?;
        let reader = file.take(range.end - range.start);
        Ok(Box::pin(tokio_util::io::ReaderStream::new(reader)))
    }

    async fn list(&self) -> io::Result<Vec<StoredObject>> {
        let objects = self.objects.clone();
        tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            for fan in std::fs::read_dir(&objects)? {
                let fan = fan?;
                let prefix = fan.file_name();
                let Some(prefix) = prefix.to_str().filter(|p| p.len() == 2) else {
                    continue;
                };
                for entry in std::fs::read_dir(fan.path())? {
                    let entry = entry?;
                    let name = entry.file_name();
                    let Some(rest) = name.to_str() else { continue };
                    let Ok(hash) = Hash::from_hex(format!("{prefix}{rest}")) else {
                        continue;
                    };
                    let meta = entry.metadata()?;
                    out.push(StoredObject {
                        hash,
                        size: meta.len(),
                        stored_at: meta.modified()?,
                    });
                }
            }
            Ok(out)
        })
        .await
        .map_err(io::Error::other)?
    }

    async fn delete(&self, hash: &Hash) -> io::Result<()> {
        match tokio::fs::remove_file(self.object_path(hash)).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    async fn delete_if_older_than(&self, hash: &Hash, cutoff: SystemTime) -> io::Result<bool> {
        let path = self.object_path(hash);
        let stored_at = match tokio::fs::metadata(&path).await {
            Ok(meta) => meta.modified()?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        if stored_at >= cutoff {
            return Ok(false);
        }
        self.delete(hash).await?;
        Ok(true)
    }
}

/// What [`reclaim_unreferenced`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReclaimStats {
    pub deleted_objects: u64,
    pub deleted_bytes: u64,
    /// Unreferenced objects spared because they are younger than the grace period.
    pub spared_young_objects: u64,
}

/// Deletes every object that isn't in `roots` and is older than `grace`.
///
/// The caller must include in `roots` every hash that:
///
/// - a retained snapshot lists,
/// - a live `st_file` row names,
/// - an `st_file` row inserted in the retained commitlog names,
///   since reverting to a point in that log restores the row, and
/// - a transaction in flight may still commit.
///
/// The grace period covers uploads that have been persisted but whose `st_file` row hasn't
/// committed yet. It must be longer than the longest such window.
pub async fn reclaim_unreferenced(
    store: &dyn FileStore,
    roots: &HashSet<Hash>,
    grace: Duration,
    now: SystemTime,
) -> io::Result<ReclaimStats> {
    let mut stats = ReclaimStats::default();
    let cutoff = now.checked_sub(grace).unwrap_or(SystemTime::UNIX_EPOCH);
    for object in store.list().await? {
        if roots.contains(&object.hash) {
            continue;
        }
        if object.stored_at >= cutoff || !store.delete_if_older_than(&object.hash, cutoff).await? {
            stats.spared_young_objects += 1;
            continue;
        }
        stats.deleted_objects += 1;
        stats.deleted_bytes += object.size;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::TryStreamExt;
    use spacetimedb_lib::file::content_hash;

    async fn read_all(store: &dyn FileStore, hash: &Hash, range: Range<u64>) -> Vec<u8> {
        let chunks: Vec<Bytes> = store.read(hash, range).await.unwrap().try_collect().await.unwrap();
        chunks.concat()
    }

    #[tokio::test]
    async fn persisted_objects_are_readable_by_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let bytes = Bytes::from_static(b"hello, files");
        let hash = content_hash(&bytes);

        store.persist_bytes(hash, bytes.clone()).await.unwrap();

        assert_eq!(store.size(&hash).await.unwrap(), Some(bytes.len() as u64));
        assert_eq!(read_all(&store, &hash, 0..12).await, b"hello, files");
        assert_eq!(read_all(&store, &hash, 7..12).await, b"files");
        assert!(store.read(&hash, 5..13).await.is_err());
    }

    #[tokio::test]
    async fn spool_hashes_what_it_receives() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let mut spool = store.spool().await.unwrap();
        spool.write(b"hello, ").await.unwrap();
        spool.write(b"files").await.unwrap();
        let spooled = spool.finish().await.unwrap();
        assert_eq!(spooled.hash(), content_hash(b"hello, files"));
        assert_eq!(spooled.size(), 12);
        let hash = spooled.hash();
        store.persist(spooled).await.unwrap();
        assert_eq!(read_all(&store, &hash, 0..12).await, b"hello, files");
        // The spool's temporary file was moved, not left behind.
        assert_eq!(std::fs::read_dir(dir.path().join("tmp")).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn storing_the_same_contents_twice_keeps_one_object() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let bytes = Bytes::from_static(b"same");
        let hash = content_hash(&bytes);
        store.persist_bytes(hash, bytes.clone()).await.unwrap();
        store.persist_bytes(hash, bytes).await.unwrap();
        assert_eq!(store.list().await.unwrap().len(), 1);
        assert_eq!(std::fs::read_dir(dir.path().join("tmp")).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn persist_bytes_rejects_the_wrong_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let err = store
            .persist_bytes(content_hash(b"other"), Bytes::from_static(b"bytes"))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(store.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn dropped_spools_leave_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let mut spool = store.spool().await.unwrap();
        spool.write(b"abandoned upload").await.unwrap();
        drop(spool);
        assert_eq!(std::fs::read_dir(dir.path().join("tmp")).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn reclaim_deletes_only_old_unreferenced_objects() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let mut hashes = Vec::new();
        for contents in [&b"kept"[..], b"young", b"garbage"] {
            let hash = content_hash(contents);
            store
                .persist_bytes(hash, Bytes::copy_from_slice(contents))
                .await
                .unwrap();
            hashes.push(hash);
        }
        let [kept, young, garbage] = hashes[..] else {
            unreachable!()
        };
        let roots = HashSet::from([kept]);
        let grace = Duration::from_secs(60);

        // Everything is younger than the grace period, so nothing is deleted yet.
        let stats = reclaim_unreferenced(&store, &roots, grace, SystemTime::now())
            .await
            .unwrap();
        assert_eq!(stats.deleted_objects, 0);
        assert_eq!(stats.spared_young_objects, 2);

        // Make `garbage` old, and leave `young` young.
        let old = SystemTime::now() - Duration::from_secs(3600);
        let path = store.object_path(&garbage);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();

        let stats = reclaim_unreferenced(&store, &roots, grace, SystemTime::now())
            .await
            .unwrap();
        assert_eq!(stats.deleted_objects, 1);
        assert_eq!(stats.deleted_bytes, b"garbage".len() as u64);
        assert_eq!(store.size(&garbage).await.unwrap(), None);
        assert!(store.size(&kept).await.unwrap().is_some());
        assert!(store.size(&young).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn open_clears_stale_spools() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let spool = store.spool().await.unwrap();
        // Simulate a crash: the process dies without dropping the spool.
        std::mem::forget(spool);
        assert_eq!(std::fs::read_dir(dir.path().join("tmp")).unwrap().count(), 1);
        let _store = LocalFileStore::open(dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path().join("tmp")).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn persisting_again_saves_an_object_from_reclamation() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalFileStore::open(dir.path()).unwrap();
        let hash = content_hash(b"again");
        store.persist_bytes(hash, Bytes::from_static(b"again")).await.unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(store.object_path(&hash))
            .unwrap()
            .set_modified(old)
            .unwrap();

        // Reclamation decided on a cutoff before the object was persisted again.
        let cutoff = SystemTime::now() - Duration::from_secs(60);
        store.persist_bytes(hash, Bytes::from_static(b"again")).await.unwrap();
        assert!(!store.delete_if_older_than(&hash, cutoff).await.unwrap());
        assert!(store.size(&hash).await.unwrap().is_some());
    }
}
