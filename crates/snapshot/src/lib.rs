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

use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::compression::{
    compress_with_zstd, CompressCount, CompressReader, CompressType, CompressionAlgorithm,
};
use spacetimedb_fs_utils::{
    dir_trie::{o_excl, o_rdonly, CountCreated, DirTrie},
    lockfile::{Lockfile, LockfileError},
};
use spacetimedb_lib::Identity;
use spacetimedb_paths::server::{ArchivedSnapshotDirPath, SnapshotDirPath, SnapshotFilePath, SnapshotsPath};
use spacetimedb_paths::FromPathUnchecked;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{bsatn, de::Deserialize, ser::Serialize};
use spacetimedb_table::{
    blob_store::{BlobHash, BlobStore, HashMapBlobStore},
    page::Page,
    page_pool::PagePool,
    table::Table,
};
use std::fs;
use std::ops::RangeBounds;
use std::time::{Duration, Instant};
use std::{
    collections::BTreeMap,
    collections::HashMap,
    ffi::OsStr,
    fmt,
    io::{BufWriter, Read, Write},
    ops::{Add, AddAssign},
    path::PathBuf,
};
use tokio::task::spawn_blocking;

pub mod remote;
use remote::verify_snapshot;

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
}

/// Magic number for snapshot files: a point in spacetime.
///
/// Chosen because the commitlog magic number is a spacetime interval,
/// so a snapshot should be a point to which an interval can be applied.
pub const MAGIC: [u8; 4] = *b"txyz";

/// Snapshot format version number.
pub const CURRENT_SNAPSHOT_VERSION: u8 = 0;

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

#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// A magic number: must be equal to [`MAGIC`].
    magic: [u8; 4],

    /// The snapshot version number. Must be equal to [`CURRENT_SNAPSHOT_VERSION`].
    version: u8,

    /// The identity of the snapshotted database.
    pub database_identity: Identity,
    /// The instance ID of the snapshotted database.
    pub replica_id: u64,

    /// ABI version of the module from which this snapshot was created, as [MAJOR, MINOR].
    ///
    /// As of this proposal, [7, 0].
    module_abi_version: [u16; 2],

    /// The transaction offset of the state this snapshot reflects.
    pub tx_offset: TxOffset,

    /// The hashes and reference counts of all objects in the blob store.
    blobs: Vec<BlobEntry>,

    /// For each table, its table ID followed by the hashes of all resident pages.
    ///
    /// It's necessary to store the table ID rather than relying on order
    /// because table IDs may be sparse (and will be, once we reserve a bunch of system table ids).
    tables: Vec<TableEntry>,
}

impl Snapshot {
    /// Insert a single large blob from a [`BlobStore`]
    /// into the in-memory snapshot `self`
    /// and the on-disk object repository `object_repo`.
    ///
    /// If the `prev_snapshot` is supplied, this method will attempt to hardlink the blob's on-disk object
    /// from that previous snapshot into `object_repo` rather than creating a fresh object.
    fn write_blob(
        &mut self,
        object_repo: &DirTrie,
        hash: &BlobHash,
        uses: usize,
        blob: &[u8],
        prev_snapshot: Option<&DirTrie>,
        counter: &mut CountCreated,
    ) -> Result<(), SnapshotError> {
        object_repo
            .hardlink_or_write(prev_snapshot, &hash.data, || blob, counter)
            .map_err(|cause| SnapshotError::WriteObject {
                ty: ObjectType::Blob(*hash),
                dest_repo: object_repo.root().to_path_buf(),
                source_repo: prev_snapshot.map(|dest_repo| dest_repo.root().to_path_buf()),
                cause,
            })?;
        self.blobs.push(BlobEntry {
            hash: *hash,
            uses: uses as u32,
        });
        Ok(())
    }

    /// Populate the in-memory snapshot `self`,
    /// and the on-disk object repository `object_repo`,
    /// with all large blobs from `blobs`.
    fn write_all_blobs(
        &mut self,
        object_repo: &DirTrie,
        blobs: &dyn BlobStore,
        prev_snapshot: Option<&DirTrie>,
        counter: &mut CountCreated,
    ) -> Result<(), SnapshotError> {
        for (hash, uses, blob) in blobs.iter_blobs() {
            self.write_blob(object_repo, hash, uses, blob, prev_snapshot, counter)?;
        }
        Ok(())
    }

