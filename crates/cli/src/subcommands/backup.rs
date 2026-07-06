use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::common_args;
use crate::config::Config;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_arg};
use crate::util::{add_auth_header_opt, database_identity, get_auth_header, ResponseExt};
use anyhow::Context;
use clap::{Arg, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use spacetimedb_commitlog::{payload::Txdata, Commitlog};
use spacetimedb_fs_utils::{copy_dir_all, copy_file_sync, create_dir_all_sync, sync_dir};
use spacetimedb_lib::{Identity, ProductValue};
use spacetimedb_paths::{
    server::{CommitLogDir, ServerDataDir, SnapshotsPath},
    FromPathUnchecked, SpacetimePaths,
};
use spacetimedb_snapshot::SnapshotRepository;
use spacetimedb_table::page_pool::PagePool;

static RESTORE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn cli() -> Command {
    Command::new("backup")
        .about("Create server-side database backups")
        .subcommand_required(true)
        .subcommand(
            Command::new("create")
                .about("Create a hot backup of a running database")
                .arg(
                    Arg::new("database")
                        .long("database")
                        .required(false)
                        .help("The name or identity of the database to back up"),
                )
                .arg(
                    Arg::new("output_dir")
                        .long("output-dir")
                        .value_name("ROOT_RELATIVE_OUTPUT_DIR")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Directory relative to the server's configured hot-backup root; it must be empty"),
                )
                .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
                .arg(common_args::anonymous())
                .arg(
                    Arg::new("no_config")
                        .long("no-config")
                        .action(clap::ArgAction::SetTrue)
                        .help("Ignore spacetime.json configuration"),
                ),
        )
        .subcommand(
            Command::new("restore")
                .about("Restore a hot backup into an offline server data directory")
                .arg(
                    Arg::new("input_dir")
                        .long("input-dir")
                        .value_name("BACKUP_DIR")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Directory containing manifest.json, snapshots/, and clog/"),
                )
                .arg(
                    Arg::new("data_dir")
                        .long("data-dir")
                        .value_name("SERVER_DATA_DIR")
                        .value_parser(clap::value_parser!(ServerDataDir))
                        .help("Offline server data directory whose matching replica will be restored; defaults to the CLI root data-dir"),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .action(clap::ArgAction::SetTrue)
                        .help("Overwrite the existing target replica directory"),
                ),
        )
}

pub async fn exec(config: Config, paths: &SpacetimePaths, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    match cmd {
        "create" => exec_create(config, subcommand_args).await,
        "restore" => exec_restore(paths, subcommand_args),
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {unknown}")),
    }
}

#[derive(Serialize)]
struct BackupRequest {
    server_output_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct BackupManifest {
    version: u32,
    database_identity: String,
    replica_id: u64,
    output_dir: PathBuf,
    snapshot_offset: u64,
    durable_offset: u64,
    snapshot_ms: u64,
    copy_ms: u64,
    total_ms: u64,
    bytes: u64,
}

async fn exec_create(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server_from_cli = args.get_one::<String>("server").map(|s| s.as_ref());
    let no_config = args.get_flag("no_config");
    let anon_identity = args.get_flag("anon_identity");
    let database_arg = args.get_one::<String>("database").map(|s| s.as_str());
    let config_targets = load_config_db_targets(no_config)?;
    let resolved = resolve_database_arg(
        database_arg,
        config_targets.as_deref(),
        "spacetime backup create --database <database> --output-dir <root-relative-output-dir> [--no-config]",
    )?;
    let server = server_from_cli.or(resolved.server.as_deref());

    let identity = database_identity(&config, &resolved.database, server).await?;
    let host_url = config.get_host_url(server)?;
    let auth_header = get_auth_header(&mut config, anon_identity, server, true).await?;
    let server_output_dir = args.get_one::<PathBuf>("output_dir").unwrap().clone();

    let mut builder = reqwest::Client::new()
        .post(format!("{host_url}/v1/database/{identity}/backup"))
        .json(&BackupRequest { server_output_dir });
    builder = add_auth_header_opt(builder, &auth_header);
    let manifest: BackupManifest = builder.send().await?.json_or_error().await?;

    println!("Backup written on server to {}", manifest.output_dir.display());
    println!("snapshot_offset: {}", manifest.snapshot_offset);
    println!("durable_offset: {}", manifest.durable_offset);
    println!("bytes: {}", manifest.bytes);
    println!("snapshot_ms: {}", manifest.snapshot_ms);
    println!("copy_ms: {}", manifest.copy_ms);
    println!("total_ms: {}", manifest.total_ms);
    Ok(())
}

fn exec_restore(paths: &SpacetimePaths, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let input_dir = args.get_one::<PathBuf>("input_dir").unwrap();
    let data_dir = args.get_one::<ServerDataDir>("data_dir").unwrap_or(&paths.data_dir);
    let force = args.get_flag("force");

    let manifest = restore_backup(input_dir, data_dir, force)?;

    println!(
        "Restored database {} replica {} into {}",
        manifest.database_identity,
        manifest.replica_id,
        data_dir.replica(manifest.replica_id).display()
    );
    println!("snapshot_offset: {}", manifest.snapshot_offset);
    println!("durable_offset: {}", manifest.durable_offset);
    Ok(())
}

fn restore_backup(input_dir: &Path, data_dir: &ServerDataDir, force: bool) -> anyhow::Result<BackupManifest> {
    let manifest = read_backup_manifest(input_dir)?;
    validate_backup(input_dir, &manifest)?;
    create_dir_all_sync(&data_dir.0)?;

    // The data-dir lock catches the common footgun; per-replica online restore needs a real restore service.
    let _pid_file = data_dir
        .pid_file()
        .context("target data-dir must be offline before restore")?;
    restore_backup_inner(input_dir, data_dir, force, manifest)
}

fn restore_backup_inner(
    input_dir: &Path,
    data_dir: &ServerDataDir,
    force: bool,
    manifest: BackupManifest,
) -> anyhow::Result<BackupManifest> {
    let replica_dir = data_dir.replica(manifest.replica_id);
    let restore_temp_id = RESTORE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = replica_dir.0.with_file_name(format!(
        "{}.restore_tmp_{}_{}",
        manifest.replica_id,
        std::process::id(),
        restore_temp_id
    ));
    let old_tmp_dir = replica_dir.0.with_file_name(format!(
        "{}.restore_old_{}_{}",
        manifest.replica_id,
        std::process::id(),
        restore_temp_id
    ));

    anyhow::ensure!(
        force || !replica_dir.0.exists(),
        "target replica directory already exists: {}; pass --force to overwrite it",
        replica_dir.display()
    );
    anyhow::ensure!(
        !tmp_dir.exists(),
        "temporary restore directory already exists: {}",
        tmp_dir.display()
    );
    if force {
        anyhow::ensure!(
            !old_tmp_dir.exists(),
            "temporary old replica directory already exists: {}",
            old_tmp_dir.display()
        );
    }

    if let Some(parent) = tmp_dir.parent() {
        create_dir_all_sync(parent)?;
    }

    let staged_server_state = stage_missing_server_state(input_dir, data_dir, restore_temp_id)?;
    anyhow::ensure!(
        !(force && replica_dir.0.exists() && !staged_server_state.is_empty()),
        "cannot restore over existing replica {} while target data-dir is missing server state; restore into an empty data-dir or complete the target server state first",
        replica_dir.display()
    );

    let mut old_tmp_moved = false;
    let mut restore_old_tmp_on_error = false;
    let mut committed_server_state = None;
    let res = (|| -> anyhow::Result<()> {
        create_dir_all_sync(&tmp_dir)?;
        copy_dir_all(input_dir.join("snapshots"), tmp_dir.join("snapshots"))?;
        copy_dir_all(input_dir.join("clog"), tmp_dir.join("clog"))?;
        create_dir_all_sync(&tmp_dir.join("module_logs"))?;
        sync_parent_dir(&tmp_dir)?;

        committed_server_state = Some(staged_server_state.commit()?);

        if force && replica_dir.0.exists() {
            std::fs::rename(&replica_dir.0, &old_tmp_dir)
                .with_context(|| format!("moving existing target replica directory {}", replica_dir.display()))?;
            sync_parent_dir(&replica_dir.0)?;
            old_tmp_moved = true;
            restore_old_tmp_on_error = true;
        }
        std::fs::rename(&tmp_dir, &replica_dir.0)
            .with_context(|| format!("moving restored replica into {}", replica_dir.display()))?;
        sync_parent_dir(&replica_dir.0)?;
        restore_old_tmp_on_error = false;

        if let Some(committed_server_state) = &mut committed_server_state {
            committed_server_state.keep();
        }

        if old_tmp_moved {
            if let Err(err) = std::fs::remove_dir_all(&old_tmp_dir) {
                tracing::warn!(
                    "restore completed, but failed to remove old replica directory {}: {err}",
                    old_tmp_dir.display()
                );
            } else {
                let _ = sync_parent_dir(&old_tmp_dir);
            }
            old_tmp_moved = false;
        }
        Ok(())
    })();

    if res.is_err() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let _ = sync_parent_dir(&tmp_dir);
        if restore_old_tmp_on_error && old_tmp_moved && old_tmp_dir.exists() && !replica_dir.0.exists() {
            let _ = std::fs::rename(&old_tmp_dir, &replica_dir.0);
            let _ = sync_parent_dir(&replica_dir.0);
        }
    }
    res?;

    Ok(manifest)
}

fn sync_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        sync_dir(parent).with_context(|| format!("syncing parent directory {}", parent.display()))?;
    }
    Ok(())
}

