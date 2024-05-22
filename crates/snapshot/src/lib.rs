use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::lockfile::Lockfile;
use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize, Address};
use spacetimedb_primitives::TableId;
use spacetimedb_table::{
    blob_store::{BlobHash, BlobStore},
    page::Page,
    table::Table,
};
use std::{
    fs::OpenOptions,
    io::Write,
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
        let object_repo = DirTrie::open(snapshot_dir.join("objects"))?;

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
}
