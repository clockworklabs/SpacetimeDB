//! This crate implements capturing and restoring snapshots in SpacetimeDB.
//!
//! A snapshot is an on-disk view of the committed state of a database at a particular transaction offset.
//! Snapshots exist as an optimization over replaying the commitlog;
//! when restoring to the most recent transaction, rather than replaying the commitlog from 0,
//! we can reload the most recent snapshot, then replay only the suffix of the commitlog.
//!
//! This crate is responsible for:
//! - The on-disk format of snapshots.
//! - A [`SnapshotRepository`] which contains multiple snapshots of a DB and can create and retrieve them.
//! - Creating a snapshot given a view of a DB's committed state in [`SnapshotRepository::create_snapshot`].
//! - Reading an on-disk snapshot into memory as a [`ReconstructedSnapshot`] in [`SnapshotRepository::read_snapshot`].
//!   The [`ReconstructedSnapshot`] can then be installed into a datastore.
//! - Locating the most-recent snapshot of a DB, or the most recent snapshot not newer than a given tx offset,
//!   in [`SnapshotRepository::latest_snapshot`] and [`SnapshotRepository::latest_snapshot_older_than`].
//!
//! This crate *is not* responsible for:
//! - Determining when to capture snapshots.
//! - Deciding which snapshot to restore from after a restart.
//! - Replaying the suffix of the commitlog after restoring a snapshot.
//! - Transforming a [`ReconstructedSnapshot`] into a live Spacetime datastore.
// TODO(docs): consider making the snapshot proposal public and either linking or pasting it here.

#![allow(clippy::result_large_err)]

use log::error;
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashMap};
use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::compression::{
    compress_with_zstd, CompressCount, CompressReader, CompressType, CompressionAlgorithm,
};
use spacetimedb_fs_utils::{
    dir_trie::{o_excl, o_rdonly, DirTrie},
    lockfile::{Lockfile, LockfileError},
};
use spacetimedb_lib::ser::Serializer;
use spacetimedb_lib::Identity;
use spacetimedb_paths::server::{ArchivedSnapshotDirPath, SnapshotDirPath, SnapshotFilePath, SnapshotsPath};
use spacetimedb_paths::FromPathUnchecked;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{bsatn, de::Deserialize, ser::Serialize};
use spacetimedb_table::table::Table;
use spacetimedb_table::tiered::{
    self, BlobError, BlobMap, ObjectKey, ObjectStoreError, PageError, PageHash, SnapshotObjectStorage,
    TieredDatabaseStorage,
};
use spacetimedb_table::{blob_store::BlobHash, page::Page, page_pool::PagePool};
use std::fs::{self, File};
use std::io;
use std::ops::{Range, RangeBounds};
use std::path::Path;
use std::time::{Duration, Instant};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fmt,
    io::BufWriter,
    ops::{Add, AddAssign},
    path::PathBuf,
};
use tokio::task::spawn_blocking;

mod common;
pub mod remote;
mod v0;
mod v1;

use crate::remote::ObjectProvider;
use crate::{
    common::{read_from_file, read_from_file_with_metrics},
    remote::verify_snapshot,
};

#[derive(Debug, Copy, Clone)]
/// An object which may be associated with an error during snapshotting.
pub enum ObjectType {
    Blob(BlobHash),
    Page(blake3::Hash),
    Snapshot,
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ObjectType::Blob(hash) => write!(f, "blob {hash:x?}"),
            ObjectType::Page(hash) => write!(f, "page {hash:x?}"),
            ObjectType::Snapshot => write!(f, "snapshot"),
        }
    }
}

impl From<ObjectKey> for ObjectType {
    fn from(key: ObjectKey) -> Self {
        match key {
            ObjectKey::Page(hash) => Self::Page(hash.into()),
            ObjectKey::Blob(hash) => Self::Blob(hash.into()),
        }
    }
}