fn read_backup_manifest(input_dir: &Path) -> anyhow::Result<BackupManifest> {
    let manifest_path = input_dir.join("manifest.json");
    let bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("reading backup manifest {}", manifest_path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing backup manifest {}", manifest_path.display()))
}

fn validate_backup(input_dir: &Path, manifest: &BackupManifest) -> anyhow::Result<()> {
    anyhow::ensure!(
        manifest.version == 1,
        "unsupported backup manifest version {}",
        manifest.version
    );
    anyhow::ensure!(
        manifest.durable_offset == manifest.snapshot_offset,
        "backup durable_offset {} does not match snapshot_offset {}",
        manifest.durable_offset,
        manifest.snapshot_offset
    );
    let database_identity: Identity = manifest
        .database_identity
        .parse()
        .with_context(|| format!("parsing backup database identity {}", manifest.database_identity))?;

    let snapshots = SnapshotsPath::from_path_unchecked(input_dir.join("snapshots"));
    let snapshot_dir = snapshots.snapshot_dir(manifest.snapshot_offset);
    anyhow::ensure!(
        snapshot_dir.0.is_dir(),
        "backup snapshot directory is missing: {}",
        snapshot_dir.display()
    );
    let snapshot_file = snapshot_dir.snapshot_file(manifest.snapshot_offset);
    anyhow::ensure!(
        snapshot_file.0.is_file(),
        "backup snapshot file is missing: {}",
        snapshot_file.display()
    );
    let snapshot_repo =
        SnapshotRepository::open(snapshots, database_identity, manifest.replica_id).with_context(|| {
            format!(
                "opening backup snapshots directory {}",
                input_dir.join("snapshots").display()
            )
        })?;
    let snapshot = snapshot_repo
        .read_snapshot(manifest.snapshot_offset, &PagePool::new(None))
        .with_context(|| format!("reading backup snapshot {}", snapshot_file.display()))?;
    anyhow::ensure!(
        snapshot.database_identity == database_identity,
        "backup snapshot database identity {} does not match manifest {}",
        snapshot.database_identity,
        database_identity
    );
    anyhow::ensure!(
        snapshot.replica_id == manifest.replica_id,
        "backup snapshot replica_id {} does not match manifest {}",
        snapshot.replica_id,
        manifest.replica_id
    );
    anyhow::ensure!(
        snapshot.tx_offset == manifest.snapshot_offset,
        "backup snapshot tx_offset {} does not match manifest {}",
        snapshot.tx_offset,
        manifest.snapshot_offset
    );

    let clog_dir = CommitLogDir::from_path_unchecked(input_dir.join("clog"));
    anyhow::ensure!(
        clog_dir.0.is_dir(),
        "backup clog directory is missing: {}",
        clog_dir.display()
    );
    let initial_segment = clog_dir.segment(0);
    anyhow::ensure!(
        initial_segment.0.is_file(),
        "backup commitlog segment is missing: {}",
        initial_segment.display()
    );
    let mut segment_count = 0u64;
    for entry in std::fs::read_dir(&clog_dir.0).with_context(|| format!("reading {}", clog_dir.display()))? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.ends_with(".stdb.log") {
            anyhow::ensure!(
                is_commitlog_segment_name(&file_name) && entry.file_type()?.is_file(),
                "invalid backup commitlog segment: {}",
                entry.path().display()
            );
            segment_count += 1;
        }
    }
    anyhow::ensure!(
        segment_count > 0,
        "backup clog directory contains no segment files: {}",
        clog_dir.display()
    );
    let clog = Commitlog::<Txdata<ProductValue>>::open(clog_dir, Default::default(), None)
        .with_context(|| format!("opening backup commitlog {}", input_dir.join("clog").display()))?;
    anyhow::ensure!(
        clog.max_committed_offset() == Some(manifest.snapshot_offset),
        "backup commitlog max committed offset {:?} does not match manifest snapshot_offset {}",
        clog.max_committed_offset(),
        manifest.snapshot_offset
    );
    for commit in clog.commits() {
        commit.with_context(|| format!("reading backup commitlog {}", input_dir.join("clog").display()))?;
    }
    Ok(())
}

