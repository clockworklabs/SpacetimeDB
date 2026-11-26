use std::{collections::BTreeSet, io::Cursor, sync::Arc};

use proptest::{collection::vec, prelude::*};
use spacetimedb_sats::{product, AlgebraicType};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_table::{
    indexes::SquashedOffset,
    table::Table,
    tiered::{EvictionPolicy, TieredDatabaseStorage, TieredStorageConfig},
};
use tempfile::tempdir;

use super::*;
use crate::remote::{synchronize_snapshot, ObjectProvider};

const TABLE: TableId = TableId(17);
const EMPTY_TABLE: TableId = TableId(99);
const OFFSET: TxOffset = 42;
const BLOB_USES: u32 = 3;

#[derive(Clone, Copy, Debug)]
enum Version {
    V0,
    V1,
}

const VERSIONS: [Version; 2] = [Version::V0, Version::V1];

impl Version {
    fn number(self) -> u8 {
        match self {
            Self::V0 => 0,
            Self::V1 => 1,
        }
    }
}

fn v0_manifest(offset: TxOffset) -> v0::Snapshot {
    v0::Snapshot {
        magic: MAGIC,
        version: v0::SNAPSHOT_VERSION,
        database_identity: Identity::ZERO,
        replica_id: 7,
        module_abi_version: CURRENT_MODULE_ABI_VERSION,
        tx_offset: offset,
        blobs: vec![],
        tables: vec![],
    }
}

fn new_table(id: TableId, storage: &TieredDatabaseStorage) -> Table {
    let mut schema = TableSchema::from_product_type([AlgebraicType::U64].into());
    schema.table_id = id;
    Table::new(
        schema.into(),
        SquashedOffset::COMMITTED_STATE,
        storage.pages().clone(),
        EvictionPolicy::Never,
    )
}

fn capture(
    root: &SnapshotsPath,
    version: Version,
    offset: TxOffset,
    bytes: &[u8],
    row: u64,
) -> anyhow::Result<Snapshot> {
    let dir = root.snapshot_dir(offset);
    let lockfile = Lockfile::for_file(&dir)?;
    dir.create()?;
    let manifest = match version {
        Version::V0 => Snapshot::V0(v0_manifest(offset)),
        Version::V1 => Snapshot::V1(v1::Snapshot::new(v0_manifest(offset))),
    };
    let mut writable = manifest.into_writable(&dir)?;

    let storage = TieredDatabaseStorage::empty(TieredStorageConfig::new_for_test());
    let mut tables = [new_table(TABLE, &storage), new_table(EMPTY_TABLE, &storage)];
    tables[0].insert(storage.blobs(), &product![row])?;

    for _ in 0..BLOB_USES {
        storage.blobs().insert(EvictionPolicy::Memory, bytes)?;
    }

    let (blobs, tables) = storage.create_snapshot(&writable, tables.iter_mut())?.into_parts();
    writable.set_blobs(blobs.into_iter().map(|(hash, uses)| BlobEntry {
        hash,
        uses: uses.get() as _,
    }));
    writable.set_tables(tables.into_iter().map(|(table_id, pages)| {
        let mut pages = pages
            .into_iter()
            .map(|maybe_hash| maybe_hash.map(Into::into).unwrap_or(ZERO_HASH_DENOTING_ABSENT_PAGE))
            .collect::<Vec<_>>();

        if table_id == TABLE {
            pages.push(ZERO_HASH_DENOTING_ABSENT_PAGE);
        }

        TableEntry { table_id, pages }
    }));

    let manifest = writable.manifest();
    let pending = UnflushedSnapshot {
        inner: Some(UnflushedSnapshotInner {
            snapshot_dir: dir.clone(),
            repo: writable,
            lockfile,
        }),
    };
    assert_eq!(pending.tx_offset(), offset);
    assert!(!dir.snapshot_file(offset).0.exists());
    assert_eq!(pending.sync_all()?.0, dir.0);

    Ok(manifest)
}

