use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::lockfile::Lockfile;
use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize, Address};
use spacetimedb_primitives::TableId;
use spacetimedb_table::{
    blob_store::{BlobHash, BlobStore, HashMapBlobStore},
    page::Page,
    table::Table,
};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::dir_trie::DirTrie;

mod dir_trie;

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
pub const SNAPSHOT_DIR_EXT: &str = ".snapshot.stdb";

/// File extension of snapshot files, which contain BSATN-encoded [`Snapshot`]s preceeded by [`blake3::Hash`]es.
pub const SNAPSHOT_FILE_EXT: &str = ".snapshot.bsatn";

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
    fn write_blob(&mut self, object_repo: &DirTrie, hash: &BlobHash, uses: usize, blob: &[u8]) -> anyhow::Result<()> {
        // TODO(deduplication): Accept a previous `DirTrie` and,
        // if it contains `hash`, hardlink rather than creating a new file.
        let mut file = object_repo.open_entry(&hash.data, &o_excl())?;
        file.write_all(blob)?;
        self.blobs.push(BlobEntry {
            hash: *hash,
            uses: uses as u32,
        });
        Ok(())
    }

    /// Populate the in-memory snapshot `self`,
    /// and the on-disk object repository `object_repo`,
    /// with all large blos from `blobs`.
    fn write_all_blobs(&mut self, object_repo: &DirTrie, blobs: &dyn BlobStore) -> anyhow::Result<()> {
        for (hash, uses, blob) in blobs.iter_blobs() {
            self.write_blob(object_repo, hash, uses, blob)?;
        }
        Ok(())
    }

    /// Insert a single large blob from a [`BlobStore`]
    /// into the on-disk object repository `object_repo`.
    ///
    /// Returns the hash of the `page` so that it may be inserted into an in-memory [`Snapshot`] object
    /// by [`Snapshot::write_table`].
    fn write_page(object_repo: &DirTrie, page: &mut Page) -> anyhow::Result<blake3::Hash> {
        let hash = page
            .unmodified_hash()
            .cloned()
            .unwrap_or_else(|| page.save_content_hash());

        // Dump the page to a file in the snapshot's object repo.
        match object_repo.open_entry(hash.as_bytes(), &o_excl()) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // We've already dumped a page with the same bytes.
                // This is most likely to occur if two tables have equivalent schemas,
                // or if a table contains multiple empty pages.
                // In this case, do nothing.
            }

            Ok(mut file) => {
                // TODO(deduplication): Accept a previous `DirTrie` and,
                // if it contains `hash`, hardlink rather than creating a new file.

                // TODO(perf): Write directly to file without intermediate vec.
                let page_bsatn = bsatn::to_vec(page)?;
                file.write_all(&page_bsatn)?;
            }
            Err(e) => return Err(e.into()),
        }
        Ok(hash)
    }

    /// Populate the in-memory snapshot `self`,
    /// and the on-disk object repository `object_repo`,
    /// with all pages from `table`.
    fn write_table(&mut self, object_repo: &DirTrie, table: &mut Table) -> anyhow::Result<()> {
        let pages = table
            .pages_mut()
            .iter_mut()
            .map(|page| Self::write_page(object_repo, page))
            .collect::<anyhow::Result<Vec<blake3::Hash>>>()?;

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
    ) -> anyhow::Result<()> {
        for table in tables {
            self.write_table(object_repo, table)?;
        }
        Ok(())
    }

    /// Read a [`Snapshot`] from the file at `path`, verify its hash, and return it.
    ///
    /// Fails if:
    /// - `path` does not refer to a readable file.
    /// - The file at `path` is corrupted,
    ///   as detected by comparing the hash of its bytes to a hash recorded in the file.
    fn read_from_file(path: &Path) -> anyhow::Result<Self> {
        let mut snapshot_file = o_rdonly().open(&path)?;

        let mut hash = [0; blake3::OUT_LEN];
        snapshot_file.read_exact(&mut hash)?;
        let hash = blake3::Hash::from_bytes(hash);
        let mut snapshot_bsatn = vec![];
        snapshot_file.read_to_end(&mut snapshot_bsatn)?;
        let computed_hash = blake3::hash(&snapshot_bsatn);

        if hash != computed_hash {
            anyhow::bail!("Computed hash of snapshot file {path:?} does not match recorded hash: computed {computed_hash:?} but expected {hash:?}");
        }

        Ok(bsatn::from_slice::<Snapshot>(&snapshot_bsatn)?)
    }

    /// Construct a [`HashMapBlobStore`] containing all the blobs referenced in `self`,
    /// reading their data from files in the `object_repo`.
    ///
    /// Fails if any of the object files is missing or corrupted,
    /// as detected by comparing the hash of its bytes to the hash recorded in `self`.
    fn reconstruct_blob_store(&self, object_repo: &DirTrie) -> anyhow::Result<HashMapBlobStore> {
        let mut blob_store = HashMapBlobStore::default();

        for BlobEntry { hash, uses } in &self.blobs {
            let mut file = object_repo.open_entry(&hash.data, &o_rdonly())?;
            let mut buf = Vec::with_capacity(file.metadata()?.len() as usize);
            file.read_to_end(&mut buf)?;
            let computed_hash = BlobHash::hash_from_bytes(&buf);
            if *hash != computed_hash {
                anyhow::bail!("Computed hash of large blob does not match recorded hash: computed {computed_hash:?} but expected {hash:?}");
            }

            blob_store.insert_with_uses(hash, *uses as usize, buf.into_boxed_slice());
        }

        Ok(blob_store)
    }

    /// Read all the pages referenced by `pages` from the `object_repo`.
    ///
    /// Fails if any of the pages files is missing or corrupted,
    /// as detected by comparing the hash of its bytes to the hash listed in `pages`.
    fn reconstruct_one_table_pages(object_repo: &DirTrie, pages: &[blake3::Hash]) -> anyhow::Result<Vec<Box<Page>>> {
        pages.iter().map(|hash| {
            let mut file = object_repo.open_entry(&hash.as_bytes(), &o_rdonly())?;
            // TODO: avoid allocating a `Vec` here.
            let mut buf = Vec::with_capacity(file.metadata()?.len() as usize);
            file.read_to_end(&mut buf)?;
            let page = bsatn::from_slice::<Box<Page>>(&buf)?;

            let computed_hash = page.content_hash();

            if *hash != computed_hash {
                anyhow::bail!("Computed hash of page does not match recorded hash: computed {computed_hash:?} but expected {hash:?}");
            }

            Ok::<Box<Page>, anyhow::Error>(page)
        }).collect()
    }

    fn reconstruct_one_table(
        object_repo: &DirTrie,
        TableEntry { table_id, pages }: &TableEntry,
    ) -> anyhow::Result<(TableId, Vec<Box<Page>>)> {
        Ok((*table_id, Self::reconstruct_one_table_pages(object_repo, pages)?))
    }

    /// Reconstruct all the table data from `self`,
    /// reading pages from files in the `object_repo`.
    ///
    /// This method cannot construct [`Table`] objects
    /// because doing so requires knowledge of the system tables' schemas
    /// to compute the schemas of the user-defined tables.o
    ///
    /// Fails if any object file referenced in `self` (as a page or large blob)
    /// is missing or corrupted,
    /// as detected by comparing the hash of its bytes to the hash recorded in `self`.
    fn reconstruct_tables(&self, object_repo: &DirTrie) -> anyhow::Result<BTreeMap<TableId, Vec<Box<Page>>>> {
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

fn o_excl() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    options
}