    /// Write a single `page` into the on-disk object repository `object_repo`.
    ///
    /// `hash` must be the content hash of `page`, and must be stored in `page.unmodified_hash()`.
    ///
    /// Returns the `hash` for convenient use with [`Iter::map`] in [`Self::write_table`].
    ///
    /// If the `prev_snapshot` is supplied, this function will attempt to hardlink the page's on-disk object
    /// from that previous snapshot into `object_repo` rather than creating a fresh object.
    fn write_page(
        object_repo: &DirTrie,
        page: &Page,
        hash: blake3::Hash,
        prev_snapshot: Option<&DirTrie>,
        counter: &mut CountCreated,
    ) -> Result<blake3::Hash, SnapshotError> {
        debug_assert!(page.unmodified_hash().copied() == Some(hash));

        object_repo
            .hardlink_or_write(prev_snapshot, hash.as_bytes(), || bsatn::to_vec(page).unwrap(), counter)
            .map_err(|cause| SnapshotError::WriteObject {
                ty: ObjectType::Page(hash),
                dest_repo: object_repo.root().to_path_buf(),
                source_repo: prev_snapshot.map(|source_repo| source_repo.root().to_path_buf()),
                cause,
            })?;

        Ok(hash)
    }

    /// Populate the in-memory snapshot `self`,
    /// and the on-disk object repository `object_repo`,
    /// with all pages from `table`.
    fn write_table(
        &mut self,
        object_repo: &DirTrie,
        table: &mut Table,
        prev_snapshot: Option<&DirTrie>,
        counter: &mut CountCreated,
    ) -> Result<(), SnapshotError> {
        let pages = table
            .iter_pages_with_hashes()
            .map(|(hash, page)| Self::write_page(object_repo, page, hash, prev_snapshot, counter))
            .collect::<Result<Vec<blake3::Hash>, SnapshotError>>()?;

        self.tables.push(TableEntry {
            table_id: table.schema.table_id,
            pages,
        });
        Ok(())
    }

