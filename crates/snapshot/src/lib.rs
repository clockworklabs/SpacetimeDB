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

use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::{
    dir_trie::{o_excl, o_rdonly, CountCreated, DirTrie},
    lockfile::{Lockfile, LockfileError},
};
use spacetimedb_lib::{
    bsatn::{self},
    de::Deserialize,
    ser::Serialize,
    Address,
};
use spacetimedb_primitives::TableId;
use spacetimedb_table::{
    blob_store::{BlobHash, BlobStore, HashMapBlobStore},
    page::Page,
    table::Table,
};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    io::{Read, Write},
    path::{Path, PathBuf},
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
    NotDirectory { root: PathBuf },
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

    /// The address of the snapshotted database.
    database_address: Address,
    /// The instance ID of the snapshotted database.
    database_instance_id: u64,

    /// ABI version of the module from which this snapshot was created, as [MAJOR, MINOR].
    ///
    /// As of this proposal, [7, 0].
    module_abi_version: [u16; 2],

    /// The transaction offset of the state this snapshot reflects.
    tx_offset: TxOffset,

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
    /// Fails if:
    /// - `path` does not refer to a readable file.
    /// - The file at `path` is corrupted,
    ///   as detected by comparing the hash of its bytes to a hash recorded in the file.
    fn read_from_file(path: &Path) -> Result<Self, SnapshotError> {
        let err_read_object = |cause| SnapshotError::ReadObject {
            ty: ObjectType::Snapshot,
            source_repo: path.to_path_buf(),
            cause,
        };
        let mut snapshot_file = o_rdonly().open(path).map_err(err_read_object)?;

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
                source_repo: path.to_path_buf(),
            });
        }

        let snapshot = bsatn::from_slice::<Snapshot>(&snapshot_bsatn).map_err(|cause| SnapshotError::Deserialize {
            ty: ObjectType::Snapshot,
            source_repo: path.to_path_buf(),
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
}

/// A repository of snapshots of a particular database instance.
pub struct SnapshotRepository {
    /// The directory which contains all the snapshots.
    root: PathBuf,

    /// The database address of the database instance for which this repository stores snapshots.
    database_address: Address,

    /// The database instance ID of the database instance for which this repository stores snapshots.
    database_instance_id: u64,
    // TODO(deduplication): track the most recent successful snapshot
    // (possibly in a file)
    // and hardlink its objects into the next snapshot for deduplication.
}

impl SnapshotRepository {
    /// Returns [`Address`] of the database this [`SnapshotRepository`] is configured to snapshot.
    pub fn database_address(&self) -> Address {
        self.database_address
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
    ) -> Result<PathBuf, SnapshotError> {
        // If a previous snapshot exists in this snapshot repo,
        // get a handle on its object repo in order to hardlink shared objects into the new snapshot.
        let prev_snapshot = self
            .latest_snapshot()?
            .map(|tx_offset| self.snapshot_dir_path(tx_offset));
        let prev_snapshot = if let Some(prev_snapshot) = prev_snapshot {
            assert!(
                prev_snapshot.is_dir(),
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
        let _lock = Lockfile::for_file(&snapshot_dir)?;

        // Create the snapshot directory.
        std::fs::create_dir_all(&snapshot_dir)?;

        // Create a new `DirTrie` to hold all the content-addressed objects in the snapshot.
        let object_repo = Self::object_repo(&snapshot_dir)?;

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
            let mut snapshot_file = o_excl().open(Self::snapshot_file_path(tx_offset, &snapshot_dir))?;
            snapshot_file.write_all(hash.as_bytes())?;
            snapshot_file.write_all(&snapshot_bsatn)?;
        }

        log::info!(
            "[{}] SNAPSHOT {:0>20}: Hardlinked {} objects and wrote {} objects",
            self.database_address,
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
            database_address: self.database_address,
            database_instance_id: self.database_instance_id,
            module_abi_version: CURRENT_MODULE_ABI_VERSION,
            tx_offset,
            blobs: vec![],
            tables: vec![],
        }
    }

    fn snapshot_dir_path(&self, tx_offset: TxOffset) -> PathBuf {
        let dir_name = format!("{tx_offset:0>20}.{SNAPSHOT_DIR_EXT}");
        self.root.join(dir_name)
    }

    fn snapshot_file_path(tx_offset: TxOffset, snapshot_dir: &Path) -> PathBuf {
        let file_name = format!("{tx_offset:0>20}.{SNAPSHOT_FILE_EXT}");
        snapshot_dir.join(file_name)
    }

    fn object_repo(snapshot_dir: &Path) -> Result<DirTrie, std::io::Error> {
        DirTrie::open(snapshot_dir.join("objects"))
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

        let snapshot_file_path = Self::snapshot_file_path(tx_offset, &snapshot_dir);
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

        let object_repo = Self::object_repo(&snapshot_dir)?;

        let blob_store = snapshot.reconstruct_blob_store(&object_repo)?;

        let tables = snapshot.reconstruct_tables(&object_repo)?;

        Ok(ReconstructedSnapshot {
            database_address: snapshot.database_address,
            database_instance_id: snapshot.database_instance_id,
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
    pub fn open(root: PathBuf, database_address: Address, database_instance_id: u64) -> Result<Self, SnapshotError> {
        if !root.is_dir() {
            return Err(SnapshotError::NotDirectory { root });
        }
        Ok(Self {
            root,
            database_address,
            database_instance_id,
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
            .filter_map(|path| TxOffset::from_str_radix(path.file_stem()?.to_str()?, 10).ok())
            // Ignore `tx_offset`s greater than the current upper bound.
            .filter(|tx_offset| *tx_offset <= upper_bound)
            // Select the largest TxOffset.
            .max())
    }

    /// Return the `TxOffset` of the highest-offset complete snapshot in the repository.
    ///
    /// Does not verify that the snapshot of the returned `TxOffset` is valid and uncorrupted,
    /// so a subsequent [`Self::read_snapshot`] may fail.
    pub fn latest_snapshot(&self) -> Result<Option<TxOffset>, SnapshotError> {
        self.latest_snapshot_older_than(TxOffset::MAX)
    }
}

pub struct ReconstructedSnapshot {
    /// The address of the snapshotted database.
    pub database_address: Address,
    /// The instance ID of the snapshotted database.
    pub database_instance_id: u64,
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
