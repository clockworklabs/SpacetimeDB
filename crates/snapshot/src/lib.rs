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
use spacetimedb_fs_utils::compression::{CompressReader, CompressType, CompressWriter};
use spacetimedb_fs_utils::{
    dir_trie::{o_excl, o_rdonly, CountCreated, DirTrie},
    lockfile::{Lockfile, LockfileError},
};
use spacetimedb_lib::{
    bsatn::{self},
    de::Deserialize,
    ser::Serialize,
    Identity,
};
use spacetimedb_paths::server::{SnapshotDirPath, SnapshotFilePath, SnapshotsPath};
use spacetimedb_primitives::TableId;
use spacetimedb_table::{
    blob_store::{BlobHash, BlobStore, HashMapBlobStore},
    page::Page,
    table::Table,
};
use std::io::Write;
use std::{collections::BTreeMap, ffi::OsStr, fmt, io::Read, path::PathBuf};

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

/// File extension of snapshot files, which contain BSATN-encoded [`Snapshot`]s preceeded by [`blake3::Hash`]es.
pub const SNAPSHOT_FILE_EXT: &str = "snapshot_bsatn";

/// File extension of snapshots which have been marked invalid by [`SnapshotRepository::invalidate_newer_snapshots`].
pub const INVALID_SNAPSHOT_DIR_EXT: &str = "invalid_snapshot";

#[derive(Serialize, Deserialize)]
/// The hash and refcount of a single blob in the blob store.
struct BlobEntry {
    hash: BlobHash,
    uses: u32,
}

#[derive(Serialize, Deserialize)]
/// A snapshot of a single table, containing the hashes of all its resident pages.
struct TableEntry {
    table_id: TableId,
    pages: Vec<blake3::Hash>,
}

#[derive(Serialize, Deserialize)]
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
    pub fn read_from_file(path: &SnapshotFilePath) -> Result<Self, SnapshotError> {
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

        Ok(snapshot)
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
                let page = bsatn::from_slice::<Box<Page>>(&buf).map_err(|cause| SnapshotError::Deserialize {
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
    ) -> Result<(TableId, Vec<Box<Page>>), SnapshotError> {
        Ok((*table_id, Self::reconstruct_one_table_pages(object_repo, pages)?))
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
    fn reconstruct_tables(&self, object_repo: &DirTrie) -> Result<BTreeMap<TableId, Vec<Box<Page>>>, SnapshotError> {
        self.tables
            .iter()
            .map(|tbl| Self::reconstruct_one_table(object_repo, tbl))
            .collect()
    }

    /// Obtain an iterator over the [`blake3::Hash`]es of all objects
    /// this snapshot is referring to.
    pub fn objects(&self) -> impl Iterator<Item = blake3::Hash> + '_ {
        self.blobs
            .iter()
            .map(|b| blake3::Hash::from_bytes(b.hash.data))
            .chain(self.tables.iter().flat_map(|t| t.pages.iter().copied()))
    }
}

#[derive(Clone)]
pub struct SnapshotSize {
    pub compressed_type: CompressType,
    /// The size of the snapshot file in `bytes`.
    pub file_size: u64,
    /// The size of the snapshot's objects in `bytes`.
    pub object_size: u64,
    /// The number of objects in the snapshot.
    pub object_count: u64,
    /// Total size of the snapshot in `bytes`.
    pub total_size: u64,
}

impl fmt::Debug for SnapshotSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnapshotSize")
            .field("compressed_type", &self.compressed_type)
            .field("object_count   ", &self.object_count)
            .field("file_size      ", &format_args!("{:>8} bytes", self.file_size))
            .field("object_size    ", &format_args!("{:>8} bytes", self.object_size))
            .field("total_size     ", &format_args!("{:>8} bytes", self.total_size))
            .finish()
    }
}

/// A repository of snapshots of a particular database instance.
pub struct SnapshotRepository {
    /// The directory which contains all the snapshots.
    root: SnapshotsPath,

    /// The database address of the database instance for which this repository stores snapshots.
    database_identity: Identity,

    /// The database instance ID of the database instance for which this repository stores snapshots.
    replica_id: u64,

    /// Whether to use compression when *writing* snapshots.
    compress_type: CompressType,
    // TODO(deduplication): track the most recent successful snapshot
    // (possibly in a file)
    // and hardlink its objects into the next snapshot for deduplication.
}