    /// Populate the in-memory snapshot `self`,
    /// and the on-disk object repository `object_repo`,
    /// with all pages from all tables in `tables`.
    fn write_all_tables<'db>(
        &mut self,
        object_repo: &DirTrie,
        tables: impl Iterator<Item = &'db mut Table>,
        prev_snapshot: Option<&DirTrie>,
        counter: &mut CountCreated,
    ) -> Result<(), SnapshotError> {
        for table in tables {
            self.write_table(object_repo, table, prev_snapshot, counter)?;
        }
        Ok(())
    }

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
        let err_read_object = |cause| SnapshotError::ReadObject {
            ty: ObjectType::Snapshot,
            source_repo: path.0.clone(),
            cause,
        };
        let snapshot_file = path.open_file(&o_rdonly()).map_err(err_read_object)?;
        let mut snapshot_file = CompressReader::new(snapshot_file)?;

        // The snapshot file is prefixed with the hash of the `Snapshot`'s BSATN.
        // Read that hash.
        let mut hash = [0; blake3::OUT_LEN];
        snapshot_file.read_exact(&mut hash).map_err(err_read_object)?;
        let hash = blake3::Hash::from_bytes(hash);

        // Read the `Snapshot`'s BSATN and compute its hash.
        let mut snapshot_bsatn = vec![];
        snapshot_file
            .read_to_end(&mut snapshot_bsatn)
            .map_err(err_read_object)?;
        let computed_hash = blake3::hash(&snapshot_bsatn);

        // Compare the saved and computed hashes, and fail if they do not match.
        if hash != computed_hash {
            return Err(SnapshotError::HashMismatch {
                ty: ObjectType::Snapshot,
                expected: *hash.as_bytes(),
                computed: *computed_hash.as_bytes(),
                source_repo: path.0.clone(),
            });
        }

        let snapshot = bsatn::from_slice::<Snapshot>(&snapshot_bsatn).map_err(|cause| SnapshotError::Deserialize {
            ty: ObjectType::Snapshot,
            source_repo: path.0.clone(),
            cause,
        })?;

        Ok((snapshot, snapshot_file.compress_type()))
    }

    /// Construct a [`HashMapBlobStore`] containing all the blobs referenced in `self`,
    /// reading their data from files in the `object_repo`.
    ///
    /// Fails if any of the object files is missing or corrupted,
    /// as detected by comparing the hash of its bytes to the hash recorded in `self`.
    fn reconstruct_blob_store(&self, object_repo: &DirTrie) -> Result<HashMapBlobStore, SnapshotError> {
        let mut blob_store = HashMapBlobStore::default();

        for BlobEntry { hash, uses } in &self.blobs {
            // Read the bytes of the blob object.
            let buf = object_repo
                .read_entry(&hash.data)
                .map_err(|cause| SnapshotError::ReadObject {
                    ty: ObjectType::Blob(*hash),
                    source_repo: object_repo.root().to_path_buf(),
                    cause,
                })?;

            // Compute the blob's hash.
            let computed_hash = BlobHash::hash_from_bytes(&buf);

            // Compare the computed hash to the one recorded in the `Snapshot`,
            // and fail if they do not match.
            if *hash != computed_hash {
                return Err(SnapshotError::HashMismatch {
                    ty: ObjectType::Blob(*hash),
                    expected: hash.data,
                    computed: computed_hash.data,
                    source_repo: object_repo.root().to_path_buf(),
                });
            }

            blob_store.insert_with_uses(hash, *uses as usize, buf.into_boxed_slice());
        }

        Ok(blob_store)
    }

    /// Read all the pages referenced by `pages` from the `object_repo`.
    ///
    /// Fails if any of the pages files is missing or corrupted,
    /// as detected by comparing the hash of its bytes to the hash listed in `pages`.
    fn reconstruct_one_table_pages(
        object_repo: &DirTrie,
        pages: &[blake3::Hash],
        page_pool: &PagePool,
    ) -> Result<Vec<Box<Page>>, SnapshotError> {
        pages
            .iter()
            .map(|hash| {
                // Read the BSATN bytes of the on-disk page object.
                let buf = object_repo
                    .read_entry(hash.as_bytes())
                    .map_err(|cause| SnapshotError::ReadObject {
                        ty: ObjectType::Page(*hash),
                        source_repo: object_repo.root().to_path_buf(),
                        cause,
                    })?;

                // Deserialize the bytes into a `Page`.
                let page = page_pool.take_deserialize_from(&buf);
                let page = page.map_err(|cause| SnapshotError::Deserialize {
                    ty: ObjectType::Page(*hash),
                    source_repo: object_repo.root().to_path_buf(),
                    cause,
                })?;

                // Compute the hash of the page.
                let computed_hash = page.content_hash();

                // Compare the computed hash to the one recorded in the `Snapshot`,
                // and fail if they do not match.
                if *hash != computed_hash {
                    return Err(SnapshotError::HashMismatch {
                        ty: ObjectType::Page(*hash),
                        expected: *hash.as_bytes(),
                        computed: *computed_hash.as_bytes(),
                        source_repo: object_repo.root().to_path_buf(),
                    });
                }

                Ok::<Box<Page>, SnapshotError>(page)
            })
            .collect()
    }

    fn reconstruct_one_table(
        object_repo: &DirTrie,
        TableEntry { table_id, pages }: &TableEntry,
        page_pool: &PagePool,
    ) -> Result<(TableId, Vec<Box<Page>>), SnapshotError> {
        Ok((
            *table_id,
            Self::reconstruct_one_table_pages(object_repo, pages, page_pool)?,
        ))
    }

    /// Reconstruct all the table data from `self`,
    /// reading pages from files in the `object_repo`.
    ///
    /// This method cannot construct [`Table`] objects
    /// because doing so requires knowledge of the system tables' schemas
    /// to compute the schemas of the user-defined tables
    ///
    /// Fails if any object file referenced in `self` (as a page or large blob)
    /// is missing or corrupted,
    /// as detected by comparing the hash of its bytes to the hash recorded in `self`.
    fn reconstruct_tables(
        &self,
        object_repo: &DirTrie,
        page_pool: &PagePool,
    ) -> Result<BTreeMap<TableId, Vec<Box<Page>>>, SnapshotError> {
        self.tables
            .iter()
            .map(|tbl| Self::reconstruct_one_table(object_repo, tbl, page_pool))
            .collect()
    }

    /// The number of objects in this snapshot, both blobs and pages.
    pub fn total_objects(&self) -> usize {
        self.blobs.len() + self.tables.iter().map(|table| table.pages.len()).sum::<usize>()
    }

    /// Obtain an iterator over the [`blake3::Hash`]es of all objects
    /// this snapshot is referring to.
    pub fn objects(&self) -> impl Iterator<Item = blake3::Hash> + '_ {
        self.blobs
            .iter()
            .map(|b| blake3::Hash::from_bytes(b.hash.data))
            .chain(self.tables.iter().flat_map(|t| t.pages.iter().copied()))
    }

    /// Obtain an iterator over the [`PathBuf`]s of all objects
    pub fn files<'a>(&'a self, src_repo: &'a DirTrie) -> impl Iterator<Item = (blake3::Hash, PathBuf)> + 'a {
        self.objects().map(move |hash| {
            let path = src_repo.file_path(hash.as_bytes());
            (hash, path)
        })
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
    fn add_assign(&mut self, Self { compressed, hardlinked }: Self) {
        self.compressed += compressed;
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
    /// Returns the path of the newly-created snapshot directory.
    ///
    /// **NOTE**: The current snapshot is uncompressed to avoid the potential slowdown.
    pub fn create_snapshot<'db>(
        &self,
        tables: impl Iterator<Item = &'db mut Table>,
        blobs: &'db dyn BlobStore,
        tx_offset: TxOffset,
    ) -> Result<SnapshotDirPath, SnapshotError> {
        // Invalidate equal to or newer than `tx_offset`.
        //
        // This is because snapshots don't currently track the epoch in which
        // they were created:
        //
        // Say, for example, a snapshot was created at offset 10, then a leader
        // failover causes the commitlog to be reset to offset 9. The next
        // transaction (also offset 10) will trigger snapshot creation, but we'd
        // mistake the existing snapshot (now invalid) as the previous snapshot.
        self.invalidate_newer_snapshots(tx_offset.saturating_sub(1))?;

        // If a previous snapshot exists in this snapshot repo,
        // get a handle on its object repo in order to hardlink shared objects into the new snapshot.
        let prev_snapshot = self.latest_snapshot()?.map(|offset| self.snapshot_dir_path(offset));

        let prev_snapshot = if let Some(prev_snapshot) = prev_snapshot {
            assert!(
                prev_snapshot.0.is_dir(),
                "prev_snapshot {prev_snapshot:?} is not a directory"
            );
            let object_repo = Self::object_repo(&prev_snapshot)?;
            Some(object_repo)
        } else {
            None
        };

        let mut counter = CountCreated::default();

        let snapshot_dir = self.snapshot_dir_path(tx_offset);

        // Before performing any observable operations,
        // acquire a lockfile on the snapshot you want to create.
        // Because we could be compressing the snapshot.
        let _lock = Lockfile::for_file(&snapshot_dir)?;

        // Create the snapshot directory.
        snapshot_dir.create()?;

        // Create a new `DirTrie` to hold all the content-addressed objects in the snapshot.
        let object_repo = Self::object_repo(&snapshot_dir)?;

        // Build the in-memory `Snapshot` object.
        let mut snapshot = self.empty_snapshot(tx_offset);

        // Populate both the in-memory `Snapshot` object and the on-disk object repository
        // with all the blobs and pages.
        snapshot.write_all_blobs(&object_repo, blobs, prev_snapshot.as_ref(), &mut counter)?;
        snapshot.write_all_tables(&object_repo, tables, prev_snapshot.as_ref(), &mut counter)?;

        self.write_snapshot_file(&snapshot_dir, snapshot)?;

        log::info!(
            "[{}] SNAPSHOT {:0>20}: Hardlinked {} objects and wrote {} objects",
            self.database_identity,
            tx_offset,
            counter.objects_hardlinked,
            counter.objects_written,
        );

        // Success! return the directory of the newly-created snapshot.
        // The lockfile will be dropped here.
        Ok(snapshot_dir)
    }

    /// Write the on-disk snapshot file containing the BSATN-encoded `snapshot`
    /// into `snapshot_dir`.
    ///
    /// It is not recommanded to call this method directly; prefer using `create_snapshot`.
    /// It is exposed publicly to be used in very specific scenarios, like modifying an existing
    /// snapshot.
    pub fn write_snapshot_file(&self, snapshot_dir: &SnapshotDirPath, snapshot: Snapshot) -> Result<(), SnapshotError> {
        // Serialize and hash the in-memory `Snapshot` object.
        let snapshot_bsatn = bsatn::to_vec(&snapshot).map_err(|cause| SnapshotError::Serialize {
            ty: ObjectType::Snapshot,
            cause,
        })?;
        let hash = blake3::hash(&snapshot_bsatn);

        // Create the snapshot file, containing first the hash, then the `Snapshot`.
        {
            let mut snapshot_file =
                BufWriter::new(snapshot_dir.snapshot_file(snapshot.tx_offset).open_file(&o_excl())?);
            snapshot_file.write_all(hash.as_bytes())?;
            snapshot_file.write_all(&snapshot_bsatn)?;
            snapshot_file.flush()?;
        }

        Ok(())
    }

    fn empty_snapshot(&self, tx_offset: TxOffset) -> Snapshot {
        Snapshot {
            magic: MAGIC,
            version: CURRENT_SNAPSHOT_VERSION,
            database_identity: self.database_identity,
            replica_id: self.replica_id,
            module_abi_version: CURRENT_MODULE_ABI_VERSION,
            tx_offset,
            blobs: vec![],
            tables: vec![],
        }
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

    /// Given `snapshot_dir` as the result of [`SnapshotRepository::snapshot_dir_path`],
    /// get the [`DirTrie`] which contains serialized objects (pages and large blobs)
    /// referenced by the [`Snapshot`] contained in the [`SnapshotDirPath`].
    ///
    /// Consequences are unspecified if this method is called from outside this crate
    /// on a non-existent, locked or incomplete `snapshot_dir`.
    ///
    /// Any mutations to the returned [`DirTrie`] or its contents
    /// will likely render the snapshot corrupted,
    /// causing future attempts to reconstruct it to fail.
    pub fn object_repo(snapshot_dir: &SnapshotDirPath) -> Result<DirTrie, std::io::Error> {
        DirTrie::open(snapshot_dir.objects().0)
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
    /// - The snapshot file's version does not match [`CURRENT_SNAPSHOT_VERSION`].
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
        let (snapshot, compress_type) = Snapshot::read_from_file(&snapshot_file_path)?;

        if snapshot.magic != MAGIC {
            return Err(SnapshotError::BadMagic {
                tx_offset,
                magic: snapshot.magic,
            });
        }

        if snapshot.version != CURRENT_SNAPSHOT_VERSION {
            return Err(SnapshotError::BadVersion {
                tx_offset,
                version: snapshot.version,
            });
        }

        let snapshot_dir = self.snapshot_dir_path(tx_offset);
        let object_repo = Self::object_repo(&snapshot_dir)?;

        let blob_store = snapshot.reconstruct_blob_store(&object_repo)?;

        let tables = snapshot.reconstruct_tables(&object_repo, page_pool)?;

        Ok(ReconstructedSnapshot {
            database_identity: snapshot.database_identity,
            replica_id: snapshot.replica_id,
            tx_offset: snapshot.tx_offset,
            module_abi_version: snapshot.module_abi_version,
            blob_store,
            tables,
            compress_type,
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
        let snapshot = spawn_blocking({
            let snapshot_dir = snapshot_dir.clone();
            move || {
                let lockfile = Lockfile::lock_path(&snapshot_dir);
                if lockfile.try_exists()? {
                    return Err(SnapshotError::Incomplete { tx_offset, lockfile });
                }

                let snapshot_file_path = snapshot_dir.snapshot_file(tx_offset);
                let (snapshot, _compress_type) = Snapshot::read_from_file(&snapshot_file_path)?;

                if snapshot.magic != MAGIC {
                    return Err(SnapshotError::BadMagic {
                        tx_offset,
                        magic: snapshot.magic,
                    });
                }
                Ok(snapshot)
            }
        })
        .await
        .unwrap()?;
        let object_repo = Self::object_repo(&snapshot_dir)?;
        verify_snapshot(object_repo, self.root.clone(), snapshot.clone())
            .await
            .map(drop)?;
        Ok(snapshot)
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
        Ok(self
            .all_snapshots()?
            // Ignore `tx_offset`s greater than the current upper bound.
            .filter(|tx_offset| *tx_offset <= upper_bound)
            // Select the largest TxOffset.
            .max())
    }

    pub fn all_snapshots(&self) -> Result<impl Iterator<Item = TxOffset>, SnapshotError> {
        Ok(self
            .root
            // Item = Result<DirEntry>
            .read_dir()?
            // Item = DirEntry
            .filter_map(Result::ok)
            // Item = PathBuf
            .map(|dirent| dirent.path())
            // Ignore entries not shaped like snapshot directories.
            .filter(|path| path.extension() == Some(OsStr::new(SNAPSHOT_DIR_EXT)))
            // Ignore entries whose lockfile still exists.
            .filter(|path| !Lockfile::lock_path(path).exists())
            // Parse each entry's TxOffset from the file name; ignore unparsable.
            // Also ignore if the snapshot file doesn't exists.
            // This can happen on incomplete transfers, or if something went
            // wrong during creation.
            // Item = TxOffset
            .filter_map(|path| {
                let offset = TxOffset::from_str_radix(path.file_stem()?.to_str()?, 10).ok()?;
                let snapshot_file = SnapshotDirPath::from_path_unchecked(path).snapshot_file(offset);
                if !snapshot_file.0.exists() {
                    None
                } else {
                    Some(offset)
                }
            }))
    }

    /// Return an interator of [`ArchivedSnapshotDirPath`] for all the archived snapshot directories on disk
    pub fn all_archived_snapshots(&self) -> Result<impl Iterator<Item = ArchivedSnapshotDirPath>, SnapshotError> {
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
            let path = self.snapshot_dir_path(newer_snapshot);
            log::info!("Renaming snapshot newer than {upper_bound} from {path:?} to {path:?}");
            path.rename_invalid()?;
        }
        Ok(())
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
        let snapshot_file = snapshot_dir.snapshot_file(tx_offset);
        let (snapshot, compress_type) = Snapshot::read_from_file(&snapshot_file)?;

        let mut stats = ObjectCompressionStats::default();
        if let Some(algo) = compress_type.algorithm() {
            log::debug!(
                "Snapshot {snapshot_dir:?} of replica {} is already compressed: {algo:?}",
                snapshot.replica_id
            );
            return Ok(stats);
        }

        let old = if let Some((tx_offset, snapshot_dir)) = parent {
            let snapshot_file = snapshot_dir.snapshot_file(*tx_offset);
            let (snapshot, _) = Snapshot::read_from_file(&snapshot_file)?;
            let dir = SnapshotRepository::object_repo(snapshot_dir)?;
            snapshot.files(&dir).collect()
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
            if let Some(hash) = hash {
                if let Some(old_path) = old.get(&hash) {
                    let old_file = CompressReader::new(o_rdonly().open(old_path)?)?;
                    if old_file.is_compressed() {
                        std::fs::hard_link(old_path, src.with_extension("_tmp"))?;
                        std::fs::rename(src.with_extension("_tmp"), src)?;
                        if let Some(stats) = stats {
                            stats.hardlinked += 1;
                        }
                        return Ok(());
                    }
                }
            }

            let dst = src.with_extension("_tmp");
            let mut write = BufWriter::new(o_excl().open(&dst)?);
            // The default frame size compress better.
            compress_with_zstd(read, &mut write, None)?;
            std::fs::rename(dst, src)?;
            if let Some(stats) = stats {
                stats.compressed += 1;
            }

            Ok(())
        }

        let _lock = Lockfile::for_file(snapshot_dir)?;

        log::info!(
            "Compressing snapshot {snapshot_dir:?} of replica {}",
            snapshot.replica_id
        );

        let dir = SnapshotRepository::object_repo(snapshot_dir)?;
        for (hash, path) in snapshot.files(&dir) {
            compress(&old, &path, Some(hash), Some(&mut stats)).inspect_err(|err| {
                log::error!("Failed to compress object file {path:?}: {err}");
            })?;
        }

        // Compress the snapshot file last,
        // which marks the whole snapshot as compressed.
        //
        // Don't update the stats for the snapshot file.
        compress(&old, &snapshot_file.0, None, None).inspect_err(|err| {
            log::error!("Failed to compress snapshot file {snapshot_file:?}: {err}");
        })?;

        log::info!(
            "Compressed snapshot {snapshot_dir:?} of replica {}: {compress_type:?}",
            snapshot.replica_id
        );
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
        let snapshot_file = snapshot_dir.snapshot_file(offset);
        let snapshot_file_size = snapshot_file.metadata()?.len();

        let (snapshot, compress_type) = Snapshot::read_from_file(&snapshot_file)?;

        size.snapshot = match compress_type {
            CompressType::None => CompressCount { none: 1, zstd: 0 },
            CompressType::Algorithm(CompressionAlgorithm::Zstd) => CompressCount { none: 0, zstd: 1 },
        };

        size.file_size += snapshot_file_size;
        size.total_size += snapshot_file_size;
        let repo = Self::object_repo(&snapshot_dir)?;
        for (_, f) in snapshot.files(&repo) {
            let file_size = f.metadata()?.len();
            size.object_size += file_size;
            size.total_size += file_size;
            size.object_count += 1;
        }

        Ok(size)
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
    pub blob_store: HashMapBlobStore,

    /// All the tables from the snapshotted state, sans schema information and indexes.
    ///
    /// This includes the system tables,
    /// so the schema of user-defined tables can be recovered
    /// given knowledge of the schema of `st_table` and `st_column`.
    pub tables: BTreeMap<TableId, Vec<Box<Page>>>,
    /// If the snapshot was compressed or not.
    pub compress_type: CompressType,
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn listing_ignores_if_snapshot_file_is_missing() -> anyhow::Result<()> {
        let tmp = tempdir()?;

        let root = SnapshotsPath::from_path_unchecked(tmp.path());
        let repo = SnapshotRepository::open(root, Identity::ZERO, 42)?;
        for i in 0..10 {
            repo.snapshot_dir_path(i).create()?;
        }
        repo.snapshot_dir_path(5)
            .snapshot_file(5)
            .open_file(OpenOptions::new().write(true).create_new(true))
            .map(drop)?;

        assert_eq!(vec![5], repo.all_snapshots()?.collect::<Vec<_>>());

        Ok(())
    }

    #[test]
    fn listing_ignores_if_lockfile_exists() -> anyhow::Result<()> {
        let tmp = tempdir()?;

        let root = SnapshotsPath::from_path_unchecked(tmp.path());
        let repo = SnapshotRepository::open(root, Identity::ZERO, 42)?;
        for i in 0..10 {
            let snapshot_dir = repo.snapshot_dir_path(i);
            snapshot_dir.create()?;
            snapshot_dir
                .snapshot_file(i)
                .open_file(OpenOptions::new().write(true).create_new(true))
                .map(drop)?;
        }
        let _lock = Lockfile::for_file(repo.snapshot_dir_path(5))?;

        let mut snapshots = repo.all_snapshots()?.collect::<Vec<_>>();
        snapshots.sort();
        assert_eq!(vec![0, 1, 2, 3, 4, 6, 7, 8, 9], snapshots);

        Ok(())
    }
}