fn is_commitlog_segment_name(file_name: &str) -> bool {
    let Some(offset) = file_name.strip_suffix(".stdb.log") else {
        return false;
    };
    offset.len() == 20 && offset.bytes().all(|byte| byte.is_ascii_digit())
}

#[derive(Debug)]
struct StagedServerStateEntry {
    final_path: PathBuf,
    staged_path: PathBuf,
}

#[derive(Debug)]
struct StagedServerState {
    entries: Vec<StagedServerStateEntry>,
}

impl StagedServerState {
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn cleanup(&self) {
        for entry in &self.entries {
            if entry.staged_path.is_dir() {
                let _ = std::fs::remove_dir_all(&entry.staged_path);
            } else {
                let _ = std::fs::remove_file(&entry.staged_path);
            }
            let _ = sync_parent_dir(&entry.staged_path);
        }
    }

    fn commit(self) -> anyhow::Result<CommittedServerState> {
        let mut committed = CommittedServerState::default();
        let res = (|| -> anyhow::Result<()> {
            for entry in &self.entries {
                anyhow::ensure!(
                    !entry.final_path.exists(),
                    "target data-dir server state appeared during restore: {}",
                    entry.final_path.display()
                );
                std::fs::rename(&entry.staged_path, &entry.final_path).with_context(|| {
                    format!(
                        "moving staged server state {} to {}",
                        entry.staged_path.display(),
                        entry.final_path.display()
                    )
                })?;
                committed.paths.push(entry.final_path.clone());
                sync_parent_dir(&entry.final_path)?;
            }
            Ok(())
        })();
        if res.is_err() {
            self.cleanup();
            committed.rollback();
        }
        res?;
        Ok(committed)
    }
}