fn at_version(manifest: &Snapshot, version: Version, offset: TxOffset) -> Snapshot {
    let mut snapshot = match (version, manifest) {
        (Version::V0, Snapshot::V0(_)) | (Version::V1, Snapshot::V1(_)) => manifest.clone(),
        (Version::V1, Snapshot::V0(inner)) => Snapshot::V1(v1::Snapshot::new(inner.clone())),
        (Version::V0, Snapshot::V1(inner)) => {
            // Downgrading is only a test fixture operation: V0 and V1 share
            // object encodings, so remote transfer can exercise either layout.
            let mut inner = (**inner).clone();
            inner.version = v0::SNAPSHOT_VERSION;
            Snapshot::V0(inner)
        }
    };
    *snapshot.tx_offset_mut() = offset;
    snapshot
}

fn assert_layout(root: &SnapshotsPath, manifest: &Snapshot, version: Version) -> anyhow::Result<()> {
    let dir = root.snapshot_dir(manifest.tx_offset());
    let writable = manifest.clone().into_writable(&dir)?;
    let mut expected = BTreeSet::new();
    for key in writable.objects() {
        // Construct expected paths independently of the repository's path resolver.
        let hash = key.to_string();
        let namespace = match (version, key) {
            (Version::V0, _) => dir.objects().0,
            (Version::V1, ObjectKey::Page(_)) => dir.objects().pages().0,
            (Version::V1, ObjectKey::Blob(_)) => dir.objects().blobs().0,
        };
        let path = namespace.join(&hash[..2]).join(&hash[2..]);
        assert_eq!(writable.file_path(key), path);
        assert!(path.is_file(), "missing object at {}", path.display());
        expected.insert(path);
    }
    assert_eq!(expected.len(), 2);

    // Compare the full object tree so a correct copy cannot hide extra files in
    // the wrong layout or an object for the absent-page sentinel.
    let mut actual = BTreeSet::new();
    let mut directories = vec![dir.objects().0];
    let mut visited = 0u32;
    while let Some(directory) = directories.pop() {
        visited += 1;
        assert!(visited <= 8, "fixture has at most eight object directories");
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                directories.push(entry.path());
            } else {
                actual.insert(entry.path());
            }
        }
    }
    assert_eq!(actual, expected);

    Ok(())
}

fn assert_restored(root: &SnapshotsPath, offset: TxOffset, bytes: &[u8], row: u64) -> anyhow::Result<()> {
    let repo = SnapshotRepository::open(root.clone(), Identity::ZERO, 7)?;
    let storage = TieredDatabaseStorage::empty(TieredStorageConfig::new_for_test());
    let pool = PagePool::new_for_test();
    let mut restored = repo.read_snapshot(offset, &pool)?;
    assert_eq!(restored.database_identity, Identity::ZERO);
    assert_eq!(restored.replica_id, 7);
    assert_eq!(restored.module_abi_version, CURRENT_MODULE_ABI_VERSION);
    assert_eq!(restored.tx_offset, offset);
    assert_eq!(restored.read_metrics.blob.files, 1);
    assert_eq!(restored.read_metrics.page.files, 1);
    assert_eq!(restored.read_metrics.absent_pages, 1);
    let blobs = restored.blob_store.iter().collect::<Vec<_>>();
    assert_eq!(blobs.len(), 1);
    assert_eq!(*blobs[0].0, BlobHash::hash_from_bytes(bytes).into());
    assert_eq!(blobs[0].1.blob.as_ref(), bytes);
    assert_eq!(blobs[0].1.uses.get() as u64, u64::from(BLOB_USES));

    // Verify reconstructed row contents as well as metadata and page hashes.
    assert_eq!(restored.tables.len(), 2);
    assert!(restored.tables.remove(&EMPTY_TABLE).unwrap().is_empty());
    let pages = restored.tables.remove(&TABLE).unwrap();
    assert_eq!(pages.len(), 2);
    assert!(pages[0].is_some());
    assert!(pages[1].is_none());
    let mut table = new_table(TABLE, &storage);
    // SAFETY: The fixture was captured from a table with this same U64 schema.
    unsafe { table.set_pages(pages, storage.blobs())? };
    let mut rows = table.scan_rows(storage.blobs());
    assert_eq!(rows.next().unwrap()?.to_product_value(), product![row]);
    assert!(rows.next().is_none());

    Ok(())
}

