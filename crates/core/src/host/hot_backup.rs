use crate::db::relational_db::{open_snapshot_repo, RelationalDB, TxOffset};
use crate::util::asyncify;
use anyhow::{anyhow, Context};
use parking_lot::Mutex;
use spacetimedb_commitlog::repo::Repo;
use spacetimedb_commitlog::{self as commitlog};
use spacetimedb_fs_utils::{
    atomic_write_bytes, copy_dir_all, copy_file_sync, create_dir_all_sync, dir_size, normalize_absolute_path, sync_dir,
};
use spacetimedb_lib::Identity;
use spacetimedb_paths::server::{CommitLogDir, ReplicaDir, ServerDataDir};
use spacetimedb_paths::FromPathUnchecked;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, serde::Serialize)]
pub struct HotBackupManifest {
    pub version: u32,
    pub database_identity: Identity,
    pub replica_id: u64,
    pub snapshot_offset: TxOffset,
    pub durable_offset: TxOffset,
    pub output_dir: PathBuf,
    pub snapshot_ms: u64,
    pub copy_ms: u64,
    pub total_ms: u64,
    pub bytes: u64,
}

impl HotBackupManifest {
    /// Convert a [`Duration`] to whole milliseconds, saturating at `u64::MAX`.
    ///
    /// Used to fill in the `*_ms` timing fields of the manifest.
    pub fn elapsed_ms(duration: Duration) -> u64 {
        duration.as_millis().try_into().unwrap_or(u64::MAX)
    }
}

/// A hot backup whose output directory is still being finalized.
///
/// Dropping this value releases the in-process guard for its output directory.
/// Call [`Self::finalize_with_blocking`] to add caller-owned state and write the
/// final `manifest.json` while the guard is still held.
pub struct HotBackupInProgress {
    manifest: HotBackupManifest,
    _dir_guard: BackupDirGuard,
}

impl HotBackupInProgress {
    /// The manifest that will be written when this backup is finalized.
    pub fn manifest(&self) -> &HotBackupManifest {
        &self.manifest
    }

    fn into_manifest(self) -> HotBackupManifest {
        self.manifest
    }

    /// Run caller-provided blocking finalization work, then write
    /// `manifest.json` while still holding the backup output directory guard.
    ///
    /// The guard is moved into the blocking task with the manifest. If the
    /// awaiting future is cancelled, the blocking task continues to own the
    /// guard until the caller's work and final manifest write have returned.
    pub async fn finalize_with_blocking<F>(self, f: F) -> anyhow::Result<HotBackupManifest>
    where
        F: FnOnce(&mut HotBackupManifest) -> anyhow::Result<()> + Send + 'static,
    {
        asyncify(move || {
            let mut backup = self;
            f(&mut backup.manifest)?;
            write_hot_backup_manifest_sync(&backup.manifest.output_dir, &backup.manifest)?;
            Ok(backup.into_manifest())
        })
        .await
    }
}