impl From<PageHash> for ObjectType {
    fn from(hash: PageHash) -> Self {
        Self::Page(hash.into())
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct SnapshotReadKindMetrics {
    /// Time spent reading, decoding, and verifying objects of this kind.
    pub read_time: Duration,
    /// Number of files read for this kind.
    pub files: u64,
    /// Number of bytes stored on disk for files of this kind.
    pub disk_bytes: u64,
    /// Time spent computing content hashes for objects of this kind.
    pub hash_time: Duration,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct SnapshotReadMetrics {
    pub metadata: SnapshotReadKindMetrics,
    pub page: SnapshotReadKindMetrics,
    pub blob: SnapshotReadKindMetrics,
    /// Number of files skipped due to having [`ZERO_HASH_DENOTING_ABSENT_PAGE`] as their hash.
    ///
    /// This is only meaningful for pages, so is stored separate from the other metrics.
    pub absent_pages: u64,
}

pub type TablePages = Vec<Option<Box<Page>>>;

impl SnapshotReadMetrics {
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &SnapshotReadKindMetrics)> + '_ {
        [("metadata", &self.metadata), ("page", &self.page), ("blob", &self.blob)].into_iter()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SnapshotError {
    #[error("Cannot open SnapshotRepo {0}: not an accessible directory")]
    Open(PathBuf),
    #[error("Failed to write {ty} to {dest_repo:?}, attempting to hardlink link from {source_repo:?}: {cause}")]
    WriteObject {
        ty: ObjectType,
        dest_repo: PathBuf,
        source_repo: Option<PathBuf>,
        #[source]
        cause: std::io::Error,
    },
    #[error("Failed to read {ty} from {source_repo:?}: {cause}")]
    ReadObject {
        ty: ObjectType,
        source_repo: PathBuf,
        #[source]
        cause: std::io::Error,
    },
    #[error("Encountered corrupted {ty} in {source_repo:?}: expected hash {expected:x?}, but computed {computed:x?}")]
    HashMismatch {
        ty: ObjectType,
        expected: [u8; 32],
        computed: [u8; 32],
        source_repo: PathBuf,
    },
    #[error("Failed to BSATN serialize {ty}: {cause}")]
    Serialize {
        ty: ObjectType,
        #[source]
        cause: bsatn::ser::BsatnError,
    },
    #[error("Failed to BSATN deserialize {ty} from {source_repo:?}: {cause}")]
    Deserialize {
        ty: ObjectType,
        source_repo: PathBuf,
        cause: bsatn::DecodeError,
    },
    #[error("Refusing to reconstruct incomplete snapshot {tx_offset}: lockfile {lockfile:?} exists")]
    Incomplete { tx_offset: TxOffset, lockfile: PathBuf },
    #[error("Refusing to reconstruct snapshot {tx_offset} with bad magic number {magic:x?}")]
    BadMagic { tx_offset: TxOffset, magic: [u8; 4] },
    #[error("Refusing to reconstruct snapshot {tx_offset} with unsupported version {version}")]
    BadVersion { tx_offset: TxOffset, version: u8 },
    #[error("Cannot open snapshot repository in non-directory {root:?}")]
    NotDirectory { root: SnapshotsPath },
    #[error(transparent)]
    Lockfile(#[from] LockfileError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Blob(#[from] BlobError),
    #[error(transparent)]
    ObjectStore(#[from] ObjectStoreError),
    #[error(transparent)]
    Tiered(#[from] tiered::SnapshotError),
}

impl SnapshotError {
    pub fn is_already_exists(&self) -> bool {
        matches!(self, Self::Io(e) if e.kind() == std::io::ErrorKind::AlreadyExists)
    }
}

/// Magic number for snapshot files: a point in spacetime.
///
/// Chosen because the commitlog magic number is a spacetime interval,
/// so a snapshot should be a point to which an interval can be applied.
pub const MAGIC: [u8; 4] = *b"txyz";

/// Snapshot format version number.
pub const CURRENT_SNAPSHOT_VERSION: u8 = 1;

/// ABI version of the module from which this snapshot was created, as [MAJOR, MINOR].
pub const CURRENT_MODULE_ABI_VERSION: [u16; 2] = [7, 0];

/// File extension of snapshot directories.
pub const SNAPSHOT_DIR_EXT: &str = "snapshot_dir";

/// File extension of snapshot files, which contain BSATN-encoded [`Snapshot`]s preceded by [`blake3::Hash`]es.
pub const SNAPSHOT_FILE_EXT: &str = "snapshot_bsatn";

/// File extension of snapshots which have been marked invalid by [`SnapshotRepository::invalidate_newer_snapshots`].
pub const INVALID_SNAPSHOT_DIR_EXT: &str = "invalid_snapshot";

/// File extension of snapshots which have been archived
pub const ARCHIVED_SNAPSHOT_EXT: &str = "archived_snapshot";

pub const ZERO_HASH_DENOTING_ABSENT_PAGE: blake3::Hash = blake3::Hash::from_bytes([0; _]);

#[derive(Clone, Serialize, Deserialize)]
/// The hash and refcount of a single blob in the blob store.
struct BlobEntry {
    hash: BlobHash,
    uses: u32,
}

#[derive(Clone, Serialize, Deserialize)]
/// A snapshot of a single table, containing the hashes of all its resident pages.
struct TableEntry {
    table_id: TableId,
    pages: Vec<blake3::Hash>,
}

impl TableEntry {
    /// Iterate over all pages in the table.
    ///
    /// Includes absent pages, use [Self::present_pages] to exclude them.
    pub fn pages(&self) -> impl Iterator<Item = PageHash> {
        self.pages.iter().map(|hash|
            // SAFETY: A snapshot manifest table entry describes pages.
            unsafe { PageHash::from_blake3(*hash) })
    }

    pub fn present_pages(&self) -> impl Iterator<Item = PageHash> {
        self.pages()
            .filter(|hash| blake3::Hash::from(*hash) != ZERO_HASH_DENOTING_ABSENT_PAGE)
    }
}

/// A non-durable snapshot created via [SnapshotRepository::create_snapshot].
///
/// When [SnapshotRepository::create_snapshot] returns, all objects will have
/// been written to the underlying object repository, but not `fsync`'ed.
///
/// Because this means that the snapshot may be incomplete, the [Snapshot] file
/// will _not_ have been written, and the snapshot remains locked (via a [Lockfile]).
///
/// To turn an [UnflushedSnapshot] into a durable snapshot, call
/// [UnflushedSnapshot::sync_all]. This will:
///
/// - sync all objects the snapshot references
/// - sync the object repository root
/// - write and sync the snapshot file
/// - drop the lock file
///
/// This ensures that the snapshot file is present only if all objects are
/// present and durable, and that the snapshot is considered invalid otherwise.
///
/// If [UnflushedSnapshot] is dropped without calling `sync_all`, the [Drop]
/// impl will attempt to call `sync_all` and log any errors.
///
/// This two-stage snapshot creation exists in order to not introduce additional
/// latency while the datastore is locked for snapshotting.
#[must_use = "snapshots are not durable until `sync_all` is called"]
pub struct UnflushedSnapshot {
    inner: Option<UnflushedSnapshotInner>,
}

impl UnflushedSnapshot {
    fn new(snapshot_dir: SnapshotDirPath, snapshot: WritableSnapshot, lockfile: Lockfile) -> Self {
        let inner = UnflushedSnapshotInner {
            snapshot_dir,
            repo: snapshot,
            lockfile,
        };

        Self { inner: Some(inner) }
    }

    /// Return the transaction offset this pending snapshot will finalize at.
    pub fn tx_offset(&self) -> TxOffset {
        self.inner.as_ref().unwrap().repo.tx_offset()
    }

    /// Sync all objects in the snapshot and write out the snapshot file.
    ///
    /// Returns the [SnapshotDirPath] on success.
    pub fn sync_all(mut self) -> Result<SnapshotDirPath, SnapshotError> {
        self.inner.take().unwrap().sync_all()
    }
}

impl Drop for UnflushedSnapshot {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take()
            && let Err(e) = inner.sync_all()
        {
            error!("failed to sync unflushed snapshot dropped without syncing: {e}");
        }
    }
}

struct UnflushedSnapshotInner {
    snapshot_dir: SnapshotDirPath,
    repo: WritableSnapshot,
    lockfile: Lockfile,
}

impl UnflushedSnapshotInner {
    fn sync_all(self) -> Result<SnapshotDirPath, SnapshotError> {
        // Sync all objects and their parent directories.
        // The paths yielded by the [Snapshot::files] iterator are constructed
        // by [DirTree::file_path], which creates a path with a parent.
        // `parent()` is thus known to succeed.
        for path in self.repo.file_paths() {
            FileOrDirPath::File(&path).sync_all()?;
            FileOrDirPath::Dir(path.parent().unwrap()).sync_all()?;
        }
        // Sync the root directory of the object repo
        FileOrDirPath::Dir(self.repo.root()).sync_all()?;
        // Write out the snapshot file (syncs internally).
        self.repo.commit()?;

        // We can now drop the lockfile.
        drop(self.lockfile);

        Ok(self.snapshot_dir)
    }
}

pub trait PendingSnapshot: Send {
    /// Sync all snapshot state and return the finalized transaction offset.
    fn sync_all(self: Box<Self>) -> Result<TxOffset, SnapshotError>;
}

// We pass snapshot repos as `dyn` rather than compile-time generics for convenience,
// as threading generics through a deep callstack would be a hassle.
pub type BoxedPendingSnapshot = Box<dyn PendingSnapshot>;
pub type DynSnapshotRepo = dyn SnapshotRepo<Pending = BoxedPendingSnapshot>;

impl PendingSnapshot for BoxedPendingSnapshot {
    fn sync_all(self: Box<Self>) -> Result<TxOffset, SnapshotError> {
        (*self).sync_all()
    }
}

impl PendingSnapshot for UnflushedSnapshot {
    fn sync_all(self: Box<Self>) -> Result<TxOffset, SnapshotError> {
        let tx_offset = self.tx_offset();
        UnflushedSnapshot::sync_all(*self)?;
        Ok(tx_offset)
    }
}

pub struct WritableSnapshot {
    parent: Option<ParentRepo>,
    inner: WritableSnapshotInner,
}

enum ParentRepo {
    V0(v0::SnapshotRepo),
    V1(v1::SnapshotRepo),
}

impl ParentRepo {
    fn file_path(&self, key: ObjectKey) -> PathBuf {
        match self {
            Self::V0(inner) => inner.file_path(key),
            Self::V1(inner) => inner.file_path(key),
        }
    }
}

enum WritableSnapshotInner {
    V0 {
        manifest: v0::Snapshot,
        repo: v0::SnapshotRepo,
    },
    V1 {
        manifest: v1::Snapshot,
        repo: v1::SnapshotRepo,
    },
}

impl WritableSnapshot {
    pub fn root(&self) -> &Path {
        match &self.inner {
            WritableSnapshotInner::V0 { repo, .. } => repo.root(),
            WritableSnapshotInner::V1 { repo, .. } => repo.root(),
        }
    }

    pub fn file_path(&self, key: ObjectKey) -> PathBuf {
        match &self.inner {
            WritableSnapshotInner::V0 { repo, .. } => repo.file_path(key),
            WritableSnapshotInner::V1 { repo, .. } => repo.file_path(key),
        }
    }

    pub fn tx_offset(&self) -> TxOffset {
        match &self.inner {
            WritableSnapshotInner::V0 { manifest, .. } => manifest.tx_offset,
            WritableSnapshotInner::V1 { manifest, .. } => manifest.tx_offset,
        }
    }

    pub fn manifest(&self) -> Snapshot {
        match &self.inner {
            WritableSnapshotInner::V0 { manifest, .. } => Snapshot::V0(manifest.clone()),
            WritableSnapshotInner::V1 { manifest, .. } => Snapshot::V1(manifest.clone()),
        }
    }

    pub fn file_paths(&self) -> impl Iterator<Item = PathBuf> {
        self.object_keys_and_file_paths().map(|(_, path)| path)
    }

    pub fn objects(&self) -> impl Iterator<Item = ObjectKey> {
        let (v0, v1) = match &self.inner {
            WritableSnapshotInner::V0 { manifest, .. } => (Some(manifest.objects()), None),
            WritableSnapshotInner::V1 { manifest, .. } => (None, Some(manifest.objects())),
        };

        v0.into_iter().flatten().chain(v1.into_iter().flatten())
    }

    fn object_keys_and_file_paths(&self) -> impl Iterator<Item = (ObjectKey, PathBuf)> {
        let (v0, v1) = match &self.inner {
            WritableSnapshotInner::V0 { manifest, repo } => {
                (Some(manifest.objects().map(|key| (key, repo.file_path(key)))), None)
            }
            WritableSnapshotInner::V1 { manifest, repo } => {
                (None, Some(manifest.objects().map(|key| (key, repo.file_path(key)))))
            }
        };

        v0.into_iter().flatten().chain(v1.into_iter().flatten())
    }

    fn commit(self) -> Result<(), SnapshotError> {
        match self.inner {
            WritableSnapshotInner::V0 { manifest, repo } => manifest.write_snapshot_file(&repo),
            WritableSnapshotInner::V1 { manifest, repo } => manifest.write_snapshot_file(&repo),
        }
    }

    fn reconstruct(
        &self,
        page_pool: &PagePool,
        metrics: &mut SnapshotReadMetrics,
    ) -> Result<(BlobMap, BTreeMap<TableId, TablePages>), SnapshotError> {
        match &self.inner {
            WritableSnapshotInner::V0 { manifest, repo } => {
                let blob_start = Instant::now();
                let blob_store = repo.reconstruct_blobs(manifest, &mut metrics.blob)?;
                metrics.blob.read_time = blob_start.elapsed();

                let page_start = Instant::now();
                let tables = repo.reconstruct_tables(manifest, page_pool, &mut metrics.page)?;
                metrics.page.read_time = page_start.elapsed();

                Ok((blob_store, tables))
            }
            WritableSnapshotInner::V1 { manifest, repo } => {
                let blob_start = Instant::now();
                let blob_store = repo.reconstruct_blobs(manifest, &mut metrics.blob)?;
                metrics.blob.read_time = blob_start.elapsed();

                let page_start = Instant::now();
                let tables = repo.reconstruct_tables(manifest, page_pool, &mut metrics.page)?;
                metrics.page.read_time = page_start.elapsed();

                Ok((blob_store, tables))
            }
        }
    }

    fn try_hardlink_from(&self, parent: &Self, key: ObjectKey) -> io::Result<bool> {
        let src = parent.file_path(key);
        if src.is_file() {
            let dst = self.file_path(key);
            fs::create_dir_all(dst.parent().expect("dir trie paths have a parent"))?;
            fs::hard_link(src, dst)?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn set_blobs(&mut self, blobs: impl Iterator<Item = BlobEntry>) {
        match &mut self.inner {
            WritableSnapshotInner::V0 { manifest, .. } => manifest.blobs.extend(blobs),
            WritableSnapshotInner::V1 { manifest, .. } => manifest.blobs_mut().extend(blobs),
        }
    }

    fn set_tables(&mut self, tables: impl Iterator<Item = TableEntry>) {
        match &mut self.inner {
            WritableSnapshotInner::V0 { manifest, .. } => manifest.tables.extend(tables),
            WritableSnapshotInner::V1 { manifest, .. } => manifest.tables_mut().extend(tables),
        }
    }
}

impl ObjectProvider for WritableSnapshot {
    type Reader = <DirTrie as ObjectProvider>::Reader;

    async fn object_reader(&self, key: ObjectKey) -> io::Result<Self::Reader> {
        match &self.inner {
            WritableSnapshotInner::V0 { repo, .. } => repo.object_reader(key).await,
            WritableSnapshotInner::V1 { repo, .. } => repo.object_reader(key).await,
        }
    }
}

impl SnapshotObjectStorage for WritableSnapshot {
    fn write(&self, object: tiered::SnapshotObject<'_>) -> io::Result<()> {
        let path = match &self.inner {
            WritableSnapshotInner::V0 { repo, .. } => repo.file_path(object.key()),
            WritableSnapshotInner::V1 { repo, .. } => repo.file_path(object.key()),
        };
        fs::create_dir_all(path.parent().expect("dir trie file paths have a parent"))?;

        // Avoid writing resident objects on each snapshot.
        if let Some((parent_repo, resident_object)) = self.parent.as_ref().zip(object.as_resident()) {
            let parent_path = parent_repo.file_path(resident_object.key());
            if parent_path.is_file() {
                fs::hard_link(&parent_path, &path)?;

                return Ok(());
            }
        }

        object.write(path, tiered::Fsync::Defer)
    }
}

#[derive(Clone)]
pub enum Snapshot {
    V0(v0::Snapshot),
    V1(v1::Snapshot),
}

impl Snapshot {
    /// Read a [`Snapshot`] from the file at `path`, verify its hash, and return it.
    ///
    /// **NOTE**: It detects if the file was compressed or not.
    ///
    /// Fails if:
    /// - `path` does not refer to a readable file.
    /// - Fails to check if is compressed or not.
    /// - The file at `path` is corrupted,
    ///   as detected by comparing the hash of its bytes to a hash recorded in the file.
    pub fn read_from_file(path: &SnapshotFilePath) -> Result<(Self, CompressType), SnapshotError> {
        read_from_file(path)
    }

    /// The transaction offset at which this snapshot was taken.
    pub fn tx_offset(&self) -> TxOffset {
        match self {
            Self::V0(inner) => inner.tx_offset,
            Self::V1(inner) => inner.tx_offset,
        }
    }

    /// The transaction offset at which this snapshot was taken, as a mutable borrow.
    ///
    /// NOTE: This is exposed for integration tests.
    pub fn tx_offset_mut(&mut self) -> &mut TxOffset {
        match self {
            Self::V0(inner) => &mut inner.tx_offset,
            Self::V1(inner) => inner.tx_offset_mut(),
        }
    }

    /// The total number of objects this snapshot references.
    pub fn total_objects(&self) -> usize {
        match self {
            Self::V0(inner) => inner.total_objects(),
            Self::V1(inner) => inner.total_objects(),
        }
    }

    fn magic(&self) -> [u8; 4] {
        match self {
            Self::V0(inner) => inner.magic,
            Self::V1(inner) => inner.magic,
        }
    }

    #[cfg(test)]
    fn version(&self) -> u8 {
        match self {
            Self::V0(inner) => inner.version,
            Self::V1(inner) => inner.version,
        }
    }

    fn database_identity(&self) -> Identity {
        match self {
            Self::V0(inner) => inner.database_identity,
            Self::V1(inner) => inner.database_identity,
        }
    }

    fn replica_id(&self) -> u64 {
        match self {
            Self::V0(inner) => inner.replica_id,
            Self::V1(inner) => inner.replica_id,
        }
    }

    fn module_abi_version(&self) -> [u16; 2] {
        match self {
            Self::V0(inner) => inner.module_abi_version,
            Self::V1(inner) => inner.module_abi_version,
        }
    }

    fn blobs(&self) -> &Vec<BlobEntry> {
        match self {
            Self::V0(inner) => &inner.blobs,
            Self::V1(inner) => &inner.blobs,
        }
    }

    fn tables(&self) -> &Vec<TableEntry> {
        match self {
            Self::V0(inner) => &inner.tables,
            Self::V1(inner) => &inner.tables,
        }
    }

    pub fn into_writable(self, dir: &SnapshotDirPath) -> Result<WritableSnapshot, SnapshotError> {
        match self {
            Self::V0(inner) => {
                let repo = inner.repo(dir)?;
                let parent = repo.parent()?;
                Ok(WritableSnapshot {
                    parent,
                    inner: WritableSnapshotInner::V0 { manifest: inner, repo },
                })
            }
            Self::V1(inner) => {
                let repo = inner.repo(dir)?;
                let parent = repo.parent()?;
                Ok(WritableSnapshot {
                    parent,
                    inner: WritableSnapshotInner::V1 { manifest: inner, repo },
                })
            }
        }
    }

    pub(crate) fn into_parent_repo(self, dir: &SnapshotDirPath) -> io::Result<ParentRepo> {
        match self {
            Self::V0(inner) => inner.repo(dir).map(ParentRepo::V0),
            Self::V1(inner) => inner.repo(dir).map(ParentRepo::V1),
        }
    }
}

impl Serialize for Snapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::V0(v0) => v0.serialize(serializer),
            Self::V1(v1) => v1.serialize(serializer),
        }
    }
}

/// Collect the size of the snapshot and the number of objects in it.
#[derive(Clone, Default)]
pub struct SnapshotSize {
    /// How many snapshots are in the snapshot directory, and what `CompressType` they are.
    pub snapshot: CompressCount,
    /// The size of the snapshot file in `bytes`.
    pub file_size: u64,
    /// The size of the snapshot's objects in `bytes`.
    pub object_size: u64,
    /// The number of objects in the snapshot.
    pub object_count: u64,
    /// Total size of the snapshot in `bytes`, `file_size + object_size`.
    pub total_size: u64,
}

impl Add for SnapshotSize {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            snapshot: CompressCount {
                none: self.snapshot.none + rhs.snapshot.none,
                zstd: self.snapshot.zstd + rhs.snapshot.zstd,
            },
            file_size: self.file_size + rhs.file_size,
            object_size: self.object_size + rhs.object_size,
            object_count: self.object_count + rhs.object_count,
            total_size: self.total_size + rhs.total_size,
        }
    }
}

impl AddAssign for SnapshotSize {
    fn add_assign(&mut self, rhs: Self) {
        *self = self.clone() + rhs;
    }
}

impl fmt::Debug for SnapshotSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnapshotSize")
            .field("snapshot       ", &self.snapshot)
            .field("object_count   ", &self.object_count)
            .field("file_size      ", &format_args!("{:>8} bytes", self.file_size))
            .field("object_size    ", &format_args!("{:>8} bytes", self.object_size))
            .field("total_size     ", &format_args!("{:>8} bytes", self.total_size))
            .finish()
    }
}