#[test]
fn v1_writes_separate_page_and_blob_namespaces() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let root = SnapshotsPath::from_path_unchecked(temp.path());
    let manifest = capture(&root, Version::V1, OFFSET, b"v1 blob", 123)?;
    assert_layout(&root, &manifest, Version::V1)?;
    let names = fs::read_dir(root.snapshot_dir(OFFSET).objects().0)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<BTreeSet<_>>>()?;
    assert_eq!(names, ["pages".into(), "blobs".into()].into());

    Ok(())
}

proptest! {
    #[test]
    fn v1_conversion_sets_version_one(
        source_version in any::<u8>(),
        offset in any::<u64>(),
        hash in any::<[u8; 32]>(),
        uses in any::<u32>(),
    ) {
        let mut source = v0_manifest(offset);
        source.version = source_version;
        source.blobs.push(BlobEntry { hash: BlobHash { data: hash }, uses });
        let converted = v1::Snapshot::new(source.clone());
        prop_assert_eq!(converted.version, v1::SNAPSHOT_VERSION);

        // Conversion changes only the wire version and must serialize as V1
        // without the fixture correcting the stored version field.
        source.version = v1::SNAPSHOT_VERSION;
        let encoded = bsatn::to_vec(&converted).unwrap();
        prop_assert_eq!(&encoded, &bsatn::to_vec(&source).unwrap());
        let decoded: v1::Snapshot = bsatn::from_slice(&encoded).unwrap();
        prop_assert_eq!(decoded.version, v1::SNAPSHOT_VERSION);
    }

    #[test]
    fn reads_both_versions(
        bytes in vec(any::<u8>(), 0..1024),
        row in any::<u64>(),
        offset in any::<u64>(),
        compressed in any::<bool>(),
    ) {
        for version in VERSIONS {
            let temp = tempdir().unwrap();
            let root = SnapshotsPath::from_path_unchecked(temp.path());
            let expected = capture(&root, version, offset, &bytes, row).unwrap();
            if compressed {
                SnapshotRepository::compress_snapshot(None, &(offset, root.snapshot_dir(offset)))
                    .unwrap();
            }

            // Both metadata decoding and the public restoring API must accept
            // old snapshots, independently of the current write-version constant.
            let file = root.snapshot_dir(offset).snapshot_file(offset);
            let (decoded, compression) = Snapshot::read_from_file(&file).unwrap();
            prop_assert_eq!(decoded.version(), version.number());
            prop_assert_eq!(compression.is_compressed(), compressed);
            prop_assert_eq!(bsatn::to_vec(&decoded).unwrap(), bsatn::to_vec(&expected).unwrap());
            assert_layout(&root, &decoded, version).unwrap();
            assert_restored(&root, offset, &bytes, row).unwrap();
        }
    }
}

#[test]
fn v1_deserialization_requires_version_one() -> anyhow::Result<()> {
    // Exhaust every possible version byte, including V0 and future versions.
    for version in 0..=u8::MAX {
        let mut manifest = v0_manifest(OFFSET);
        manifest.version = version;
        let encoded = bsatn::to_vec(&manifest)?;
        let decoded = bsatn::from_slice::<v1::Snapshot>(&encoded);
        if version == v1::SNAPSHOT_VERSION {
            assert_eq!(decoded?.version, v1::SNAPSHOT_VERSION);
        } else {
            assert!(decoded.is_err(), "V1 accepted wire version {version}");
        }
    }

    Ok(())
}