impl Drop for StagedServerState {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[derive(Debug, Default)]
struct CommittedServerState {
    paths: Vec<PathBuf>,
    keep: bool,
}

impl CommittedServerState {
    fn keep(&mut self) {
        self.keep = true;
    }

    fn rollback(&self) {
        for path in self.paths.iter().rev() {
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(path);
            } else {
                let _ = std::fs::remove_file(path);
            }
            let _ = sync_parent_dir(path);
        }
    }
}

impl Drop for CommittedServerState {
    fn drop(&mut self) {
        if !self.keep {
            self.rollback();
        }
    }
}

fn stage_missing_server_state(
    input_dir: &Path,
    data_dir: &ServerDataDir,
    restore_temp_id: u64,
) -> anyhow::Result<StagedServerState> {
    let server_dir = input_dir.join("server");
    let needs_required_dirs = ["control-db", "program-bytes"]
        .into_iter()
        .any(|required_dir| !data_dir.0.join(required_dir).exists());
    if !needs_required_dirs && !server_dir.exists() {
        return Ok(StagedServerState { entries: Vec::new() });
    }

    let mut entries = Vec::new();
    for required_dir in ["control-db", "program-bytes"] {
        let dst = data_dir.0.join(required_dir);
        if dst.exists() {
            continue;
        }
        let src = server_dir.join(required_dir);
        anyhow::ensure!(
            src.is_dir(),
            "target data-dir is missing {}; backup is missing server state {}",
            dst.display(),
            src.display()
        );
        let staged = staged_server_state_path(&dst, restore_temp_id);
        anyhow::ensure!(
            !staged.exists(),
            "temporary server state path already exists: {}",
            staged.display()
        );
        entries.push((src, dst, staged, true));
    }
    for file in ["config.toml", "metadata.toml"] {
        let dst = data_dir.0.join(file);
        if dst.exists() {
            continue;
        }
        let src = server_dir.join(file);
        if !needs_required_dirs && !src.exists() {
            continue;
        }
        anyhow::ensure!(
            src.is_file(),
            "target data-dir is missing {}; backup is missing server state {}",
            dst.display(),
            src.display()
        );
        let staged = staged_server_state_path(&dst, restore_temp_id);
        anyhow::ensure!(
            !staged.exists(),
            "temporary server state path already exists: {}",
            staged.display()
        );
        entries.push((src, dst, staged, false));
    }

    let mut staged = StagedServerState { entries: Vec::new() };
    let res = (|| -> anyhow::Result<()> {
        for (src, final_path, staged_path, is_dir) in entries {
            copy_staged_server_state_entry(&mut staged, &src, final_path, staged_path, is_dir)?;
        }
        Ok(())
    })();
    if res.is_err() {
        staged.cleanup();
    }
    res?;
    Ok(staged)
}