/// Number of objects compressed or hardlinked.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectCompressionStats {
    /// Number of objects freshly compressed.
    pub compressed: usize,
    /// Cumulative stats of the compressed objects.
    pub compression_stats: spacetimedb_fs_utils::compression::CompressionStats,
    /// Number of objects hardlinked from a parent repository.
    pub hardlinked: usize,
}

impl ObjectCompressionStats {
    fn is_zero(&self) -> bool {
        self.compressed == 0 && self.hardlinked == 0
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl AddAssign for ObjectCompressionStats {
    fn add_assign(
        &mut self,
        Self {
            compressed,
            compression_stats,
            hardlinked,
        }: Self,
    ) {
        self.compressed += compressed;
        self.compression_stats += compression_stats;
        self.hardlinked += hardlinked;
    }
}

/// Information about the progress of [compress_snapshots].
///
/// [compress_snapshots]: SnapshotRepository::compress_snapshots
#[derive(Default)]
pub struct CompressionStats {
    /// Incremented for each snapshot in the supplied range that
    /// is found to be already compressed.
    pub skipped: usize,
    /// Times to compress individual snapshots.
    ///
    /// Timings are only recorded for snapshots that were actually compressed
    /// during the [compress_snapshots] pass, not the `skipped` ones.
    ///
    /// That is, `compressed.len() + skipped` is the total number of visited
    /// snapshots during the pass.
    ///
    /// [compress_snapshots]: SnapshotRepository::compress_snapshots
    pub compression_timings: Vec<Duration>,
    /// The cumulative [ObjectCompressionStats] for all snapshots visited.
    pub objects: ObjectCompressionStats,
    /// The offset of the latest snapshot in the supplied range found to be
    /// compressed.
    ///
    /// Note that the snapshot may have been compressed already, or was
    /// compressed during the current [compress_snapshots] pass.
    ///
    /// If no snapshot was visited during the run, the value is left unchanged.
    ///
    /// [compress_snapshots]: SnapshotRepository::compress_snapshots
    pub last_compressed: Option<TxOffset>,
}

impl CompressionStats {
    /// Number of snapshots that were compressed (as opposed to `skipped`).
    pub fn compressed(&self) -> usize {
        self.compression_timings.len()
    }
}

/// A repository of snapshots of a particular database instance.
#[derive(Clone)]
pub struct SnapshotRepository {
    /// The directory which contains all the snapshots.
    root: SnapshotsPath,

    /// The database identity of the database instance for which this repository stores snapshots.
    database_identity: Identity,

    /// The database instance ID of the database instance for which this repository stores snapshots.
    replica_id: u64,
    // TODO(deduplication): track the most recent successful snapshot
    // (possibly in a file)
    // and hardlink its objects into the next snapshot for deduplication.
}

impl SnapshotRepository {
    /// Returns the [`Identity`] of the database this [`SnapshotRepository`] is configured to snapshot.
    pub fn database_identity(&self) -> Identity {
        self.database_identity
    }

    /// Capture a snapshot of the state of the database at `tx_offset`,
    /// where `tables` is the committed state of all the tables in the database,
    /// and `blobs` is the committed state's blob store.
    ///
    /// The returned [UnflushedSnapshot] is **not** durable -- call
    /// [UnflushedSnapshot::sync_all] to finalize it (see the struct docs for
    /// more details).
    ///
    /// Also note that the snapshot remains locked for as long as [UnflushedSnapshot]
    /// is alive. It will not appear in [Self::all_snapshots] nor can it be
    /// modified by methods in [SnapshotRepository].
    pub fn create_snapshot<'db>(
        &self,
        offset: TxOffset,
        storage: &TieredDatabaseStorage,
        tables: impl Iterator<Item = &'db mut Table>,
    ) -> Result<UnflushedSnapshot, SnapshotError> {
        self.invalidate_newer_snapshots(offset.saturating_sub(1))?;
        let snapshot_dir = self.snapshot_dir_path(offset);
        let lockfile = Lockfile::for_file(&snapshot_dir)?;
        snapshot_dir.create()?;

        let manifest = v0::Snapshot {
            magic: MAGIC,
            version: CURRENT_SNAPSHOT_VERSION,
            database_identity: self.database_identity,
            replica_id: self.replica_id,
            module_abi_version: CURRENT_MODULE_ABI_VERSION,
            tx_offset: offset,
            blobs: vec![],
            tables: vec![],
        };

        let manifest = match CURRENT_SNAPSHOT_VERSION {
            v0::SNAPSHOT_VERSION => Snapshot::V0(manifest),
            v1::SNAPSHOT_VERSION => Snapshot::V1(v1::Snapshot::new(manifest)),
            v => unreachable!("invalid CURRENT_SNAPSHOT_VERSION: {v}"),
        };
        let mut snapshot = manifest.into_writable(&snapshot_dir)?;
        let (blobs, tables) = storage.create_snapshot(&snapshot, tables)?.into_parts();
        snapshot.set_blobs(blobs.into_iter().map(|(hash, uses)| BlobEntry {
            hash,
            uses: uses.get() as u32,
        }));
        snapshot.set_tables(tables.into_iter().map(|(table_id, pages)| {
            TableEntry {
                table_id,
                pages: pages
                    .into_iter()
                    .map(|page| page.map(Into::into).unwrap_or(ZERO_HASH_DENOTING_ABSENT_PAGE))
                    .collect(),
            }
        }));

        Ok(UnflushedSnapshot::new(snapshot_dir, snapshot, lockfile))
    }

    /// Get the path to the directory which would contain the snapshot of transaction `tx_offset`.
    ///
    /// The directory may not exist if no snapshot has been taken of `tx_offset`.
    ///
    /// The directory may exist but be locked or incomplete
    /// if a file with the same name and the extension `.lock` exists.
    /// In this case, callers should treat the snapshot as if it did not exist.
    ///
    /// Use `[Self::all_snapshots]` to get `tx_offsets` which will return valid extant paths.
    /// `[Self::all_snapshots]` will never return a `tx_offset` for a locked or incomplete snapshot.
    /// `[Self::all_snapshots]` does not validate the contents of snapshots,
    /// so it may return a `tx_offset` whose snapshot is corrupted.
    ///
    /// Any mutations to any files contained in the returned directory
    /// will likely corrupt the snapshot,
    /// causing attempts to reconstruct it to fail.
    pub fn snapshot_dir_path(&self, tx_offset: TxOffset) -> SnapshotDirPath {
        self.root.snapshot_dir(tx_offset)
    }

    /// Read a snapshot contained in self referring to `tx_offset`,
    /// verify its hashes,
    /// and parse it into an in-memory structure [`ReconstructedSnapshot`]
    /// which can be used to build a `CommittedState`.
    ///
    /// This method cannot construct [`Table`] objects
    /// because doing so requires knowledge of the system tables' schemas
    /// to compute the schemas of the user-defined tables.
    ///
    /// Fails if:
    /// - No snapshot exists in `self` for `tx_offset`.
    /// - The snapshot is incomplete, as detected by its lockfile still existing.
    /// - Any object file (page or large blob) referenced by the snapshot file
    ///   is missing or corrupted,
    ///   as detected by comparing the hash of its bytes to the hash recorded in the snapshot file.
    /// - The snapshot file's magic number does not match [`MAGIC`].
    /// - The snapshot file's version is unsupported.
    ///
    /// The following conditions are not detected or considered as errors:
    /// - The snapshot file's database identity or instance ID do not match those in `self`.
    /// - The snapshot file's module ABI version does not match [`CURRENT_MODULE_ABI_VERSION`].
    /// - The snapshot file's recorded transaction offset does not match `tx_offset`.
    ///
    /// This means that callers must inspect the returned [`ReconstructedSnapshot`]
    /// and verify that they can handle its contained database identity, instance ID, module ABI version and transaction offset.
    pub fn read_snapshot(
        &self,
        tx_offset: TxOffset,
        page_pool: &PagePool,
    ) -> Result<ReconstructedSnapshot, SnapshotError> {
        let snapshot_dir = self.snapshot_dir_path(tx_offset);
        let lockfile = Lockfile::lock_path(&snapshot_dir);
        if lockfile.try_exists()? {
            return Err(SnapshotError::Incomplete { tx_offset, lockfile });
        }

        let snapshot_file_path = snapshot_dir.snapshot_file(tx_offset);
        let (snapshot, compress_type, metadata_metrics) = read_from_file_with_metrics(&snapshot_file_path)?;

        let mut read_metrics = SnapshotReadMetrics {
            metadata: metadata_metrics,
            ..Default::default()
        };
        read_metrics.blob.files = snapshot.blobs().len() as u64;
        read_metrics.page.files = snapshot
            .tables()
            .iter()
            .map(|table| table.present_pages().count() as u64)
            .sum();
        read_metrics.absent_pages = snapshot
            .tables()
            .iter()
            .map(|table| {
                table
                    .pages
                    .iter()
                    .filter(|page| **page == ZERO_HASH_DENOTING_ABSENT_PAGE)
                    .count() as u64
            })
            .sum();

        if snapshot.magic() != MAGIC {
            return Err(SnapshotError::BadMagic {
                tx_offset,
                magic: snapshot.magic(),
            });
        }

        let snapshot_dir = self.snapshot_dir_path(tx_offset);

        let database_identity = snapshot.database_identity();
        let replica_id = snapshot.replica_id();
        let tx_offset = snapshot.tx_offset();
        let module_abi_version = snapshot.module_abi_version();

        let repo = snapshot.into_writable(&snapshot_dir)?;
        let (blob_store, tables) = repo.reconstruct(page_pool, &mut read_metrics)?;

        Ok(ReconstructedSnapshot {
            database_identity,
            replica_id,
            tx_offset,
            module_abi_version,
            blob_store,
            tables,
            compress_type,
            read_metrics,
        })
    }

    /// Read the [`Snapshot`] metadata at `tx_offset` and verify the integrity
    /// of all objects it refers to.
    ///
    /// Fails if:
    ///
    /// - No snapshot exists in `self` for `tx_offset`
    /// - The snapshot is incomplete, as detected by its lockfile still existing.
    /// - The snapshot file's magic number does not match [`MAGIC`].
    /// - Any object file (page or large blob) referenced by the snapshot file
    ///   is missing or corrupted.
    ///
    /// The following conditions are not detected or considered as errors:
    ///
    /// - The snapshot file's version does not match [`CURRENT_SNAPSHOT_VERSION`].
    /// - The snapshot file's database identity or instance ID do not match
    ///   those in `self`.
    /// - The snapshot file's module ABI version does not match
    ///   [`CURRENT_MODULE_ABI_VERSION`].
    /// - The snapshot file's recorded transaction offset does not match
    ///   `tx_offset`.
    ///
    /// Callers may want to inspect the returned [`Snapshot`] and ensure its
    /// contents match their expectations.
    pub async fn verify_snapshot(&self, tx_offset: TxOffset) -> Result<Snapshot, SnapshotError> {
        let snapshot_dir = self.snapshot_dir_path(tx_offset);
        let manifest = spawn_blocking({
            let snapshot_dir = snapshot_dir.clone();
            move || {
                let lockfile = Lockfile::lock_path(&snapshot_dir);
                if lockfile.try_exists()? {
                    return Err(SnapshotError::Incomplete { tx_offset, lockfile });
                }

                let snapshot_file_path = snapshot_dir.snapshot_file(tx_offset);
                let (snapshot, _) = read_from_file(&snapshot_file_path)?;

                if snapshot.magic() != MAGIC {
                    return Err(SnapshotError::BadMagic {
                        tx_offset,
                        magic: snapshot.magic(),
                    });
                }
                Ok(snapshot)
            }
        })
        .await
        .unwrap()?;
        let snapshot = manifest.clone().into_writable(&snapshot_dir)?;
        verify_snapshot(snapshot, self.root.clone(), manifest.clone())
            .await
            .map(drop)?;
        Ok(manifest)
    }

    /// Open a repository at `root`, failing if the `root` doesn't exist or isn't a directory.
    ///
    /// Calls [`SnapshotsPath::is_dir`] and requires that the result is `true`.
    /// See that method for more detailed preconditions on this function.
    pub fn open(root: SnapshotsPath, database_identity: Identity, replica_id: u64) -> Result<Self, SnapshotError> {
        if !root.is_dir() {
            return Err(SnapshotError::NotDirectory { root });
        }
        Ok(Self {
            root,
            database_identity,
            replica_id,
        })
    }

    /// Return the `TxOffset` of the highest-offset complete snapshot in the repository
    /// lower than or equal to `upper_bound`.
    ///
    /// When searching for a snapshot to restore,
    /// we will pass the [`spacetimedb_durability::Durability::durable_tx_offset`]
    /// as the `upper_bound` to ensure we don't lose TXes.
    ///
    /// Does not verify that the snapshot of the returned `TxOffset` is valid and uncorrupted,
    /// so a subsequent [`Self::read_snapshot`] may fail.
    pub fn latest_snapshot_older_than(&self, upper_bound: TxOffset) -> Result<Option<TxOffset>, SnapshotError> {
        common::latest_snapshot_older_than(&self.root, upper_bound)
    }

    pub fn all_snapshots(&self) -> Result<impl Iterator<Item = TxOffset> + use<>, SnapshotError> {
        common::all_snapshots(&self.root)
    }

    /// Return an interator of [`ArchivedSnapshotDirPath`] for all the archived snapshot directories on disk
    pub fn all_archived_snapshots(
        &self,
    ) -> Result<impl Iterator<Item = ArchivedSnapshotDirPath> + use<>, SnapshotError> {
        Ok(self
            .root
            // Item = Result<DirEntry>
            .read_dir()?
            // Item = DirEntry
            .filter_map(Result::ok)
            // Item = PathBuf
            .map(|dirent| dirent.path())
            // Ignore entries not shaped like snapshot directories.
            .filter(|path| path.extension() == Some(OsStr::new(ARCHIVED_SNAPSHOT_EXT)))
            // Item = ArchivedSnapshotDirPath
            .map(ArchivedSnapshotDirPath::from_path_unchecked))
    }

    /// Delete an archived snapshot from disk
    pub fn remove_archived_snapshot(path: &ArchivedSnapshotDirPath) -> Result<(), SnapshotError> {
        fs::remove_dir_all(path).map_err(SnapshotError::Io)
    }

    /// Return the `TxOffset` of the highest-offset complete snapshot in the repository.
    ///
    /// Does not verify that the snapshot of the returned `TxOffset` is valid and uncorrupted,
    /// so a subsequent [`Self::read_snapshot`] may fail.
    pub fn latest_snapshot(&self) -> Result<Option<TxOffset>, SnapshotError> {
        self.latest_snapshot_older_than(TxOffset::MAX)
    }

    /// Rename any snapshot newer than `upper_bound` with [`INVALID_SNAPSHOT_DIR_EXT`].
    ///
    /// When rebuilding a database, we will call this method
    /// with the [`spacetimedb_durability::Durability::durable_tx_offset`] as the `upper_bound`
    /// in order to prevent us from retaining snapshots which will be superseded by the new diverging history.
    ///
    /// It is also called when creating a new snapshot via [`Self::create_snapshot`]
    /// in order to prevent a diverging snapshot from being used as its own parent.
    ///
    /// Does not invalidate snapshots which are locked.
    ///
    /// This may overwrite previously-invalidated snapshots.
    ///
    /// If this method returns an error, some snapshots may have been invalidated, but not all will have been.
    pub fn invalidate_newer_snapshots(&self, upper_bound: TxOffset) -> Result<(), SnapshotError> {
        let newer_snapshots = self
            .all_snapshots()?
            .filter(|tx_offset| *tx_offset > upper_bound)
            // Collect to a vec to avoid iterator invalidation,
            // as the subsequent `for` loop will mutate the directory.
            .collect::<Vec<TxOffset>>();

        for newer_snapshot in newer_snapshots {
            self.invalidate_snapshot(newer_snapshot)?;
        }
        Ok(())
    }

    /// Mark a single snapshot invalid so it will not be considered for future
    /// restores.
    pub fn invalidate_snapshot(&self, tx_offset: TxOffset) -> Result<(), SnapshotError> {
        let path = self.snapshot_dir_path(tx_offset);
        log::info!("Renaming snapshot {tx_offset} from {path:?} to invalid");
        path.rename_invalid().map_err(Into::into)
    }

    /// Compress the `current` snapshot, unless it is already compressed.
    ///
    /// If a `parent` snapshot is given, its object repo will be used to
    /// hardlink common objects and avoid re-compressing them:
    ///
    /// If an object in `current` is uncompressed, but exists in `parent` and
    /// is compressed, a hardlink is created in `current`. Otherwise, the object
    /// in `current` is compressed in place.
    ///
    /// The `parent`'s object repo is never modified.
    ///
    /// Returns [ObjectCompressionStats] with information about how many objects
    /// were compressed and hardlinked, respectively.
    fn compress_snapshot(
        parent: Option<&(TxOffset, SnapshotDirPath)>,
        current: &(TxOffset, SnapshotDirPath),
    ) -> Result<ObjectCompressionStats, SnapshotError> {
        let (tx_offset, snapshot_dir) = current;
        let tx_offset = *tx_offset;
        let snapshot_file_path = snapshot_dir.snapshot_file(tx_offset);
        let (manifest, compress_type) = read_from_file(&snapshot_file_path)?;
        let replica_id = manifest.replica_id();

        let mut stats = ObjectCompressionStats::default();
        if let Some(algo) = compress_type.algorithm() {
            log::debug!(
                "Snapshot {snapshot_dir:?} of replica {} is already compressed: {algo:?}",
                manifest.replica_id()
            );
            return Ok(stats);
        }

        let old = if let Some((tx_offset, snapshot_dir)) = parent {
            let snapshot_file_path = snapshot_dir.snapshot_file(*tx_offset);
            let (manifest, _) = read_from_file(&snapshot_file_path)?;
            let snapshot = manifest.into_writable(snapshot_dir)?;
            // TODO: Use no-hasher map with ObjectKey directly.
            snapshot
                .object_keys_and_file_paths()
                .map(|(key, path)| (blake3::Hash::from(key), path))
                .collect()
        } else {
            HashMap::new()
        };

        // Replace the original file with the compressed one.
        fn compress(
            old: &HashMap<blake3::Hash, PathBuf>,
            src: &PathBuf,
            hash: Option<blake3::Hash>,
            stats: Option<&mut ObjectCompressionStats>,
        ) -> Result<(), SnapshotError> {
            let read = CompressReader::new(o_rdonly().open(src)?)?;
            if read.is_compressed() {
                return Ok(()); // Already compressed
            }
            if let Some(hash) = hash
                && let Some(old_path) = old.get(&hash)
            {
                let old_file = CompressReader::new(o_rdonly().open(old_path)?)?;
                if old_file.is_compressed() {
                    std::fs::hard_link(old_path, src.with_extension("_tmp"))?;
                    std::fs::rename(src.with_extension("_tmp"), src)?;
                    File::open(src.parent().unwrap())?.sync_all()?;
                    if let Some(stats) = stats {
                        stats.hardlinked += 1;
                    }
                    return Ok(());
                }
            }

            let dst = src.with_extension("_tmp");
            let mut write = BufWriter::new(o_excl().open(&dst)?);
            // The default frame size compress better.
            let compression_stats = compress_with_zstd(read, &mut write, None)?;
            std::fs::rename(dst, src)?;
            if let Some(stats) = stats {
                stats.compressed += 1;
                stats.compression_stats += compression_stats;
            }

            Ok(())
        }

        let _lock = Lockfile::for_file(snapshot_dir)?;

        log::info!("Compressing snapshot {snapshot_dir:?} of replica {replica_id}",);

        let snapshot = manifest.into_writable(snapshot_dir)?;
        for (hash, path) in snapshot.object_keys_and_file_paths() {
            compress(&old, &path, Some(hash.into()), Some(&mut stats)).inspect_err(|err| {
                log::error!("Failed to compress object file {path:?}: {err}");
            })?;
        }
        File::open(snapshot.root())?.sync_all()?;

        // Compress the snapshot file last,
        // which marks the whole snapshot as compressed.
        //
        // Don't update the stats for the snapshot file.
        compress(&old, &snapshot_file_path.0, None, None).inspect_err(|err| {
            log::error!("Failed to compress snapshot file {snapshot_file_path:?}: {err}");
        })?;
        File::open(&snapshot_dir.0)?.sync_all()?;

        log::info!("Compressed snapshot {snapshot_dir:?} of replica {replica_id}: {compress_type:?}",);
        Ok(stats)
    }

    /// Attempt to compress all snapshots that fall into `range`, and record
    /// the outcome in `stats`.
    ///
    /// The snapshots in `range` are traversed in ascending order.
    /// If an error occurs, processing stops and the error is returned.
    ///
    /// See [CompressionStats] for how to interpret the results.
    pub fn compress_snapshots(
        &self,
        stats: &mut CompressionStats,
        range: impl RangeBounds<TxOffset>,
    ) -> Result<(), SnapshotError> {
        let mut snapshots = self
            .all_snapshots()?
            .filter(|offset| range.contains(offset))
            .map(|offset| (offset, self.snapshot_dir_path(offset)))
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|&(offset, _)| offset);

        let mut previous = None;
        for current in &snapshots {
            let start = Instant::now();
            let object_stats = Self::compress_snapshot(previous, current)?;
            if object_stats.is_zero() {
                stats.skipped += 1;
            } else {
                stats.compression_timings.push(start.elapsed());
            }
            stats.objects += object_stats;
            stats.last_compressed = Some(current.0);
            previous = Some(current);
        }

        Ok(())
    }