fn o_rdonly() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true);
    options
}

impl SnapshotRepository {
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
    ) -> anyhow::Result<PathBuf> {
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
        snapshot.write_all_blobs(&object_repo, blobs)?;
        snapshot.write_all_tables(&object_repo, tables)?;

        // Serialize and hash the in-memory `Snapshot` object.
        let snapshot_bsatn = bsatn::to_vec(&snapshot)?;
        let hash = blake3::hash(&snapshot_bsatn);

        // Create the snapshot file, containing first the hash, then the `Snapshot`.
        {
            let mut snapshot_file = o_excl().open(Self::snapshot_file_path(tx_offset, &snapshot_dir))?;
            snapshot_file.write_all(hash.as_bytes())?;
            snapshot_file.write_all(&snapshot_bsatn)?;
        }

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
        let dir_name = format!("{tx_offset:0>20}{SNAPSHOT_DIR_EXT}");
        self.root.join(dir_name)
    }

    fn snapshot_file_path(tx_offset: TxOffset, snapshot_dir: &Path) -> PathBuf {
        let file_name = format!("{tx_offset:0>20}{SNAPSHOT_FILE_EXT}");
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
    pub fn read_snapshot(&self, tx_offset: TxOffset) -> anyhow::Result<ReconstructedSnapshot> {
        let snapshot_dir = self.snapshot_dir_path(tx_offset);
        let lockfile = Lockfile::lock_path(&snapshot_dir);
        if lockfile.try_exists()? {
            anyhow::bail!("Refusing to reconstruct snapshot {snapshot_dir:?}: lockfile {lockfile:?} exists");
        }

        let snapshot_file_path = Self::snapshot_file_path(tx_offset, &snapshot_dir);
        let snapshot = Snapshot::read_from_file(&snapshot_file_path)?;

        if snapshot.magic != MAGIC {
            anyhow::bail!(
                "Refusing to reconstruct snapshot {snapshot_dir:?}: magic number {:?} ({:?}) does not match expected {:?} ({MAGIC:?})",
                String::from_utf8_lossy(&snapshot.magic),
                snapshot.magic,
                std::str::from_utf8(&MAGIC).unwrap(),
            );
        }

        if snapshot.version != CURRENT_SNAPSHOT_VERSION {
            anyhow::bail!(
                "Refusing to reconstruct snapshot {snapshot_dir:?}: snapshot file version {} does not match supported version {CURRENT_SNAPSHOT_VERSION}",
                snapshot.version,
            );
        }

        let object_repo = Self::object_repo(&snapshot_dir)?;

        let blob_store = snapshot.reconstruct_blob_store(&object_repo)?;

        let tables = snapshot.reconstruct_tables(&object_repo)?;

        Ok(ReconstructedSnapshot {
            database_address: snapshot.database_address,
            database_instance_id: snapshot.database_instance_id,
            tx_offset: snapshot.tx_offset,
            module_abi_versino: snapshot.module_abi_version,
            blob_store,
            tables,
        })
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
    pub module_abi_versino: [u16; 2],

    /// The blob store of the snapshotted state.
    pub blob_store: HashMapBlobStore,

    /// All the tables from the snapshotted state, sans schema information and indexes.
    ///
    /// This includes the system tables,
    /// so the schema of user-defined tables can be recovered
    /// given knowledge of the schema of `st_table` and `st_column`.
    pub tables: BTreeMap<TableId, Vec<Box<Page>>>,
}
