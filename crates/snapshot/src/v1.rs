use std::{
    collections::BTreeMap,
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::dir_trie::{CountCreated, DirTrie};
use spacetimedb_lib::{
    bsatn,
    de::{self, Deserializer},
    ser::Serializer,
};
use spacetimedb_paths::server::SnapshotDirPath;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{Deserialize, Serialize};
use spacetimedb_table::{
    page_pool::PagePool,
    table::Table,
    tiered::{BlobMap, ObjectKey, ObjectStoreError, ScannedBlob},
};

use crate::{common, remote::ObjectProvider, v0, ObjectType, SnapshotError, SnapshotReadKindMetrics, TablePages};

pub const SNAPSHOT_VERSION: u8 = 1;

#[derive(Clone)]
pub struct Snapshot(v0::Snapshot);

impl Snapshot {
    pub(super) fn new(mut manifest: v0::Snapshot) -> Self {
        manifest.version = self::SNAPSHOT_VERSION;
        Self(manifest)
    }

    pub(super) fn tx_offset_mut(&mut self) -> &mut TxOffset {
        &mut self.0.tx_offset
    }

    #[cfg(test)]
    pub fn get_mut(&mut self) -> &mut v0::Snapshot {
        &mut self.0
    }

    pub fn files<'a>(&'a self, repo: &'a SnapshotRepo) -> impl Iterator<Item = (blake3::Hash, PathBuf)> {
        self.objects().map(|key| (key.into(), repo.file_path(key)))
    }

    pub fn write_objects<'db, 'a>(
        &mut self,
        repo: &SnapshotRepo,
        parent: Option<&SnapshotDirPath>,
        tables: impl Iterator<Item = &'db mut Table>,
        blobs: impl Iterator<Item = Result<ScannedBlob<'a>, ObjectStoreError>>,
        counter: &mut CountCreated,
    ) -> Result<(), SnapshotError> {
        let parent = parent.map(SnapshotRepo::open).transpose()?;

        common::write_blobs(
            &repo.blobs,
            parent.as_ref().map(|repo| &repo.blobs),
            |entry| self.0.blobs.push(entry),
            counter,
            blobs,
        )?;
        common::write_tables(
            &repo.pages,
            parent.as_ref().map(|repo| &repo.pages),
            |entry| self.0.tables.push(entry),
            counter,
            tables,
        )?;

        Ok(())
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

    pub(super) fn repo(&self, dir: &SnapshotDirPath) -> io::Result<SnapshotRepo> {
        SnapshotRepo::open(dir)
    }
}

impl Deref for Snapshot {
    type Target = v0::Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for Snapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Snapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let manifest = v0::Snapshot::deserialize(deserializer)?;
        if manifest.version != self::SNAPSHOT_VERSION {
            return Err(de::Error::custom(format!(
                "snapshot manifest version mismatch: expected {}, encountered: {}",
                self::SNAPSHOT_VERSION,
                manifest.version
            )));
        }

        Ok(Self(manifest))
    }
}

pub struct SnapshotRepo {
    root: SnapshotDirPath,
    pages: DirTrie,
    blobs: DirTrie,
}

impl SnapshotRepo {
    pub fn open(dir: &SnapshotDirPath) -> io::Result<Self> {
        let objects = dir.objects();
        let pages = DirTrie::open(objects.pages().0)?;
        let blobs = DirTrie::open(objects.blobs().0)?;

        Ok(Self {
            root: dir.clone(),
            pages,
            blobs,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root.0
    }

    pub fn reconstruct_blobs(
        &self,
        manifest: &Snapshot,
        metrics: &mut SnapshotReadKindMetrics,
    ) -> Result<BlobMap, SnapshotError> {
        common::reconstruct_blob_store(&manifest.blobs, &self.blobs, metrics)
    }

    pub fn reconstruct_tables(
        &self,
        manifest: &Snapshot,
        page_pool: &PagePool,
        metrics: &mut SnapshotReadKindMetrics,
    ) -> Result<BTreeMap<TableId, TablePages>, SnapshotError> {
        common::reconstruct_tables(&manifest.tables, &self.pages, page_pool, metrics)
    }

    pub fn file_path(&self, key: ObjectKey) -> PathBuf {
        match key {
            ObjectKey::Blob(hash) => self.blobs.file_path(hash.as_bytes()),
            ObjectKey::Page(hash) => self.pages.file_path(hash.as_bytes()),
        }
    }
}

impl ObjectProvider for SnapshotRepo {
    type Reader = <DirTrie as ObjectProvider>::Reader;

    fn object_reader(&self, key: ObjectKey) -> impl Future<Output = io::Result<Self::Reader>> + Send {
        match key {
            ObjectKey::Page(_) => self.pages.object_reader(key),
            ObjectKey::Blob(_) => self.blobs.object_reader(key),
        }
    }
}