    /// Calculate the size of the snapshot repository in bytes.
    pub fn size_on_disk(&self) -> Result<SnapshotSize, SnapshotError> {
        let mut size = SnapshotSize::default();

        for snapshot in self.all_snapshots()? {
            size += self.size_on_disk_snapshot(snapshot)?;
        }
        Ok(size)
    }

    pub fn size_on_disk_snapshot(&self, offset: TxOffset) -> Result<SnapshotSize, SnapshotError> {
        let mut size = SnapshotSize::default();

        let snapshot_dir = self.snapshot_dir_path(offset);
        let snapshot_file_path = snapshot_dir.snapshot_file(offset);
        let snapshot_file_size = snapshot_file_path.metadata()?.len();

        let (manifest, compress_type) = read_from_file(&snapshot_file_path)?;

        size.snapshot = match compress_type {
            CompressType::None => CompressCount { none: 1, zstd: 0 },
            CompressType::Algorithm(CompressionAlgorithm::Zstd) => CompressCount { none: 0, zstd: 1 },
        };

        size.file_size += snapshot_file_size;
        size.total_size += snapshot_file_size;
        let snapshot = manifest.into_writable(&snapshot_dir)?;
        for path in snapshot.file_paths() {
            let file_size = path.metadata()?.len();
            size.object_size += file_size;
            size.total_size += file_size;
            size.object_count += 1;
        }

        Ok(size)
    }
}