/// Export a point-in-time hot backup into `output_dir`.
#[cfg(test)]
async fn create_hot_backup(
    db: &RelationalDB,
    replica_dir: &ReplicaDir,
    server_data_dir: Option<&ServerDataDir>,
    replica_id: u64,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<HotBackupManifest> {
    let backup = create_hot_backup_inner(db, replica_dir, server_data_dir, replica_id, output_dir, true).await?;
    Ok(backup.into_manifest())
}

/// Export a point-in-time hot backup into `output_dir`, leaving scoped server
/// state for the caller to provide.
///
/// Unlike a finalized hot backup, this does **not** compute
/// [`HotBackupManifest::bytes`] (it is left at 0) and does **not** write
/// `manifest.json`: the caller is expected to add server state that must be
/// exported with broader context, such as `server/control-db`, and then finalize
/// the manifest itself. This both avoids traversing the backup tree twice and
/// makes `manifest.json` a reliable marker of a fully-written backup.
pub(super) async fn create_hot_backup_without_control_db(
    db: &RelationalDB,
    replica_dir: &ReplicaDir,
    server_data_dir: Option<&ServerDataDir>,
    replica_id: u64,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<HotBackupInProgress> {
    create_hot_backup_inner(db, replica_dir, server_data_dir, replica_id, output_dir, false).await
}

async fn create_hot_backup_inner(
    db: &RelationalDB,
    replica_dir: &ReplicaDir,
    server_data_dir: Option<&ServerDataDir>,
    replica_id: u64,
    output_dir: impl AsRef<Path>,
    copy_control_db: bool,
) -> anyhow::Result<HotBackupInProgress> {
    let total_start = Instant::now();
    let output_dir = ensure_backup_path(replica_dir, server_data_dir, output_dir.as_ref())?;
    // Guard against two concurrent backups interleaving writes into the same
    // directory after both passed the `ensure_empty_dir` check.
    let dir_guard = BackupDirGuard::acquire(&output_dir)?;
    anyhow::ensure!(
        !(copy_control_db && server_data_dir.is_some()),
        "copying live server/control-db is not supported by hot backup; export control-db separately and finalize the manifest after the export"
    );

    let snapshot_start = Instant::now();
    let snapshot_offset = db.request_hot_backup_snapshot().await?;
    let snapshot_ms = HotBackupManifest::elapsed_ms(snapshot_start.elapsed());
    let durable_offset = snapshot_offset;

    let copy_start = Instant::now();
    let snapshot_repo = open_snapshot_repo(replica_dir.snapshots(), db.database_identity(), replica_id)?;
    let src_snapshot_dir = snapshot_repo.snapshot_dir_path(snapshot_offset);
    let dst_snapshots = output_dir.join("snapshots");
    let dst_snapshot_dir = dst_snapshots.join(
        src_snapshot_dir
            .0
            .file_name()
            .context("snapshot directory has no file name")?,
    );
    // `request_hot_backup_snapshot` returns only after the snapshot worker has
    // fsynced the snapshot to disk (see `SnapshotWorker::ensure_snapshot_at_least`),
    // so both paths must already exist; a missing path is a bug, not a timing issue.
    anyhow::ensure!(
        src_snapshot_dir.0.is_dir(),
        "snapshot directory does not exist: {}",
        src_snapshot_dir.display()
    );
    let src_snapshot_file = src_snapshot_dir.snapshot_file(snapshot_offset);
    anyhow::ensure!(
        src_snapshot_file.0.is_file(),
        "snapshot file does not exist: {}",
        src_snapshot_file.display()
    );
    let snapshot_file_name: OsString = src_snapshot_file
        .0
        .file_name()
        .context("snapshot file has no file name")?
        .to_owned();

    asyncify({
        let output_dir = output_dir.clone();
        let src_snapshot_dir = src_snapshot_dir.clone();
        let snapshot_file_name = snapshot_file_name.clone();
        let dir_guard = dir_guard.clone();
        move || -> anyhow::Result<()> {
            ensure_empty_dir(&output_dir, &dir_guard)?;
            copy_dir_all_retry(&src_snapshot_dir.0, &dst_snapshot_dir, &snapshot_file_name)?;
            Ok(())
        }
    })
    .await?;

    if let Some(server_data_dir) = server_data_dir {
        copy_server_state(
            server_data_dir,
            &output_dir,
            copy_control_db,
            commitlog::DEFAULT_LOG_FORMAT_VERSION,
            dir_guard.clone(),
        )
        .await?;
    }
    copy_commitlog_range(
        replica_dir.commit_log(),
        output_dir.join("clog"),
        snapshot_offset,
        dir_guard.clone(),
    )
    .await?;
    let copy_ms = HotBackupManifest::elapsed_ms(copy_start.elapsed());

    let mut manifest = HotBackupManifest {
        version: 1,
        database_identity: db.database_identity(),
        replica_id,
        snapshot_offset,
        durable_offset,
        output_dir: output_dir.clone(),
        snapshot_ms,
        copy_ms,
        total_ms: HotBackupManifest::elapsed_ms(total_start.elapsed()),
        bytes: 0,
    };
    // When the caller provides `server/control-db` itself, leave `bytes`
    // and `manifest.json` to it, so the backup tree is only traversed once
    // and the manifest is written exactly once, after the backup is complete.
    if copy_control_db {
        manifest.bytes = asyncify({
            let output_dir = output_dir.clone();
            let dir_guard = dir_guard.clone();
            move || {
                let _dir_guard = dir_guard;
                dir_size(&output_dir)
            }
        })
        .await?;
        write_hot_backup_manifest_guarded(&output_dir, &manifest, dir_guard.clone()).await?;
    }
    Ok(HotBackupInProgress {
        manifest,
        _dir_guard: dir_guard,
    })
}

async fn copy_commitlog_range(
    src: CommitLogDir,
    dst: PathBuf,
    through: TxOffset,
    dir_guard: BackupDirGuard,
) -> anyhow::Result<()> {
    asyncify(move || -> anyhow::Result<()> {
        let _dir_guard = dir_guard;
        copy_commitlog_range_sync(src, &dst, through)?;
        sync_dir(&dst).with_context(|| format!("syncing backup commitlog dir {}", dst.display()))?;
        if let Some(parent) = dst.parent() {
            sync_dir(parent).with_context(|| format!("syncing backup commitlog parent {}", parent.display()))?;
        }
        Ok(())
    })
    .await?;
    Ok(())
}

fn copy_commitlog_range_sync(src: CommitLogDir, dst: &Path, through: TxOffset) -> anyhow::Result<()> {
    let src_repo = commitlog::repo::Fs::new(src, None)?;
    let dst_repo = commitlog::repo::Fs::new(CommitLogDir::from_path_unchecked(dst), None)?;
    let mut copied_any_commit = false;
    let mut next_tx_offset = None;
    for segment_offset in src_repo.existing_offsets()? {
        let reader =
            commitlog::repo::open_segment_reader(&src_repo, commitlog::DEFAULT_LOG_FORMAT_VERSION, segment_offset)
                .with_context(|| format!("opening source commitlog segment {segment_offset}"))?;
        let mut writer = None;
        for commit in reader.commits() {
            let commit = commit.with_context(|| format!("reading source commitlog segment {segment_offset}"))?;
            if commit.min_tx_offset > through {
                break;
            }
            let commit_range = commit.tx_range();
            if let Some(expected_tx_offset) = next_tx_offset {
                anyhow::ensure!(
                    commit.min_tx_offset == expected_tx_offset,
                    "source commitlog is not contiguous at segment {segment_offset}: expected tx offset {expected_tx_offset}, got {}",
                    commit.min_tx_offset
                );
            }
            next_tx_offset = Some(commit_range.end);
            let writer = writer.get_or_insert_with(|| {
                dst_repo.create_segment(
                    segment_offset,
                    commitlog::segment::Header {
                        log_format_version: commitlog::DEFAULT_LOG_FORMAT_VERSION,
                        checksum_algorithm: commitlog::Commit::CHECKSUM_ALGORITHM,
                    },
                )
            });
            let writer = writer
                .as_mut()
                .map_err(|err| anyhow!("creating target commitlog segment {segment_offset}: {err}"))?;
            commitlog::Commit::from(commit)
                .write(&mut *writer)
                .with_context(|| format!("writing target commitlog segment {segment_offset}"))?;
            copied_any_commit = true;
        }
        if let Some(writer) = writer {
            let mut writer = writer.with_context(|| format!("creating target commitlog segment {segment_offset}"))?;
            writer
                .flush()
                .with_context(|| format!("flushing target commitlog segment {segment_offset}"))?;
            writer
                .sync_data()
                .with_context(|| format!("syncing target commitlog segment {segment_offset}"))?;
        }
    }
    anyhow::ensure!(
        copied_any_commit,
        "source commitlog does not contain commits through snapshot offset {through}"
    );
    Ok(())
}

async fn write_hot_backup_manifest_guarded(
    output_dir: &Path,
    manifest: &HotBackupManifest,
    dir_guard: BackupDirGuard,
) -> anyhow::Result<()> {
    let output_dir = output_dir.to_path_buf();
    let json = serde_json::to_vec_pretty(manifest)?;
    asyncify(move || {
        let _dir_guard = dir_guard;
        write_hot_backup_manifest_bytes(&output_dir, &json)
    })
    .await?;
    Ok(())
}

fn write_hot_backup_manifest_sync(output_dir: &Path, manifest: &HotBackupManifest) -> anyhow::Result<()> {
    let json = serde_json::to_vec_pretty(manifest)?;
    write_hot_backup_manifest_bytes(output_dir, &json)
}

fn write_hot_backup_manifest_bytes(output_dir: &Path, json: &[u8]) -> anyhow::Result<()> {
    atomic_write_bytes(&output_dir.join("manifest.json"), json)
        .with_context(|| format!("writing hot backup manifest in {}", output_dir.display()))
}

const BACKUP_DIR_CLAIM_MARKER: &str = ".spacetimedb-hot-backup-in-progress";

/// Serializes concurrent hot backups into overlapping output directories in
/// this process and claims the exact output directory across processes.
///
/// Two concurrent backups into the same (empty) directory would both pass the
/// `ensure_empty_dir` check and interleave their writes, producing a corrupt
/// backup. The process-wide map rejects overlapping paths locally, while a
/// hidden `create_new` marker atomically claims the exact path on the
/// filesystem. Both claims are released when the last guard clone drops.
struct BackupDirGuard(PathBuf);

static ACTIVE_BACKUP_DIRS: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());

impl BackupDirGuard {
    fn acquire(output_dir: &Path) -> anyhow::Result<Self> {
        let mut active = ACTIVE_BACKUP_DIRS.lock();
        if let Some(overlapping) = active
            .keys()
            .find(|active_dir| backup_paths_overlap(output_dir, active_dir))
        {
            anyhow::bail!(
                "backup output path {} overlaps with in-progress backup {}",
                output_dir.display(),
                overlapping.display()
            );
        }

        ensure_no_claimed_ancestor(output_dir)?;
        create_dir_all_sync(output_dir)
            .with_context(|| format!("creating backup output directory {}", output_dir.display()))?;
        let marker_path = Self::marker_path_for(output_dir);
        let marker = match OpenOptions::new().write(true).create_new(true).open(&marker_path) {
            Ok(marker) => marker,
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                anyhow::bail!(
                    "backup output directory {} is already claimed by marker {}",
                    output_dir.display(),
                    marker_path.display()
                )
            }
            Err(err) => {
                return Err(err).with_context(|| format!("claiming backup output directory {}", output_dir.display()))
            }
        };
        let claim_result = marker
            .sync_all()
            .map_err(anyhow::Error::from)
            .and_then(|()| ensure_no_claimed_ancestor(output_dir))
            .and_then(|()| sync_dir(output_dir).map_err(anyhow::Error::from));
        drop(marker);
        if let Err(err) = claim_result {
            let _ = std::fs::remove_file(&marker_path);
            let _ = sync_dir(output_dir);
            return Err(err).with_context(|| format!("persisting backup output claim {}", marker_path.display()));
        }

        active.insert(output_dir.to_path_buf(), 1);
        Ok(Self(output_dir.to_path_buf()))
    }

    fn marker_path_for(output_dir: &Path) -> PathBuf {
        output_dir.join(BACKUP_DIR_CLAIM_MARKER)
    }

    fn marker_path(&self) -> PathBuf {
        Self::marker_path_for(&self.0)
    }
}

