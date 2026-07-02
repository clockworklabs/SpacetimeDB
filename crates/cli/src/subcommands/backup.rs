use std::path::{Path, PathBuf};

use crate::common_args;
use crate::config::Config;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_arg};
use crate::util::{add_auth_header_opt, database_identity, get_auth_header, ResponseExt};
use anyhow::Context;
use clap::{Arg, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use spacetimedb_paths::{server::ServerDataDir, SpacetimePaths};

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
                        .value_name("SERVER_OUTPUT_DIR")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Directory on the server where the backup will be written; it must be empty"),
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
        "spacetime backup create --database <database> --output-dir <server-output-dir> [--no-config]",
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
    std::fs::create_dir_all(data_dir)?;

    // ponytail: data-dir lock catches the common footgun; per-replica online restore needs a real restore service.
    let _pid_file = data_dir
        .pid_file()
        .context("target data-dir must be offline before restore")?;
    copy_missing_server_state(input_dir, data_dir)?;

    let replica_dir = data_dir.replica(manifest.replica_id);
    let tmp_dir = replica_dir
        .0
        .with_file_name(format!("{}.restore_tmp_{}", manifest.replica_id, std::process::id()));

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

    if let Some(parent) = tmp_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let res = (|| -> anyhow::Result<()> {
        std::fs::create_dir_all(&tmp_dir)?;
        copy_dir_all(input_dir.join("snapshots"), tmp_dir.join("snapshots"))?;
        copy_dir_all(input_dir.join("clog"), tmp_dir.join("clog"))?;
        std::fs::create_dir_all(tmp_dir.join("module_logs"))?;

        if replica_dir.0.exists() {
            std::fs::remove_dir_all(&replica_dir.0)
                .with_context(|| format!("removing existing target replica directory {}", replica_dir.display()))?;
        }
        std::fs::rename(&tmp_dir, &replica_dir.0)
            .with_context(|| format!("moving restored replica into {}", replica_dir.display()))?;
        Ok(())
    })();

    if res.is_err() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    res?;

    Ok(manifest)
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

    let snapshot_dir = input_dir
        .join("snapshots")
        .join(format!("{:020}.snapshot_dir", manifest.snapshot_offset));
    anyhow::ensure!(
        snapshot_dir.is_dir(),
        "backup snapshot directory is missing: {}",
        snapshot_dir.display()
    );

    let clog_dir = input_dir.join("clog");
    anyhow::ensure!(
        clog_dir.is_dir(),
        "backup clog directory is missing: {}",
        clog_dir.display()
    );
    anyhow::ensure!(
        clog_dir.read_dir()?.next().is_some(),
        "backup clog directory is empty: {}",
        clog_dir.display()
    );
    Ok(())
}

fn copy_missing_server_state(input_dir: &Path, data_dir: &ServerDataDir) -> anyhow::Result<()> {
    let server_dir = input_dir.join("server");
    let needs_required_dirs = ["control-db", "program-bytes"]
        .into_iter()
        .any(|required_dir| !data_dir.0.join(required_dir).exists());
    if !needs_required_dirs && !server_dir.exists() {
        return Ok(());
    }

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
        copy_dir_all(src, dst)?;
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
        std::fs::copy(&src, &dst).with_context(|| format!("copying {} to {}", src.display(), dst.display()))?;
    }
    Ok(())
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst)?;
        } else {
            std::fs::copy(entry.path(), &dst)
                .with_context(|| format!("copying {} to {}", entry.path().display(), dst.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_paths::FromPathUnchecked;

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
            .join("replicas/7/snapshots/00000000000000000042.snapshot_dir/snapshot")
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

    fn make_backup_dir(path: &Path, replica_id: u64, offset: u64) -> anyhow::Result<()> {
        let snapshot_dir = path.join("snapshots").join(format!("{offset:020}.snapshot_dir"));
        std::fs::create_dir_all(&snapshot_dir)?;
        std::fs::write(snapshot_dir.join("snapshot"), b"snapshot")?;

        let clog_dir = path.join("clog");
        std::fs::create_dir_all(&clog_dir)?;
        std::fs::write(clog_dir.join("00000000000000000000.stdb.log"), b"log")?;

        let manifest = serde_json::json!({
            "version": 1,
            "database_identity": "c200000000000000000000000000000000000000000000000000000000000000",
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