#[test]
fn rejects_unsupported_versions() -> anyhow::Result<()> {
    for version in [2, u8::MAX] {
        let temp = tempdir()?;
        let root = SnapshotsPath::from_path_unchecked(temp.path());
        let mut manifest = capture(&root, Version::V0, OFFSET, b"blob", 123)?;
        let Snapshot::V0(inner) = &mut manifest else {
            unreachable!()
        };
        inner.version = version;
        let file = root.snapshot_dir(OFFSET).snapshot_file(OFFSET);
        fs::remove_file(&file)?;
        common::write_snapshot_file(file.clone(), &bsatn::to_vec(&manifest)?)?;

        for error in [
            Snapshot::read_from_file(&file)
                .err()
                .expect("unsupported metadata version"),
            SnapshotRepository::open(root.clone(), Identity::ZERO, 7)?
                .read_snapshot(OFFSET, &PagePool::new_for_test())
                .err()
                .expect("unsupported restore version"),
        ] {
            assert!(matches!(error, SnapshotError::BadVersion { version: actual, .. }
                if actual == version));
        }
    }

    Ok(())
}

#[tokio::test]
async fn remote_selects_layout_from_received_version() -> anyhow::Result<()> {
    let bytes = b"a remotely transferred blob";
    for version in VERSIONS {
        let source = tempdir()?;
        let source_root = SnapshotsPath::from_path_unchecked(source.path());
        capture(&source_root, version, OFFSET, bytes, 123)?;

        // Transfer the source's persisted manifest unchanged. Its version must
        // determine the receiver's layout, regardless of the current write version.
        let source_dir = source_root.snapshot_dir(OFFSET);
        let (received, _) = Snapshot::read_from_file(&source_dir.snapshot_file(OFFSET))?;
        assert_eq!(received.version(), version.number());
        let provider = received.clone().into_writable(&source_dir)?;
        let target = tempdir()?;
        let root = SnapshotsPath::from_path_unchecked(target.path());
        let stats = synchronize_snapshot(provider, root.clone(), received.clone()).await?;
        assert_eq!(stats.objects_written, 2);
        assert_eq!(stats.objects_hardlinked, 0);
        assert_layout(&root, &received, version)?;
        assert_restored(&root, OFFSET, bytes, 123)?;

        // Read back the destination's manifest and verify its objects through
        // the destination's provider, including version-specific object routing.
        let (decoded, _) = Snapshot::read_from_file(&root.snapshot_dir(OFFSET).snapshot_file(OFFSET))?;
        assert_eq!(decoded.version(), received.version());
        assert_eq!(bsatn::to_vec(&decoded)?, bsatn::to_vec(&received)?);
        let destination = decoded.clone().into_writable(&root.snapshot_dir(OFFSET))?;
        remote::verify_snapshot(destination, root, decoded).await?;
    }

    Ok(())
}

fn no_fetch() -> impl ObjectProvider {
    |_key| async { Err::<Cursor<Vec<u8>>, _>(io::Error::other("existing objects must not be fetched")) }
}

#[tokio::test]
async fn remote_hardlinks_across_versions_and_skips_existing_objects() -> anyhow::Result<()> {
    let bytes = b"a blob shared across snapshot formats";
    let source = tempdir()?;
    let source_root = SnapshotsPath::from_path_unchecked(source.path());
    let original = capture(&source_root, Version::V1, OFFSET, bytes, 123)?;
    let provider = Arc::new(original.clone().into_writable(&source_root.snapshot_dir(OFFSET))?);
    for parent_version in VERSIONS {
        for child_version in VERSIONS {
            let target = tempdir()?;
            let root = SnapshotsPath::from_path_unchecked(target.path());
            let parent = at_version(&original, parent_version, OFFSET);
            synchronize_snapshot(provider.clone(), root.clone(), parent).await?;
            let child = at_version(&original, child_version, OFFSET + 1);

            // A provider that always fails proves both layouts can reuse a
            // parent without silently fetching objects into the target layout.
            let stats = synchronize_snapshot(no_fetch(), root.clone(), child.clone()).await?;
            assert_eq!(stats.objects_hardlinked, 2);
            assert_eq!(stats.objects_written, 0);
            assert_eq!(stats.objects_skipped, 0);
            assert_layout(&root, &child, child_version)?;
            assert_restored(&root, OFFSET + 1, bytes, 123)?;

            let stats = synchronize_snapshot(no_fetch(), root.clone(), child).await?;
            assert_eq!(stats.objects_skipped, 2);
            assert_eq!(stats.objects_hardlinked, 0);
            assert_eq!(stats.objects_written, 0);
        }
    }

    Ok(())
}
