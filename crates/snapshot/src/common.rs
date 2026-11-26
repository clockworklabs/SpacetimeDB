use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    io::{BufWriter, Read as _, Write as _},
    num::NonZeroUsize,
    time::Instant,
};

use spacetimedb_durability::TxOffset;
use spacetimedb_fs_utils::{
    compression::{CompressReader, CompressType},
    dir_trie::{o_excl, o_rdonly, DirTrie},
    lockfile::Lockfile,
};
use spacetimedb_lib::bsatn;
use spacetimedb_paths::{
    server::{SnapshotDirPath, SnapshotFilePath, SnapshotsPath},
    FromPathUnchecked as _,
};
use spacetimedb_primitives::TableId;
use spacetimedb_table::{blob_store::BlobHash, page::Page, page_pool::PagePool, tiered::BlobMap};

use crate::{
    v0, v1, BlobEntry, FileOrDirPath, ObjectType, ParentRepo, Snapshot, SnapshotError, SnapshotReadKindMetrics,
    TableEntry, TablePages, MAGIC, SNAPSHOT_DIR_EXT, ZERO_HASH_DENOTING_ABSENT_PAGE,
};

pub(crate) fn read_from_file(path: &SnapshotFilePath) -> Result<(Snapshot, CompressType), SnapshotError> {
    read_from_file_with_metrics(path).map(|(snapshot, compress_type, _metrics)| (snapshot, compress_type))
}

pub(crate) fn read_from_file_with_metrics(
    path: &SnapshotFilePath,
) -> Result<(Snapshot, CompressType, SnapshotReadKindMetrics), SnapshotError> {
    let read_start = Instant::now();
    let mut read_metrics = SnapshotReadKindMetrics {
        files: 1,
        ..Default::default()
    };
    let err_read_object = |cause| SnapshotError::ReadObject {
        ty: ObjectType::Snapshot,
        source_repo: path.0.clone(),
        cause,
    };
    let snapshot_file = path.open_file(&o_rdonly()).map_err(err_read_object)?;
    read_metrics.disk_bytes = snapshot_file.metadata().map_err(err_read_object)?.len();
    let mut snapshot_file = CompressReader::new(snapshot_file)?;

    // The snapshot file is prefixed with the hash of the `Snapshot`'s BSATN.
    // Read that hash.
    let mut hash = [0; blake3::OUT_LEN];
    snapshot_file.read_exact(&mut hash).map_err(err_read_object)?;
    let hash = blake3::Hash::from_bytes(hash);

    // Figure out the version.
    // A snapshot file doesn't have a header per-se, but let's pretend the magic
    // and the version numbers are the header. Since `MAGIC` is an array, bsatn
    // prefixes it with a length (u32 le). The version number is a single byte.
    //
    // TODO: The magic is validated in [Snapshot::read_snapshot], but we could
    // do it here already and avoid I/O.
    const HEADER_LEN: usize = 4 + MAGIC.len() + 1;
    let mut magic_and_version = [8; HEADER_LEN];
    snapshot_file
        .read_exact(&mut magic_and_version)
        .map_err(err_read_object)?;
    let version = magic_and_version[HEADER_LEN - 1];

    // Read the `Snapshot`'s BSATN and compute its hash.
    let mut snapshot_bsatn = magic_and_version.to_vec();
    snapshot_file
        .read_to_end(&mut snapshot_bsatn)
        .map_err(err_read_object)?;
    let hash_start = Instant::now();
    let computed_hash = blake3::hash(&snapshot_bsatn);
    read_metrics.hash_time += hash_start.elapsed();

    // Compare the saved and computed hashes, and fail if they do not match.
    if hash != computed_hash {
        return Err(SnapshotError::HashMismatch {
            ty: ObjectType::Snapshot,
            expected: *hash.as_bytes(),
            computed: *computed_hash.as_bytes(),
            source_repo: path.0.clone(),
        });
    }

    let deserialize_err = |cause| SnapshotError::Deserialize {
        ty: ObjectType::Snapshot,
        source_repo: path.0.clone(),
        cause,
    };

    let snapshot = match version {
        v0::SNAPSHOT_VERSION => {
            let manifest: v0::Snapshot = bsatn::from_slice(&snapshot_bsatn).map_err(deserialize_err)?;
            assert_eq!(manifest.version, v0::SNAPSHOT_VERSION);
            Ok(Snapshot::V0(manifest))
        }
        v1::SNAPSHOT_VERSION => bsatn::from_slice(&snapshot_bsatn)
            .map_err(deserialize_err)
            .map(Snapshot::V1),
        v => Err(SnapshotError::BadVersion {
            tx_offset: path.snapshot_dir().tx_offset().expect("invalid snapshot directory"),
            version: v,
        }),
    }?;

    read_metrics.read_time += read_start.elapsed();

    Ok((snapshot, snapshot_file.compress_type(), read_metrics))
}

pub(crate) fn reconstruct_blob_store(
    blobs: &[BlobEntry],
    repo: &DirTrie,
    metrics: &mut SnapshotReadKindMetrics,
) -> Result<BlobMap, SnapshotError> {
    let mut blob_store = BlobMap::default();

    for BlobEntry { hash, uses } in blobs {
        // Read the bytes of the blob object.
        let (buf, disk_bytes) = read_object_with_disk_bytes(repo, &hash.data, ObjectType::Blob(*hash))?;
        metrics.disk_bytes += disk_bytes;

        // Compute the blob's hash.
        let hash_start = Instant::now();
        let computed_hash = BlobHash::hash_from_bytes(&buf);
        metrics.hash_time += hash_start.elapsed();

        // Compare the computed hash to the one recorded in the `Snapshot`,
        // and fail if they do not match.
        if *hash != computed_hash {
            return Err(SnapshotError::HashMismatch {
                ty: ObjectType::Blob(*hash),
                expected: hash.data,
                computed: computed_hash.data,
                source_repo: repo.root().to_path_buf(),
            });
        }

        if let Some(uses) = NonZeroUsize::new(*uses as usize) {
            blob_store.insert_with_uses(hash, uses, buf.into());
        }
    }

    Ok(blob_store)
}