fn ensure_no_claimed_ancestor(output_dir: &Path) -> anyhow::Result<()> {
    for ancestor in output_dir.ancestors().skip(1) {
        let marker_path = BackupDirGuard::marker_path_for(ancestor);
        anyhow::ensure!(
            !marker_path
                .try_exists()
                .with_context(|| format!("checking backup output claim {}", marker_path.display()))?,
            "backup output directory {} is inside claimed backup directory {}",
            output_dir.display(),
            ancestor.display()
        );
    }
    Ok(())
}

impl Clone for BackupDirGuard {
    fn clone(&self) -> Self {
        let mut active = ACTIVE_BACKUP_DIRS.lock();
        let count = active
            .get_mut(&self.0)
            .expect("cloning active backup directory guard after it was released");
        *count = count
            .checked_add(1)
            .expect("active backup directory guard refcount overflow");
        Self(self.0.clone())
    }
}

#[cfg(windows)]
fn backup_path_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect()
}

#[cfg(windows)]
fn backup_path_starts_with(path: &Path, base: &Path) -> bool {
    let path = backup_path_components(path);
    let base = backup_path_components(base);
    base.len() <= path.len() && path.iter().zip(&base).all(|(path, base)| path == base)
}

#[cfg(not(windows))]
fn backup_path_starts_with(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

fn backup_paths_overlap(a: &Path, b: &Path) -> bool {
    backup_path_starts_with(a, b) || backup_path_starts_with(b, a)
}

impl Drop for BackupDirGuard {
    fn drop(&mut self) {
        let mut active = ACTIVE_BACKUP_DIRS.lock();
        let Some(count) = active.get_mut(&self.0) else {
            panic!("dropping backup directory guard after it was released");
        };
        if *count == 1 {
            let _ = std::fs::remove_file(self.marker_path());
            let _ = sync_dir(&self.0);
            active.remove(&self.0);
        } else {
            *count -= 1;
        }
    }
}

fn ensure_empty_dir(path: &Path, dir_guard: &BackupDirGuard) -> anyhow::Result<()> {
    anyhow::ensure!(
        path == dir_guard.0.as_path(),
        "backup output guard does not own directory: {}",
        path.display()
    );
    anyhow::ensure!(
        path.is_dir(),
        "backup output path is not a directory: {}",
        path.display()
    );

    let marker_path = dir_guard.marker_path();
    let mut found_marker = false;
    for entry in path.read_dir()? {
        let entry = entry?;
        anyhow::ensure!(
            entry.path() == marker_path,
            "backup output directory must be empty: {}",
            path.display()
        );
        let file_type = entry.file_type()?;
        let metadata = entry.metadata()?;
        anyhow::ensure!(
            file_type.is_file() && metadata.len() == 0,
            "backup output claim marker is invalid: {}",
            marker_path.display()
        );
        found_marker = true;
    }
    anyhow::ensure!(
        found_marker,
        "backup output claim marker is missing: {}",
        marker_path.display()
    );
    Ok(())
}

/// Validate `output_dir` as a hot backup destination and return its normalized
/// form.
///
/// The path must be absolute and, after resolving symlinks in its existing
/// ancestors and rejecting `..` components (see
/// [`spacetimedb_fs_utils::normalize_absolute_path`]), must not point inside
/// the server data directory or the replica directory. All subsequent backup
/// I/O must use the returned path, so the checks cannot be bypassed via
/// symlinks or path traversal.
fn ensure_backup_path(
    replica_dir: &ReplicaDir,
    server_data_dir: Option<&ServerDataDir>,
    output_dir: &Path,
) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        output_dir.is_absolute(),
        "backup output directory must be an absolute server path: {}",
        output_dir.display()
    );
    let output_dir = normalize_absolute_path(output_dir).context("normalizing the backup output directory path")?;
    if let Some(server_data_dir) = server_data_dir {
        let data_dir = normalize_absolute_path(&server_data_dir.0).unwrap_or_else(|_| server_data_dir.0.to_path_buf());
        anyhow::ensure!(
            !output_dir.starts_with(&data_dir),
            "backup output directory must not be inside the server data directory: {}",
            output_dir.display()
        );
    }
    let replica_dir_normalized =
        normalize_absolute_path(&replica_dir.0).unwrap_or_else(|_| replica_dir.0.to_path_buf());
    anyhow::ensure!(
        !output_dir.starts_with(&replica_dir_normalized),
        "backup output directory must not be inside the replica directory: {}",
        output_dir.display()
    );
    Ok(output_dir)
}