/// Snapshot storage backend.
pub trait SnapshotRepo: Send + Sync {
    type Pending: PendingSnapshot;

    /// Return the database identity associated with this snapshot backend.
    fn database_identity(&self) -> Identity;

    /// Start creating a snapshot at `tx_offset` from the provided tables and blob store.
    fn create_snapshot(
        &self,
        tx_offset: TxOffset,
        storage: &TieredDatabaseStorage,
        tables: &mut dyn Iterator<Item = &mut Table>,
    ) -> Result<Self::Pending, SnapshotError>;

    /// Reconstruct the snapshot at `tx_offset` using the supplied page pool.
    fn read_snapshot(&self, tx_offset: TxOffset, page_pool: &PagePool) -> Result<ReconstructedSnapshot, SnapshotError>;

    /// Return the latest snapshot at or before `upper_bound`, if one exists.
    fn latest_snapshot_older_than(&self, upper_bound: TxOffset) -> Result<Option<TxOffset>, SnapshotError>;

    /// Return the latest snapshot in this backend, if one exists.
    fn latest_snapshot(&self) -> Result<Option<TxOffset>, SnapshotError> {
        self.latest_snapshot_older_than(TxOffset::MAX)
    }

    /// Attempt to compress all snapshots that fall into `range`, and record
    /// the outcome in `stats`.
    ///
    /// The snapshots in `range` are traversed in ascending order.
    /// If an error occurs, processing stops and the error is returned.
    ///
    /// See [CompressionStats] for how to interpret the results.
    fn compress_snapshots(&self, stats: &mut CompressionStats, range: Range<TxOffset>) -> Result<(), SnapshotError>;