fn copy_staged_server_state_entry(
    staged: &mut StagedServerState,
    src: &Path,
    final_path: PathBuf,
    staged_path: PathBuf,
    is_dir: bool,
) -> anyhow::Result<()> {
    staged.entries.push(StagedServerStateEntry {
        final_path,
        staged_path,
    });
    let entry = staged.entries.last().expect("staged entry was just pushed");
    let res = if is_dir {
        copy_dir_all(src, &entry.staged_path)
            .with_context(|| format!("copying {} to {}", src.display(), entry.staged_path.display()))
    } else {
        copy_file_sync(src, &entry.staged_path)
            .with_context(|| format!("copying {} to {}", src.display(), entry.staged_path.display()))
            .map(|_| ())
    };
    if res.is_err() {
        staged.cleanup();
    }
    res
}

fn staged_server_state_path(path: &Path, restore_temp_id: u64) -> PathBuf {
    let suffix = format!("restore_tmp_{}_{}", std::process::id(), restore_temp_id);
    if let Some(file_name) = path.file_name() {
        path.with_file_name(format!("{}.{}", file_name.to_string_lossy(), suffix))
    } else {
        path.with_extension(suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_paths::FromPathUnchecked;
    use spacetimedb_table::{blob_store::HashMapBlobStore, table::Table};

    #[test]
    fn backup_restore_copies_replica_into_existing_data_dir() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;

        let data = tempfile::tempdir()?;
        make_target_data_dir(data.path())?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let manifest = restore_backup(backup.path(), &data_dir, false)?;

        assert_eq!(manifest.replica_id, 7);
        assert!(data
            .path()
            .join("replicas/7/snapshots/00000000000000000042.snapshot_dir/00000000000000000042.snapshot_bsatn")
            .is_file());
        assert!(data
            .path()
            .join("replicas/7/clog/00000000000000000000.stdb.log")
            .is_file());
        assert!(data.path().join("replicas/7/module_logs").is_dir());
        assert!(!data.path().join("spacetime.pid").exists());
        Ok(())
    }

    #[test]
    fn backup_restore_requires_force_for_existing_replica() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;

        let data = tempfile::tempdir()?;
        make_target_data_dir(data.path())?;
        std::fs::create_dir_all(data.path().join("replicas/7"))?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();
        assert!(err.to_string().contains("--force"));
        assert!(data.path().join("replicas/7").is_dir());
        Ok(())
    }

    #[test]
    fn backup_restore_requires_force_before_copying_missing_server_state() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        make_backup_server_state(backup.path())?;

        let data = tempfile::tempdir()?;
        std::fs::create_dir_all(data.path().join("replicas/7"))?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();
        assert!(err.to_string().contains("--force"));
        assert!(!data.path().join("control-db").exists());
        assert!(!data.path().join("program-bytes").exists());
        assert!(!data.path().join("config.toml").exists());
        assert!(!data.path().join("metadata.toml").exists());
        Ok(())
    }

    #[test]
    fn backup_restore_force_replaces_existing_replica_without_leaving_old_tmp() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;

        let data = tempfile::tempdir()?;
        make_target_data_dir(data.path())?;
        let replica_dir = data.path().join("replicas/7");
        std::fs::create_dir_all(&replica_dir)?;
        std::fs::write(replica_dir.join("old-marker"), b"old")?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        restore_backup(backup.path(), &data_dir, true)?;

        assert!(!replica_dir.join("old-marker").exists());
        assert!(replica_dir
            .join("snapshots/00000000000000000042.snapshot_dir/00000000000000000042.snapshot_bsatn")
            .is_file());
        assert!(replica_dir.join("clog/00000000000000000000.stdb.log").is_file());
        assert!(replica_dir.join("module_logs").is_dir());

        for entry in std::fs::read_dir(data.path().join("replicas"))? {
            let entry = entry?;
            assert!(!entry.file_name().to_string_lossy().contains(".restore_old_"));
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn backup_restore_force_commits_when_old_replica_cleanup_fails() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;

        let data = tempfile::tempdir()?;
        make_target_data_dir(data.path())?;
        let replica_dir = data.path().join("replicas/7");
        std::fs::create_dir_all(&replica_dir)?;
        let old_marker = replica_dir.join("old-marker");
        std::fs::write(&old_marker, b"old")?;
        let mut permissions = std::fs::metadata(&old_marker)?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&old_marker, permissions)?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        restore_backup(backup.path(), &data_dir, true)?;
        restore_backup(backup.path(), &data_dir, true)?;

        assert!(replica_dir
            .join("snapshots/00000000000000000042.snapshot_dir/00000000000000000042.snapshot_bsatn")
            .is_file());

        for entry in std::fs::read_dir(data.path().join("replicas"))? {
            let entry = entry?;
            if !entry.file_name().to_string_lossy().contains(".restore_old_") {
                continue;
            }
            clear_readonly_recursively(&entry.path())?;
            std::fs::remove_dir_all(entry.path())?;
        }
        Ok(())
    }

    #[test]
    fn backup_restore_copies_server_state_into_empty_data_dir() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        make_backup_server_state(backup.path())?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        restore_backup(backup.path(), &data_dir, false)?;

        assert!(data.path().join("control-db/control").is_file());
        assert!(data.path().join("program-bytes/program").is_file());
        assert_eq!(std::fs::read_to_string(data.path().join("config.toml"))?, "config");
        assert_eq!(std::fs::read_to_string(data.path().join("metadata.toml"))?, "metadata");
        Ok(())
    }

    #[test]
    fn backup_restore_errors_when_empty_data_dir_lacks_server_state() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();
        assert!(err.to_string().contains("backup is missing server state"));
        assert!(!data.path().join("replicas/7").exists());
        Ok(())
    }

    #[test]
    fn backup_restore_rejects_missing_snapshot_file_before_copying_server_state() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        make_backup_server_state(backup.path())?;
        std::fs::remove_file(
            backup
                .path()
                .join("snapshots/00000000000000000042.snapshot_dir/00000000000000000042.snapshot_bsatn"),
        )?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();

        assert!(err.to_string().contains("backup snapshot file is missing"));
        assert!(!data.path().join("control-db").exists());
        assert!(!data.path().join("replicas/7").exists());
        Ok(())
    }

    #[test]
    fn backup_restore_rejects_corrupt_snapshot_before_copying_server_state() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        make_backup_server_state(backup.path())?;
        std::fs::write(
            backup
                .path()
                .join("snapshots/00000000000000000042.snapshot_dir/00000000000000000042.snapshot_bsatn"),
            b"not a snapshot",
        )?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();

        assert!(err.to_string().contains("reading backup snapshot"));
        assert!(!data.path().join("control-db").exists());
        assert!(!data.path().join("replicas/7").exists());
        Ok(())
    }

    #[test]
    fn backup_restore_rejects_clog_without_segment_file_before_copying_server_state() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        make_backup_server_state(backup.path())?;
        std::fs::remove_file(backup.path().join("clog/00000000000000000000.stdb.log"))?;
        std::fs::write(backup.path().join("clog/not-a-segment"), b"log")?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();

        assert!(err.to_string().contains("backup commitlog segment is missing"));
        assert!(!data.path().join("control-db").exists());
        assert!(!data.path().join("replicas/7").exists());
        Ok(())
    }

    #[test]
    fn backup_restore_rejects_invalid_commitlog_segment_name() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        std::fs::write(backup.path().join("clog/bad.stdb.log"), b"log")?;

        let data = tempfile::tempdir()?;
        make_target_data_dir(data.path())?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();

        assert!(err.to_string().contains("invalid backup commitlog segment"));
        assert!(!data.path().join("replicas/7").exists());
        Ok(())
    }

    #[test]
    fn backup_restore_does_not_leave_partial_server_state_when_server_state_is_incomplete() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        std::fs::create_dir_all(backup.path().join("server/control-db"))?;
        std::fs::write(backup.path().join("server/control-db/control"), b"control")?;

        let data = tempfile::tempdir()?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, false).unwrap_err();

        assert!(err.to_string().contains("backup is missing server state"));
        assert!(!data.path().join("control-db").exists());
        assert!(!data.path().join("program-bytes").exists());
        assert!(!data.path().join("config.toml").exists());
        assert!(!data.path().join("metadata.toml").exists());
        assert!(!data.path().join("replicas/7").exists());
        for entry in std::fs::read_dir(data.path())? {
            let entry = entry?;
            assert!(!entry.file_name().to_string_lossy().contains(".restore_tmp_"));
        }
        Ok(())
    }

    #[test]
    fn backup_restore_staged_server_state_cleanup_removes_current_entry_after_copy_error() -> anyhow::Result<()> {
        let data = tempfile::tempdir()?;
        let final_path = data.path().join("control-db");
        let staged_path = staged_server_state_path(&final_path, 99);
        let mut staged = StagedServerState { entries: Vec::new() };

        let err = copy_staged_server_state_entry(
            &mut staged,
            &data.path().join("missing-control-db"),
            final_path,
            staged_path.clone(),
            true,
        )
        .unwrap_err();

        assert!(err.to_string().contains("copying"));
        assert_eq!(staged.entries.len(), 1);
        assert!(!staged_path.exists());
        Ok(())
    }

    #[test]
    fn backup_restore_rejects_force_existing_replica_when_server_state_is_missing() -> anyhow::Result<()> {
        let backup = tempfile::tempdir()?;
        make_backup_dir(backup.path(), 7, 42)?;
        make_backup_server_state(backup.path())?;

        let data = tempfile::tempdir()?;
        let replica_dir = data.path().join("replicas/7");
        std::fs::create_dir_all(&replica_dir)?;
        let data_dir = ServerDataDir::from_path_unchecked(data.path());

        let err = restore_backup(backup.path(), &data_dir, true).unwrap_err();

        assert!(err.to_string().contains("cannot restore over existing replica"));
        assert!(replica_dir.is_dir());
        assert!(!data.path().join("control-db").exists());
        assert!(!data.path().join("program-bytes").exists());
        Ok(())
    }

    fn make_target_data_dir(path: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(path.join("control-db"))?;
        std::fs::create_dir_all(path.join("program-bytes"))?;
        Ok(())
    }

    fn make_backup_server_state(path: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(path.join("server/control-db"))?;
        std::fs::write(path.join("server/control-db/control"), b"control")?;
        std::fs::create_dir_all(path.join("server/program-bytes"))?;
        std::fs::write(path.join("server/program-bytes/program"), b"program")?;
        std::fs::write(path.join("server/config.toml"), b"config")?;
        std::fs::write(path.join("server/metadata.toml"), b"metadata")?;
        Ok(())
    }

    #[cfg(windows)]
    #[allow(clippy::permissions_set_readonly_false)]
    fn clear_readonly_recursively(path: &Path) -> anyhow::Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions)?;
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                clear_readonly_recursively(&entry?.path())?;
            }
        }
        Ok(())
    }

    fn make_backup_dir(path: &Path, replica_id: u64, offset: u64) -> anyhow::Result<()> {
        let database_identity: Identity = "c200000000000000000000000000000000000000000000000000000000000000".parse()?;

        let snapshots_path = SnapshotsPath::from_path_unchecked(path.join("snapshots"));
        std::fs::create_dir_all(&snapshots_path.0)?;
        let snapshot_repo = SnapshotRepository::open(snapshots_path, database_identity, replica_id)?;
        let blobs = HashMapBlobStore::default();
        snapshot_repo
            .create_snapshot(std::iter::empty::<&mut Table>(), &blobs, offset)?
            .sync_all()?;

        let clog_dir = path.join("clog");
        std::fs::create_dir_all(&clog_dir)?;
        let clog = Commitlog::<Txdata<ProductValue>>::open(
            CommitLogDir::from_path_unchecked(&clog_dir),
            Default::default(),
            None,
        )?;
        for tx_offset in 0..=offset {
            clog.commit([(
                tx_offset,
                Txdata {
                    inputs: None,
                    outputs: None,
                    mutations: None,
                },
            )])?;
        }
        clog.flush_and_sync()?;

        let manifest = serde_json::json!({
            "version": 1,
            "database_identity": database_identity,
            "replica_id": replica_id,
            "snapshot_offset": offset,
            "durable_offset": offset,
            "output_dir": path,
            "snapshot_ms": 1,
            "copy_ms": 2,
            "total_ms": 3,
            "bytes": 4
        });
        std::fs::write(path.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
        Ok(())
    }
}