async fn copy_server_state(
    data_dir: &ServerDataDir,
    output_dir: &Path,
    copy_control_db: bool,
    output_log_format_version: u8,
    dir_guard: BackupDirGuard,
) -> anyhow::Result<()> {
    let data_dir = data_dir.clone();
    let output_dir = output_dir.to_path_buf();
    asyncify(move || -> anyhow::Result<()> {
        let _dir_guard = dir_guard;
        assert!(!copy_control_db);
        let server_dir = output_dir.join("server");
        create_dir_all_sync(&server_dir)?;

        copy_server_config(
            &data_dir.config_toml().0,
            &server_dir.join("config.toml"),
            output_log_format_version,
        )?;
        copy_required_file(&data_dir.metadata_toml().0, &server_dir.join("metadata.toml"))?;
        sync_dir(&server_dir)?;
        Ok(())
    })
    .await?;
    Ok(())
}

fn copy_server_config(src: &Path, dst: &Path, output_log_format_version: u8) -> anyhow::Result<()> {
    anyhow::ensure!(src.is_file(), "server state file is missing: {}", src.display());
    let source = std::fs::read_to_string(src).with_context(|| format!("reading {}", src.display()))?;
    let mut config: toml::Table =
        toml::from_str(&source).with_context(|| format!("parsing server config {}", src.display()))?;
    let commitlog = config
        .entry("commitlog".to_owned())
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .with_context(|| format!("server config [commitlog] is not a table in {}", src.display()))?;
    commitlog.insert(
        "log-format-version".to_owned(),
        toml::Value::Integer(i64::from(output_log_format_version)),
    );

    let serialized = toml::to_string_pretty(&config).context("serializing backup server config")?;
    atomic_write_bytes(dst, serialized.as_bytes())
        .with_context(|| format!("writing backup server config {}", dst.display()))?;
    Ok(())
}