fn read_object_with_disk_bytes(
    repo: &DirTrie,
    file_id: &[u8; blake3::OUT_LEN],
    ty: ObjectType,
) -> Result<(Vec<u8>, u64), SnapshotError> {
    let source_repo = || repo.root().to_path_buf();
    let disk_bytes = fs::metadata(repo.file_path(file_id))
        .map_err(|cause| SnapshotError::ReadObject {
            ty,
            source_repo: source_repo(),
            cause,
        })?
        .len();
    let buf = repo.read_entry(file_id).map_err(|cause| SnapshotError::ReadObject {
        ty,
        source_repo: source_repo(),
        cause,
    })?;
    Ok((buf, disk_bytes))
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
pub(crate) fn reconstruct_tables(
    tables: &[TableEntry],
    repo: &DirTrie,
    page_pool: &PagePool,
    metrics: &mut SnapshotReadKindMetrics,
) -> Result<BTreeMap<TableId, TablePages>, SnapshotError> {
    tables
        .iter()
        .map(|tbl| reconstruct_one_table(repo, tbl, page_pool, metrics))
        .collect()
}

fn reconstruct_one_table(
    repo: &DirTrie,
    TableEntry { table_id, pages }: &TableEntry,
    page_pool: &PagePool,
    metrics: &mut SnapshotReadKindMetrics,
) -> Result<(TableId, TablePages), SnapshotError> {
    Ok((*table_id, reconstruct_one_table_pages(repo, pages, page_pool, metrics)?))
}

/// Read all the pages referenced by `pages` from the `object_repo`.
///
/// Fails if any of the pages files is missing or corrupted,
/// as detected by comparing the hash of its bytes to the hash listed in `pages`.
fn reconstruct_one_table_pages(
    repo: &DirTrie,
    pages: &[blake3::Hash],
    page_pool: &PagePool,
    metrics: &mut SnapshotReadKindMetrics,
) -> Result<TablePages, SnapshotError> {
    pages
        .iter()
        .map(|hash| {
            if *hash == ZERO_HASH_DENOTING_ABSENT_PAGE {
                Ok(None)
            } else {
                // Read the BSATN bytes of the on-disk page object.
                let (buf, disk_bytes) = read_object_with_disk_bytes(repo, hash.as_bytes(), ObjectType::Page(*hash))?;
                metrics.disk_bytes += disk_bytes;

                // Deserialize the bytes into a `Page`.
                let page = page_pool.take_deserialize_from(&buf);
                let page = page.map_err(|cause| SnapshotError::Deserialize {
                    ty: ObjectType::Page(*hash),
                    source_repo: repo.root().to_path_buf(),
                    cause,
                })?;

                // Compute the hash of the page.
                let hash_start = Instant::now();
                let computed_hash = page.content_hash();
                metrics.hash_time += hash_start.elapsed();

                // Compare the computed hash to the one recorded in the `Snapshot`,
                // and fail if they do not match.
                if *hash != computed_hash {
                    return Err(SnapshotError::HashMismatch {
                        ty: ObjectType::Page(*hash),
                        expected: *hash.as_bytes(),
                        computed: *computed_hash.as_bytes(),
                        source_repo: repo.root().to_path_buf(),
                    });
                }

                Ok::<Option<Box<Page>>, SnapshotError>(Some(page))
            }
        })
        .collect()
}

pub(crate) fn write_snapshot_file(path: SnapshotFilePath, snapshot_bsatn: &[u8]) -> Result<(), SnapshotError> {
    let hash = blake3::hash(snapshot_bsatn);

    // Create the snapshot file, containing first the hash, then the `Snapshot`.
    {
        let mut snapshot_file = BufWriter::new(path.open_file(&o_excl())?);
        snapshot_file.write_all(hash.as_bytes())?;
        snapshot_file.write_all(snapshot_bsatn)?;
        snapshot_file.flush()?;
        // fsync file + enclosing directory.
        snapshot_file
            .into_inner()
            .expect("buffered writer just flushed")
            .sync_all()?;
        FileOrDirPath::Dir(&path.snapshot_dir().0).sync_all()?;
    }

    Ok(())
}

pub(crate) fn latest_snapshot_older_than(
    root: &SnapshotsPath,
    upper_bound: TxOffset,
) -> Result<Option<TxOffset>, SnapshotError> {
    all_snapshots(root).map(|iter| iter.filter(|&tx_offset| tx_offset <= upper_bound).max())
}

pub(crate) fn all_snapshots(root: &SnapshotsPath) -> Result<impl Iterator<Item = TxOffset> + use<>, SnapshotError> {
    Ok(root
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

pub(crate) fn parent_repo(root: &SnapshotsPath, before: TxOffset) -> Result<Option<ParentRepo>, SnapshotError> {
    let Some(offset) = latest_snapshot_older_than(root, before)? else {
        return Ok(None);
    };

    let parent_dir = root.snapshot_dir(offset);
    let (parent_snapshot, _) = Snapshot::read_from_file(&parent_dir.snapshot_file(offset))?;
    let parent_repo = parent_snapshot.into_parent_repo(&parent_dir)?;

    Ok(Some(parent_repo))
}
