use std::{io, num::NonZeroUsize, path::Path};

use spacetimedb_primitives::TableId;

use crate::{
    blob_store::BlobHash,
    tiered::{
        blob_set::BlobData,
        error::SnapshotError,
        page_set::PageSnapshot,
        storage::{write_atomically, DiskObject},
        Fsync, Object, ObjectKey, PageHash, PageSet, TieredDatabaseStorage, TieredStorage,
    },
};

pub type PageHashes = Vec<Option<PageHash>>;

pub struct Snapshot {
    blobs: Vec<(BlobHash, NonZeroUsize)>,
    tables: Vec<(TableId, PageHashes)>,
}

impl Snapshot {
    pub fn create<'a>(
        database_storage: &TieredDatabaseStorage,
        snapshot_storage: &impl SnapshotObjectStorage,
        tables: impl IntoIterator<Item = (TableId, &'a mut PageSet)>,
    ) -> Result<Self, SnapshotError> {
        let mut blobs = Vec::new();
        for blob in database_storage.blobs.scan().into_iter() {
            let snapshot_object = match blob.data {
                BlobData::Stored { backing }
                | BlobData::Resident {
                    backing: Some(backing), ..
                } => SnapshotObject::from_backing(backing, &database_storage.store)?,
                BlobData::Resident { bytes, backing: None } => SnapshotObject::Memory(bytes.into()),
            };

            snapshot_storage.write(snapshot_object)?;
            blobs.push((blob.hash, blob.uses));
        }

        let mut tables0 = Vec::new();
        for (id, page_set) in tables {
            let mut pages = Vec::new();
            for page in page_set.snapshot() {
                let Some(page) = page else {
                    pages.push(None);
                    continue;
                };
                let snapshot_object = match page {
                    PageSnapshot::Memory(object) => SnapshotObject::Memory(object),
                    PageSnapshot::Stored(backing) => SnapshotObject::from_backing(backing, &database_storage.store)?,
                };
                let hash = snapshot_object
                    .key()
                    .as_page_hash()
                    .copied()
                    .expect("object key must be a page hash");
                snapshot_storage.write(snapshot_object)?;
                pages.push(Some(hash));
            }

            tables0.push((id, pages));
        }

        Ok(Self { blobs, tables: tables0 })
    }

    #[allow(clippy::type_complexity)]
    pub fn into_parts(self) -> (Vec<(BlobHash, NonZeroUsize)>, Vec<(TableId, PageHashes)>) {
        let Self { blobs, tables } = self;
        (blobs, tables)
    }
}

pub enum SnapshotObject<'a> {
    Memory(Object),
    Local(LocalObject<'a>),
    Remote(RemoteObject<'a>),
}

impl<'a> SnapshotObject<'a> {
    pub fn key(&self) -> ObjectKey {
        match self {
            Self::Memory(inner) => inner.key(),
            Self::Local(inner) => inner.object.key(),
            Self::Remote(inner) => inner.key,
        }
    }

    pub fn write(&self, dst: impl AsRef<Path>, fsync: Fsync) -> io::Result<()> {
        match self {
            Self::Memory(inner) => write_atomically(inner.as_bytes(), dst.as_ref(), fsync),
            Self::Local(inner) => inner.hardlink_or_write(dst, fsync),
            Self::Remote(inner) => inner.write(dst, fsync),
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    pub fn as_resident(&self) -> Option<&Object> {
        match self {
            Self::Memory(object) => Some(object),
            Self::Local(_) | Self::Remote(_) => None,
        }
    }

    fn from_backing(mut backing: DiskObject, store: &'a TieredStorage) -> Result<Self, SnapshotError> {
        // If we determine the object to be local, ensure it stays there until
        // written.
        backing.pin();

        let this = if store.stat_local(&backing)?.is_some() {
            Self::Local(LocalObject { object: backing, store })
        } else {
            // If we have a backing object, but it's not present in the local
            // tier, then it must be present in the remote tier as per our
            // eviction contract.
            //
            // The [SnapshotObjectStorage] will have to verify this before
            // committing the manifest.
            Self::Remote(RemoteObject {
                key: backing.key(),
                store,
            })
        };

        Ok(this)
    }
}

pub struct LocalObject<'a> {
    object: DiskObject,
    store: &'a TieredStorage,
}

impl LocalObject<'_> {
    pub fn hardlink_or_write(&self, dst: impl AsRef<Path>, fsync: Fsync) -> io::Result<()> {
        self.store.hardlink_or_write(&self.object, dst, fsync)
    }
}

pub struct RemoteObject<'a> {
    key: ObjectKey,
    store: &'a TieredStorage,
}

impl RemoteObject<'_> {
    pub fn write(&self, dst: impl AsRef<Path>, fsync: Fsync) -> io::Result<()> {
        let bytes = self.store.read(self.key).map_err(io::Error::other)?;
        write_atomically(&bytes, dst.as_ref(), fsync)
    }
}

pub trait SnapshotObjectStorage {
    fn write(&self, object: SnapshotObject<'_>) -> io::Result<()>;
}