    /// Invalidate every snapshot newer than `upper_bound`.
    fn invalidate_newer_snapshots(&self, upper_bound: TxOffset) -> Result<(), SnapshotError>;

    /// Invalidate the snapshot at `tx_offset`.
    fn invalidate_snapshot(&self, tx_offset: TxOffset) -> Result<(), SnapshotError>;
}

impl SnapshotRepo for SnapshotRepository {
    type Pending = BoxedPendingSnapshot;

    fn database_identity(&self) -> Identity {
        SnapshotRepository::database_identity(self)
    }

    fn create_snapshot(
        &self,
        tx_offset: TxOffset,
        storage: &TieredDatabaseStorage,
        tables: &mut dyn Iterator<Item = &mut Table>,
    ) -> Result<Self::Pending, SnapshotError> {
        Ok(Box::new(SnapshotRepository::create_snapshot(
            self, tx_offset, storage, tables,
        )?))
    }

    fn read_snapshot(&self, tx_offset: TxOffset, page_pool: &PagePool) -> Result<ReconstructedSnapshot, SnapshotError> {
        SnapshotRepository::read_snapshot(self, tx_offset, page_pool)
    }

    fn latest_snapshot_older_than(&self, upper_bound: TxOffset) -> Result<Option<TxOffset>, SnapshotError> {
        SnapshotRepository::latest_snapshot_older_than(self, upper_bound)
    }

