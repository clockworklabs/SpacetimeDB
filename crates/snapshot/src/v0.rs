use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::dir_trie::DirTrie;
use spacetimedb_lib::{bsatn, Identity};
use spacetimedb_paths::server::SnapshotDirPath;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{Deserialize, Serialize};
use spacetimedb_table::{
    page_pool::PagePool,
    tiered::{BlobMap, ObjectKey},
};

use crate::{
    common, remote::ObjectProvider, BlobEntry, ObjectType, ParentRepo, SnapshotError, SnapshotReadKindMetrics,
    TableEntry, TablePages,
};

pub const SNAPSHOT_VERSION: u8 = 0;

#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// A magic number: must be equal to [`MAGIC`].
    pub(crate) magic: [u8; 4],

    /// The snapshot version number. Must be equal to [`SNAPSHOT_VERSION`].
    pub(crate) version: u8,

    /// The identity of the snapshotted database.
    pub database_identity: Identity,
    /// The instance ID of the snapshotted database.
    pub replica_id: u64,

    /// ABI version of the module from which this snapshot was created, as [MAJOR, MINOR].
    ///
    /// As of this proposal, [7, 0].
    pub(crate) module_abi_version: [u16; 2],

    /// The transaction offset of the state this snapshot reflects.
    pub tx_offset: TxOffset,

    /// The hashes and reference counts of all objects in the blob store.
    pub(crate) blobs: Vec<BlobEntry>,

    /// For each table, its table ID followed by the hashes of all resident pages.
    ///
    /// It's necessary to store the table ID rather than relying on order
    /// because table IDs may be sparse (and will be, once we reserve a bunch of system table ids).
    pub(crate) tables: Vec<TableEntry>,
}

impl Snapshot {
    pub fn objects(&self) -> impl Iterator<Item = ObjectKey> {
        self.blobs.iter().map(|entry| ObjectKey::from(entry.hash)).chain(
            self.tables
                .iter()
                .flat_map(|table| table.present_pages().map(Into::into)),
        )
    }

    /// Obtain an iterator over the [`PathBuf`]s of all objects
    pub fn files<'a>(&'a self, repo: &'a SnapshotRepo) -> impl Iterator<Item = (blake3::Hash, PathBuf)> + 'a {
        self.objects().map(|key| (key.into(), repo.file_path(key)))
    }

    pub fn write_snapshot_file(self, repo: &SnapshotRepo) -> Result<(), SnapshotError> {
        let path = repo.root.snapshot_file(self.tx_offset);
        // Serialize and hash the in-memory `Snapshot` object.
        let snapshot_bsatn = bsatn::to_vec(&self).map_err(|cause| SnapshotError::Serialize {
            ty: ObjectType::Snapshot,
            cause,
        })?;

        common::write_snapshot_file(path, &snapshot_bsatn)
    }

    pub fn total_objects(&self) -> usize {
        self.blobs.len()
            + self
                .tables
                .iter()
                .map(|table| table.present_pages().count())
                .sum::<usize>()
    }

    pub(super) fn repo(&self, dir: &SnapshotDirPath) -> io::Result<SnapshotRepo> {
        SnapshotRepo::open(dir)
    }
}

pub struct SnapshotRepo {
    root: SnapshotDirPath,
    inner: DirTrie,
}

impl SnapshotRepo {
    pub fn open(dir: &SnapshotDirPath) -> io::Result<Self> {
        DirTrie::open(dir.objects().0).map(|inner| Self {
            root: dir.clone(),
            inner,
        })
    }

    pub fn root(&self) -> &Path {
        self.inner.root()
    }

    pub fn reconstruct_blobs(
        &self,
        manifest: &Snapshot,
        metrics: &mut SnapshotReadKindMetrics,
    ) -> Result<BlobMap, SnapshotError> {
        common::reconstruct_blob_store(&manifest.blobs, &self.inner, metrics)
    }

    pub fn reconstruct_tables(
        &self,
        manifest: &Snapshot,
        page_pool: &PagePool,
        metrics: &mut SnapshotReadKindMetrics,
    ) -> Result<BTreeMap<TableId, TablePages>, SnapshotError> {
        common::reconstruct_tables(&manifest.tables, &self.inner, page_pool, metrics)
    }

    pub fn file_path(&self, key: ObjectKey) -> PathBuf {
        self.inner.file_path(key.as_bytes())
    }

    pub(crate) fn parent(&self) -> Result<Option<ParentRepo>, SnapshotError> {
        let snapshots_dir = self.root.snapshots();
        let offset = self.root.tx_offset().map(|offset| offset - 1).unwrap_or(TxOffset::MAX);
        common::parent_repo(&snapshots_dir, offset)
    }
}

impl ObjectProvider for SnapshotRepo {
    type Reader = <DirTrie as ObjectProvider>::Reader;

    fn object_reader(&self, key: ObjectKey) -> impl Future<Output = io::Result<Self::Reader>> + Send {
        self.inner.object_reader(key)
    }
}
