//! Container build and operation commands. Local builds do not read saved
//! server credentials; network commands use the explicitly selected server.
#[path = "container/exec.rs"]
mod execute;
mod logs;
mod operations;
mod url;

use crate::{
    container::{config::ContainerConfig, prepare_container, process::LocalRunner, BuildSecret, BuildTools},
    spacetime_config::{find_and_load_with_env_from, SpacetimeConfig},
};
use anyhow::{ensure, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub fn cli() -> Command {
    let command = Command::new("container")
        .about("Build and manage a database's container")
        .subcommand_required(true)
        .subcommand(url::cli())
        .subcommand(execute::cli())
        .subcommand(logs::cli())
        .subcommand(
            Command::new("build")
                .about("Prepare verified OCI artifacts locally without publishing")
                .arg(Arg::new("database").help("Database target in local spacetime.json; no server lookup"))
                .arg(
                    Arg::new("project_path")
                        .long("project-path")
                        .default_value(".")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Directory in which to find spacetime.json"),
                )
                .arg(
                    Arg::new("out_dir")
                        .long("out-dir")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("New directory for verified OCI artifacts and prepared.json"),
                )
                .arg(
                    Arg::new("platform")
                        .long("platform")
                        .required(true)
                        .value_parser(["linux/amd64", "linux/arm64"])
                        .help("Target Linux platform, independent of this computer's architecture"),
                )
                .arg(Arg::new("env").long("env").help("Local configuration overlay name"))
                .arg(
                    Arg::new("buildkit_host")
                        .long("buildkit-host")
                        .help("Explicit local BuildKit Unix socket, required for source builds"),
                )
                .arg(
                    Arg::new("buildctl")
                        .long("buildctl")
                        .default_value("buildctl")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("BuildKit client executable"),
                )
                .arg(
                    Arg::new("railpack")
                        .long("railpack")
                        .default_value("railpack")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Pinned Railpack executable for explicitly selected Railpack builds"),
                )
                .arg(
                    Arg::new("skopeo")
                        .long("skopeo")
                        .default_value("skopeo")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Skopeo executable for prebuilt registry images"),
                )
                .arg(
                    Arg::new("registry_auth_file")
                        .long("registry-auth-file")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("Explicit registry auth JSON; omitted means anonymous, never saved Docker credentials"),
                )
                .arg(
                    Arg::new("build_secret")
                        .long("build-secret")
                        .action(ArgAction::Append)
                        .value_name("NAME=FILE")
                        .help("Explicit build secret file; separate from runtime env_keys"),
                ),
        );
    operations::commands(command)
}

pub(crate) fn select(config: &SpacetimeConfig, database: Option<&str>) -> Result<ContainerConfig> {
    let targets = config.collect_all_targets_with_inheritance();
    let selected = if let Some(database) = database {
        let mut matches = targets
            .iter()
            .filter(|target| target.fields.get("database").and_then(|v| v.as_str()) == Some(database));
        let target = matches
            .next()
            .context("database target is not in local spacetime.json")?;
        ensure!(
            matches.next().is_none(),
            "database target is ambiguous in local spacetime.json"
        );
        target
    } else {
        let mut matches = targets.iter().filter(|target| target.container.is_some());
        let target = matches
            .next()
            .context("no container declaration in local spacetime.json")?;
        ensure!(
            matches.next().is_none(),
            "several container targets exist; select a DATABASE from local spacetime.json"
        );
        target
    };
    selected
        .container
        .clone()
        .context("selected database has no container declaration; containers are not inherited")
}

pub async fn exec(mut config: crate::Config, args: &ArgMatches) -> Result<()> {
    match args.subcommand().context("missing container command")? {
        ("build", args) => exec_build(args).await,
        ("exec", args) => execute::exec(&mut config, args).await,
        ("url", args) => url::exec(&config, args).await,
        ("logs", args) => logs::exec(&mut config, args).await,
        (name @ ("status" | "start" | "stop" | "restart"), args) => operations::exec(&mut config, name, args).await,
        _ => anyhow::bail!("unsupported container command"),
    }
}

pub async fn exec_build(args: &ArgMatches) -> Result<()> {
    let project = args.get_one::<PathBuf>("project_path").unwrap().canonicalize()?;
    let loaded = find_and_load_with_env_from(args.get_one::<String>("env").map(String::as_str), project)?
        .context("spacetime.json not found")?;
    let declaration = select(&loaded.config, args.get_one::<String>("database").map(String::as_str))?;
    let output = std::env::current_dir()?.join(args.get_one::<PathBuf>("out_dir").unwrap());
    ensure!(!output.exists(), "output already exists: {}", output.display());
    let parent = output.parent().unwrap_or(Path::new("."));
    ensure!(parent.is_dir(), "output parent directory does not exist");
    let mut tools = BuildTools {
        buildctl: args.get_one::<PathBuf>("buildctl").unwrap().clone(),
        railpack: args.get_one::<PathBuf>("railpack").unwrap().clone(),
        skopeo: args.get_one::<PathBuf>("skopeo").unwrap().clone(),
        buildkit_host: args.get_one::<String>("buildkit_host").cloned(),
        registry_auth_file: args.get_one::<PathBuf>("registry_auth_file").cloned(),
        secrets: vec![],
    };
    for secret in args.get_many::<String>("build_secret").into_iter().flatten() {
        let (name, file) = secret
            .split_once('=')
            .context("build secret must be NAME=FILE, not a value")?;
        ensure!(!file.is_empty(), "build secret file is missing");
        tools.secrets.push(BuildSecret {
            name: name.into(),
            file: file.into(),
        });
    }
    let (os, architecture) = args.get_one::<String>("platform").unwrap().split_once('/').unwrap();
    let cancel = CancellationToken::new();
    let prepared = {
        let prepare = prepare_container(
            &declaration,
            &loaded.config_dir,
            spacetimedb_lib::container::ImagePlatform {
                os: os.into(),
                architecture: architecture.into(),
            },
            &tools,
            parent,
            &LocalRunner,
            cancel.clone(),
        );
        tokio::pin!(prepare);
        tokio::select! {
            result = &mut prepare => result?,
            signal = tokio::signal::ctrl_c() => {
                signal?;
                cancel.cancel();
                let _ = prepare.await;
                anyhow::bail!("container build cancelled; no output was published");
            }
        }
    };
    let digest = prepared.metadata.manifest.digest;
    prepared.persist(&output)?;
    println!("Prepared {digest} at {}", output.display());
    Ok(())
}