impl SnapshotRepository {
    /// Enabling compression with the specified  [CompressType] algorithm.
    pub fn with_compression(mut self, compress_type: CompressType) -> Self {
        self.compress_type = compress_type;
        self
    }
    /// Returns [`Address`] of the database this [`SnapshotRepository`] is configured to snapshot.
    pub fn database_identity(&self) -> Identity {
        self.database_identity
    }

    /// Capture a snapshot of the state of the database at `tx_offset`,
    /// where `tables` is the committed state of all the tables in the database,
    /// and `blobs` is the committed state's blob store.
    ///
    /// Returns the path of the newly-created snapshot directory.
    pub fn create_snapshot<'db>(
        &self,
        tables: impl Iterator<Item = &'db mut Table>,
        blobs: &'db dyn BlobStore,
        tx_offset: TxOffset,
    ) -> Result<SnapshotDirPath, SnapshotError> {
        // If a previous snapshot exists in this snapshot repo,
        // get a handle on its object repo in order to hardlink shared objects into the new snapshot.
        let prev_snapshot = self
            .latest_snapshot()?
            .map(|tx_offset| self.snapshot_dir_path(tx_offset));
        let prev_snapshot = if let Some(prev_snapshot) = prev_snapshot {
            assert!(
                prev_snapshot.0.is_dir(),
                "prev_snapshot {prev_snapshot:?} is not a directory"
            );
            let object_repo = Self::object_repo(&prev_snapshot, self.compress_type)?;
            Some(object_repo)
        } else {
            None
        };

        let mut counter = CountCreated::default();

        let snapshot_dir = self.snapshot_dir_path(tx_offset);

        // Before performing any observable operations,
        // acquire a lockfile on the snapshot you want to create.
        // TODO(noa): is this lockfile still necessary now that we have data-dir?
        let _lock = Lockfile::for_file(&snapshot_dir)?;

        // Create the snapshot directory.
        snapshot_dir.create()?;

        // Create a new `DirTrie` to hold all the content-addressed objects in the snapshot.
        let object_repo = Self::object_repo(&snapshot_dir, self.compress_type)?;

        // Build the in-memory `Snapshot` object.
        let mut snapshot = self.empty_snapshot(tx_offset);

        // Populate both the in-memory `Snapshot` object and the on-disk object repository
        // with all the blobs and pages.
        snapshot.write_all_blobs(&object_repo, blobs, prev_snapshot.as_ref(), &mut counter)?;
        snapshot.write_all_tables(&object_repo, tables, prev_snapshot.as_ref(), &mut counter)?;

        // Serialize and hash the in-memory `Snapshot` object.
        let snapshot_bsatn = bsatn::to_vec(&snapshot).map_err(|cause| SnapshotError::Serialize {
            ty: ObjectType::Snapshot,
            cause,
        })?;
        let hash = blake3::hash(&snapshot_bsatn);

        // Create the snapshot file, containing first the hash, then the `Snapshot`.
        {
            let snapshot_file = snapshot_dir.snapshot_file(tx_offset).open_file(&o_excl())?;
            let mut compress = CompressWriter::new(snapshot_file, self.compress_type)?;
            compress.write_all(hash.as_bytes())?;
            compress.write_all(&snapshot_bsatn)?;
        }

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

    /// Given `snapshot_dir` as the result of [`Self::snapshot_dir_path`],
    /// get the [`DirTrie`] which contains serialized objects (pages and large blobs)
    /// referenced by the [`Snapshot`] contained in the [`Self::snapshot_file_path`].
    ///
    /// Consequences are unspecified if this method is called from outside this crate
    /// on a non-existent, locked or incomplete `snapshot_dir`.
    ///
    /// Any mutations to the returned [`DirTrie`] or its contents
    /// will likely render the snapshot corrupted,
    /// causing future attempts to reconstruct it to fail.
    pub fn object_repo(snapshot_dir: &SnapshotDirPath, compress_type: CompressType) -> Result<DirTrie, std::io::Error> {
        DirTrie::open(snapshot_dir.objects().0, compress_type)
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
    /// - The snapshot file's database address or instance ID do not match those in `self`.
    /// - The snapshot file's module ABI version does not match [`CURRENT_MODULE_ABI_VERSION`].
    /// - The snapshot file's recorded transaction offset does not match `tx_offset`.
    ///
    /// This means that callers must inspect the returned [`ReconstructedSnapshot`]
    /// and verify that they can handle its contained database address, instance ID, module ABI version and transaction offset.
    pub fn read_snapshot(&self, tx_offset: TxOffset) -> Result<ReconstructedSnapshot, SnapshotError> {
        let snapshot_dir = self.snapshot_dir_path(tx_offset);
        let lockfile = Lockfile::lock_path(&snapshot_dir);
        if lockfile.try_exists()? {
            return Err(SnapshotError::Incomplete { tx_offset, lockfile });
        }

        let snapshot_file_path = snapshot_dir.snapshot_file(tx_offset);
        let snapshot = Snapshot::read_from_file(&snapshot_file_path)?;

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

        let object_repo = Self::object_repo(&snapshot_dir, self.compress_type)?;

        let blob_store = snapshot.reconstruct_blob_store(&object_repo)?;

        let tables = snapshot.reconstruct_tables(&object_repo)?;

        Ok(ReconstructedSnapshot {
            database_identity: snapshot.database_identity,
            replica_id: snapshot.replica_id,
            tx_offset: snapshot.tx_offset,
            module_abi_version: snapshot.module_abi_version,
            blob_store,
            tables,
        })
    }

    /// Open a repository at `root`, failing if the `root` doesn't exist or isn't a directory.
    ///
    /// Calls [`Path::is_dir`] and requires that the result is `true`.
    /// See that method for more detailed preconditions on this function.
    pub fn open(root: SnapshotsPath, database_identity: Identity, replica_id: u64) -> Result<Self, SnapshotError> {
        if !root.is_dir() {
            return Err(SnapshotError::NotDirectory { root });
        }
        Ok(Self {
            root,
            database_identity,
            replica_id,
            compress_type: CompressType::None,
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
            // Parse each entry's TxOffset from the file name; ignore unparseable.
            // Item = TxOffset
            .filter_map(|path| TxOffset::from_str_radix(path.file_stem()?.to_str()?, 10).ok()))
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

    /// Calculate the size of the snapshot repository in bytes.
    pub fn size_on_disk(&self) -> Result<SnapshotSize, SnapshotError> {
        let mut size = SnapshotSize {
            compressed_type: self.compress_type,
            file_size: 0,
            object_size: 0,
            object_count: 0,
            total_size: 0,
        };

        for snapshot in self.all_snapshots()? {
            let snap = self.size_on_disk_snapshot(snapshot)?;
            size.file_size += snap.file_size;
            size.object_size += snap.object_size;
            size.object_count += snap.object_count;
            size.total_size += snap.total_size;
        }
        Ok(size)
    }

    pub fn size_on_disk_snapshot(&self, offset: TxOffset) -> Result<SnapshotSize, SnapshotError> {
        let mut size = SnapshotSize {
            compressed_type: self.compress_type,
            file_size: 0,
            object_size: 0,
            object_count: 0,
            total_size: 0,
        };

        let snapshot_dir = self.snapshot_dir_path(offset);
        let snapshot_file = snapshot_dir.snapshot_file(offset);
        let snapshot_file_size = snapshot_file.metadata()?.len();
        size.file_size += snapshot_file_size;
        size.total_size += snapshot_file_size;
        let objects = snapshot_dir.objects().read_dir()?;
        //Search the subdirectories
        for object in objects {
            let object = object?;
            // now the files in the subdirectories
            let object_files = object.path().read_dir()?;
            for object_file in object_files {
                let object_file = object_file?;
                let file_size = object_file.metadata()?.len();
                size.object_size += file_size;
                size.total_size += file_size;
                size.object_count += 1;
            }
        }

        Ok(size)
    }

    /// Calculate the size of the snapshot repository in bytes.
    pub fn size_on_disk_last_snapshot(&self) -> Result<SnapshotSize, SnapshotError> {
        let mut size = SnapshotSize {
            compressed_type: self.compress_type,
            file_size: 0,
            object_size: 0,
            object_count: 0,
            total_size: 0,
        };

        if let Some(snapshot) = self.latest_snapshot()? {
            let snap = self.size_on_disk_snapshot(snapshot)?;
            size.file_size += snap.file_size;
            size.object_size += snap.object_size;
            size.object_count += snap.object_count;
            size.total_size += snap.total_size;
        }
        Ok(size)
    }
}

pub struct ReconstructedSnapshot {
    /// The address of the snapshotted database.
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
}