    fn latest_snapshot(&self) -> Result<Option<TxOffset>, SnapshotError> {
        SnapshotRepository::latest_snapshot(self)
    }

    fn compress_snapshots(&self, stats: &mut CompressionStats, range: Range<TxOffset>) -> Result<(), SnapshotError> {
        SnapshotRepository::compress_snapshots(self, stats, range)
    }

    fn invalidate_newer_snapshots(&self, upper_bound: TxOffset) -> Result<(), SnapshotError> {
        SnapshotRepository::invalidate_newer_snapshots(self, upper_bound)
    }

    fn invalidate_snapshot(&self, tx_offset: TxOffset) -> Result<(), SnapshotError> {
        SnapshotRepository::invalidate_snapshot(self, tx_offset)
    }
}

pub struct ReconstructedSnapshot {
    /// The identity of the snapshotted database.
    pub database_identity: Identity,
    /// The instance ID of the snapshotted database.
    pub replica_id: u64,
    /// The transaction offset of the state this snapshot reflects.
    pub tx_offset: TxOffset,
    /// ABI version of the module from which this snapshot was created, as [MAJOR, MINOR].
    pub module_abi_version: [u16; 2],

    /// The blob store of the snapshotted state.
    pub blob_store: BlobMap,

    /// All the tables from the snapshotted state, sans schema information and indexes.
    ///
    /// This includes the system tables,
    /// so the schema of user-defined tables can be recovered
    /// given knowledge of the schema of `st_table` and `st_column`.
    pub tables: BTreeMap<TableId, TablePages>,
    /// If the snapshot was compressed or not.
    pub compress_type: CompressType,
    /// Metrics collected while reading this snapshot from disk.
    pub read_metrics: SnapshotReadMetrics,
}