fn copy_required_file(src: &Path, dst: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(src.is_file(), "server state file is missing: {}", src.display());
    if let Some(parent) = dst.parent() {
        create_dir_all_sync(parent)?;
    }
    copy_file_sync(src, dst).with_context(|| format!("copying {} to {}", src.display(), dst.display()))?;
    Ok(())
}

fn copy_dir_all_retry(src: &Path, dst: &Path, required_file_name: &OsString) -> anyhow::Result<()> {
    const RETRY_LIMIT: u32 = 3_000;
    const RETRY_SLEEP: Duration = Duration::from_millis(10);

    let mut last_err = None;
    for _ in 0..RETRY_LIMIT {
        match copy_snapshot_dir(src, dst, required_file_name) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                std::thread::sleep(RETRY_SLEEP);
            }
        }
    }
    let err = last_err.expect("copy_dir_all_retry loop must run at least once");
    Err(err).with_context(|| format!("copying snapshot {} to {}", src.display(), dst.display()))
}

fn copy_snapshot_dir(src: &Path, dst: &Path, required_file_name: &OsString) -> anyhow::Result<()> {
    create_dir_all_sync(dst)?;
    let src_required = src.join(required_file_name);
    let dst_required = dst.join(required_file_name);
    copy_file_sync(&src_required, &dst_required)
        .with_context(|| format!("copying {} to {}", src_required.display(), dst_required.display()))?;

    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        if entry.file_name() == *required_file_name {
            continue;
        }
        let ty = entry.file_type()?;
        let dst = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst)?;
        } else {
            copy_file_sync(&entry.path(), &dst)?;
        }
    }
    anyhow::ensure!(
        dst_required.is_file(),
        "copying snapshot {} to {} did not copy {}",
        src.display(),
        dst.display(),
        required_file_name.to_string_lossy()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use super::*;
    use crate::db::relational_db::tests_utils::{begin_mut_tx, TestDB};
    use crate::db::relational_db::Txdata;
    use spacetimedb_commitlog::Commitlog;
    use spacetimedb_lib::db::raw_def::v9::{RawModuleDefV9Builder, RawTableDefBuilder};
    use spacetimedb_paths::server::{CommitLogDir, SnapshotsPath};
    use spacetimedb_paths::FromPathUnchecked;
    use spacetimedb_primitives::TableId;
    use spacetimedb_sats::raw_identifier::RawIdentifier;
    use spacetimedb_sats::{AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;
    use spacetimedb_schema::schema::{Schema as _, TableSchema};
    use spacetimedb_table::page_pool::PagePool;

    const HOT_BACKUP_TEST_TIMEOUT: Duration = Duration::from_secs(60);

    fn my_table(col_type: AlgebraicType) -> TableSchema {
        table("MyTable", ProductType::from([("my_col", col_type)]), |builder| builder)
    }

    fn table(
        name: &str,
        columns: ProductType,
        f: impl FnOnce(RawTableDefBuilder<'_>) -> RawTableDefBuilder,
    ) -> TableSchema {
        let mut builder = RawModuleDefV9Builder::new();
        f(builder.build_table_with_new_type(RawIdentifier::new(name), columns, true));
        let raw = builder.finish();
        let def: ModuleDef = raw.try_into().expect("table validation failed");
        let table = def.table(name).expect("table not found");
        TableSchema::from_module_def(&def, table, (), TableId::SENTINEL)
    }

    fn add_table(stdb: &RelationalDB) -> anyhow::Result<()> {
        let mut tx = begin_mut_tx(stdb);
        stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.commit_tx(tx)?;
        Ok(())
    }

    fn write_test_server_state(data_dir: &Path, config: &str) -> std::io::Result<()> {
        std::fs::write(data_dir.join("config.toml"), config)?;
        std::fs::write(data_dir.join("metadata.toml"), b"metadata")?;
        Ok(())
    }

    #[test]
    fn hot_backup_create_writes_manifest_snapshot_and_commitlog() -> anyhow::Result<()> {
        let stdb = TestDB::durable()?;
        add_table(&stdb)?;

        let backup_dir = tempfile::tempdir()?;
        let manifest = stdb.runtime().unwrap().block_on(async {
            tokio::time::timeout(
                HOT_BACKUP_TEST_TIMEOUT,
                create_hot_backup(&stdb, stdb.path().unwrap(), None, 0, backup_dir.path()),
            )
            .await
        })??;

        assert!(backup_dir.path().join("manifest.json").is_file());
        assert!(manifest.bytes > 0);
        assert_eq!(manifest.durable_offset, manifest.snapshot_offset);
        let manifest_json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(backup_dir.path().join("manifest.json"))?)?;
        assert_eq!(
            manifest_json["snapshot_offset"].as_u64(),
            Some(manifest.snapshot_offset)
        );
        assert_eq!(manifest_json["durable_offset"].as_u64(), Some(manifest.snapshot_offset));
        let repo = open_snapshot_repo(
            SnapshotsPath::from_path_unchecked(backup_dir.path().join("snapshots")),
            stdb.database_identity(),
            0,
        )?;
        let snapshot = repo.read_snapshot(manifest.snapshot_offset, &PagePool::new_for_test())?;
        assert_eq!(snapshot.database_identity, manifest.database_identity);
        assert_eq!(snapshot.replica_id, manifest.replica_id);
        assert_eq!(snapshot.tx_offset, manifest.snapshot_offset);
        let clog = Commitlog::<Txdata>::open(
            CommitLogDir::from_path_unchecked(backup_dir.path().join("clog")),
            Default::default(),
            None,
        )?;
        assert_eq!(clog.max_committed_offset(), Some(manifest.snapshot_offset));

        Ok(())
    }

    #[test]
    fn hot_backup_create_without_control_db_rewrites_v0_server_config() -> anyhow::Result<()> {
        let stdb = TestDB::durable()?;
        add_table(&stdb)?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());
        let source_config = r#"[commitlog]
log-format-version = 0
max-segment-size = 4096

[logs]
directives = ["info"]
"#;
        write_test_server_state(data.path(), source_config)?;
        std::fs::create_dir_all(data.path().join("control-db"))?;
        std::fs::write(data.path().join("control-db/control"), b"control")?;

        let backup_dir = tempfile::tempdir()?;
        let backup = stdb.runtime().unwrap().block_on(async {
            tokio::time::timeout(
                HOT_BACKUP_TEST_TIMEOUT,
                create_hot_backup_without_control_db(
                    &stdb,
                    stdb.path().unwrap(),
                    Some(&data_dir),
                    0,
                    backup_dir.path(),
                ),
            )
            .await
        })??;

        assert_eq!(backup.manifest().bytes, 0);
        assert!(!backup_dir.path().join("manifest.json").exists());
        assert_eq!(std::fs::read_to_string(data.path().join("config.toml"))?, source_config);
        let backup_config: toml::Value =
            toml::from_str(&std::fs::read_to_string(backup_dir.path().join("server/config.toml"))?)?;
        assert_eq!(
            backup_config["commitlog"]["log-format-version"].as_integer(),
            Some(i64::from(commitlog::DEFAULT_LOG_FORMAT_VERSION))
        );
        assert_eq!(backup_config["commitlog"]["max-segment-size"].as_integer(), Some(4096));
        assert_eq!(backup_config["logs"]["directives"][0].as_str(), Some("info"));
        assert_eq!(
            std::fs::read_to_string(backup_dir.path().join("server/metadata.toml"))?,
            "metadata"
        );
        assert!(!backup_dir.path().join("server/control-db").exists());
        assert!(!backup_dir.path().join("server/program-bytes").exists());

        let output_commitlog =
            commitlog::repo::Fs::new(CommitLogDir::from_path_unchecked(backup_dir.path().join("clog")), None)?;
        let segment_offset = output_commitlog
            .existing_offsets()?
            .into_iter()
            .next()
            .context("backup commitlog has no segments")?;
        let segment = output_commitlog.open_segment_reader(segment_offset)?;
        let header = commitlog::segment::Header::decode(segment)?;
        assert_eq!(header.log_format_version, commitlog::DEFAULT_LOG_FORMAT_VERSION);

        Ok(())
    }

    #[test]
    fn hot_backup_without_control_db_keeps_output_guard_until_manifest_finalization() -> anyhow::Result<()> {
        let stdb = TestDB::durable()?;
        add_table(&stdb)?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());
        write_test_server_state(data.path(), "[commitlog]\nlog-format-version = 1\n")?;

        let backup_dir = tempfile::tempdir()?;
        let backup = stdb.runtime().unwrap().block_on(async {
            tokio::time::timeout(
                HOT_BACKUP_TEST_TIMEOUT,
                create_hot_backup_without_control_db(
                    &stdb,
                    stdb.path().unwrap(),
                    Some(&data_dir),
                    0,
                    backup_dir.path(),
                ),
            )
            .await
        })??;
        let output_dir = backup.manifest().output_dir.clone();

        let err = match BackupDirGuard::acquire(&output_dir.join("nested")) {
            Ok(_) => panic!("overlapping backup directory guard was accepted"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("overlaps"));

        drop(backup);
        let guard = BackupDirGuard::acquire(&output_dir.join("nested"))?;
        drop(guard);

        Ok(())
    }

    #[test]
    fn hot_backup_finalize_with_blocking_keeps_output_guard_after_cancellation() -> anyhow::Result<()> {
        let stdb = TestDB::durable()?;
        add_table(&stdb)?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());
        write_test_server_state(data.path(), "[commitlog]\nlog-format-version = 1\n")?;

        let backup_dir = tempfile::tempdir()?;
        let backup = stdb.runtime().unwrap().block_on(async {
            tokio::time::timeout(
                HOT_BACKUP_TEST_TIMEOUT,
                create_hot_backup_without_control_db(
                    &stdb,
                    stdb.path().unwrap(),
                    Some(&data_dir),
                    0,
                    backup_dir.path(),
                ),
            )
            .await
        })??;
        let output_dir = backup.manifest().output_dir.clone();

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let handle = stdb.runtime().unwrap().spawn(async move {
            backup
                .finalize_with_blocking(move |_| {
                    started_tx
                        .send(())
                        .map_err(|_| anyhow!("finalize start receiver dropped"))?;
                    finish_rx
                        .recv()
                        .context("waiting for test to release hot backup finalize")?;
                    Ok(())
                })
                .await
        });

        started_rx
            .recv_timeout(HOT_BACKUP_TEST_TIMEOUT)
            .context("hot backup finalize did not start")?;
        handle.abort();

        let err = match BackupDirGuard::acquire(&output_dir.join("nested")) {
            Ok(_) => panic!("overlapping backup directory guard was accepted after cancellation"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("overlaps"));

        finish_tx
            .send(())
            .map_err(|_| anyhow!("finalize blocking task finished before release signal"))?;
        let deadline = Instant::now() + HOT_BACKUP_TEST_TIMEOUT;
        loop {
            match BackupDirGuard::acquire(&output_dir.join("nested")) {
                Ok(guard) => {
                    drop(guard);
                    break;
                }
                Err(err) if err.to_string().contains("overlaps") && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(err) => return Err(err),
            }
        }
        assert!(output_dir.join("manifest.json").is_file());

        Ok(())
    }

    #[test]
    fn hot_backup_rejects_raw_copy_of_live_control_db() -> anyhow::Result<()> {
        let stdb = TestDB::durable()?;
        add_table(&stdb)?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());
        write_test_server_state(data.path(), "[commitlog]\nlog-format-version = 1\n")?;
        std::fs::create_dir_all(data.path().join("control-db"))?;
        std::fs::write(data.path().join("control-db/control"), b"control")?;

        let backup_dir = tempfile::tempdir()?;
        let result = stdb.runtime().unwrap().block_on(async {
            tokio::time::timeout(
                HOT_BACKUP_TEST_TIMEOUT,
                create_hot_backup(&stdb, stdb.path().unwrap(), Some(&data_dir), 0, backup_dir.path()),
            )
            .await
        })?;
        let err = result.unwrap_err();

        assert!(err.to_string().contains("control-db"));
        assert!(!backup_dir.path().join("manifest.json").exists());
        Ok(())
    }

    #[test]
    fn hot_backup_rejects_unsafe_output_dirs() -> anyhow::Result<()> {
        let stdb = TestDB::durable()?;
        let replica_dir = stdb.path().unwrap();

        let err = stdb
            .runtime()
            .unwrap()
            .block_on(create_hot_backup(
                &stdb,
                replica_dir,
                None,
                0,
                PathBuf::from("relative"),
            ))
            .unwrap_err();
        assert!(err.to_string().contains("absolute server path"));

        let err = stdb
            .runtime()
            .unwrap()
            .block_on(create_hot_backup(
                &stdb,
                replica_dir,
                None,
                0,
                replica_dir.0.join("backup"),
            ))
            .unwrap_err();
        assert!(err.to_string().contains("replica directory"));

        Ok(())
    }

    #[test]
    fn hot_backup_dir_guard_rejects_parent_child_overlap() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let parent = temp.path().join("backup");
        let child = parent.join("nested");

        let parent_guard = BackupDirGuard::acquire(&parent)?;
        let Err(err) = BackupDirGuard::acquire(&child) else {
            panic!("child backup directory overlapped active parent");
        };
        assert!(err.to_string().contains("overlaps"));
        drop(parent_guard);

        let child_guard = BackupDirGuard::acquire(&child)?;
        let Err(err) = BackupDirGuard::acquire(&parent) else {
            panic!("parent backup directory overlapped active child");
        };
        assert!(err.to_string().contains("overlaps"));
        drop(child_guard);

        Ok(())
    }

    #[test]
    fn hot_backup_dir_guard_clone_keeps_path_active() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let parent = temp.path().join("backup");
        let child = parent.join("nested");

        let parent_guard = BackupDirGuard::acquire(&parent)?;
        let blocking_guard = parent_guard.clone();
        drop(parent_guard);

        let Err(err) = BackupDirGuard::acquire(&child) else {
            panic!("child backup directory overlapped cloned active parent");
        };
        assert!(err.to_string().contains("overlaps"));

        drop(blocking_guard);
        let child_guard = BackupDirGuard::acquire(&child)?;
        drop(child_guard);

        Ok(())
    }

    #[test]
    fn hot_backup_dir_guard_rejects_existing_filesystem_claim() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let output_dir = temp.path().join("backup");
        std::fs::create_dir(&output_dir)?;
        let marker_path = BackupDirGuard::marker_path_for(&output_dir);
        std::fs::write(&marker_path, [])?;

        let Err(err) = BackupDirGuard::acquire(&output_dir) else {
            panic!("existing filesystem claim marker was accepted");
        };
        assert!(err.to_string().contains("already claimed"));
        assert!(marker_path.is_file());

        Ok(())
    }

    #[test]
    fn hot_backup_dir_guard_rejects_claimed_ancestor() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let parent = temp.path().join("parent");
        std::fs::create_dir(&parent)?;
        let marker_path = BackupDirGuard::marker_path_for(&parent);
        std::fs::write(&marker_path, [])?;

        let child = parent.join("child");
        let err = match BackupDirGuard::acquire(&child) {
            Ok(_) => panic!("backup directory inside a claimed ancestor was accepted"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("inside claimed backup directory"));
        assert!(!child.exists());
        Ok(())
    }

    #[test]
    fn hot_backup_dir_guard_marker_is_only_allowed_entry_and_cleans_up() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let output_dir = temp.path().join("backup");
        let guard = BackupDirGuard::acquire(&output_dir)?;
        let marker_path = guard.marker_path();

        assert!(marker_path.is_file());
        assert_eq!(std::fs::metadata(&marker_path)?.len(), 0);
        ensure_empty_dir(&output_dir, &guard)?;

        let other_path = output_dir.join("other");
        std::fs::write(&other_path, b"not empty")?;
        let err = ensure_empty_dir(&output_dir, &guard).unwrap_err();
        assert!(err.to_string().contains("must be empty"));
        std::fs::remove_file(other_path)?;

        let cloned_guard = guard.clone();
        drop(guard);
        assert!(marker_path.is_file());
        drop(cloned_guard);
        assert!(!marker_path.exists());
        assert!(output_dir.read_dir()?.next().is_none());

        Ok(())
    }
}