/// A [Path] statically known to point to either a file or a directory.
pub enum FileOrDirPath<'a> {
    File(&'a Path),
    Dir(&'a Path),
}

impl FileOrDirPath<'_> {
    /// `fsync` the file or directory at path `self`.
    ///
    /// On *nix systems, both a file and its enclosing directory should be
    /// `fsync`ed to make the file durable.
    ///
    /// On Windows, only the file needs to be synced, and it's even an error to
    /// sync a directory. Passing in [Self::Dir] is thus a no-op on Windows.
    pub fn sync_all(&self) -> io::Result<()> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Dir(path) => Ok(()),
            #[cfg(not(target_os = "windows"))]
            Self::Dir(path) => File::open(path)
                .map_err(|e| {
                    io::Error::new(
                        e.kind(),
                        format!("failed to open directory {} for fsync: {}", path.display(), e),
                    )
                })?
                .sync_all()
                .map_err(|e| io::Error::new(e.kind(), format!("failed to fsync directory {}: {}", path.display(), e))),
            Self::File(path) => {
                File::options()
                    .read(true)
                    // Windows needs the file to be writable for `sync_all` to work.
                    // Set all the open options explicitly, just for visibility.
                    .write(true)
                    .truncate(false)
                    .create(false)
                    .append(false)
                    .open(path)
                    .map_err(|e| {
                        io::Error::new(
                            e.kind(),
                            format!("failed to open file {} for fsync: {}", path.display(), e),
                        )
                    })?
                    .sync_all()
                    .map_err(|e| io::Error::new(e.kind(), format!("failed to fsync file {}: {}", path.display(), e)))
            }
        }
    }
}

#[cfg(test)]
mod tests;
