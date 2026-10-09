//! Manage the container attached to a database.
use anyhow::{bail, Context};
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};
use reqwest::StatusCode;
use spacetimedb_lib::container::{
    ContainerInfo, ContainerPort, ContainerResources, ContainerSpec, ContainerState, RestartPolicy,
};

use crate::common_args;
use crate::config::Config;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_arg};
use crate::util::{add_auth_header_opt, database_identity, get_auth_header};

pub fn cli() -> Command {
    let target = |command: Command| {
        command
            .arg(
                Arg::new("database")
                    .index(1)
                    .required(false)
                    .help("The name or identity of the database"),
            )
            .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
            .arg(
                Arg::new("no_config")
                    .long("no-config")
                    .action(ArgAction::SetTrue)
                    .help("Ignore spacetime.json configuration"),
            )
    };
    let defaults = ContainerResources::default();
    Command::new("container")
        .about("Manage the container attached to a database")
        .subcommand_required(true)
        .subcommand(target(
            Command::new("set")
                .about("Attach a container to a database, replacing any existing one")
                .arg(Arg::new("image").long("image").required(true).help(
                    "An OCI image reference pinned to a digest, like `name@sha256:<hex>`, or, for a server \
                     that uses your Docker daemon, a local image ID, like `sha256:<hex>`",
                ))
                .arg(
                    Arg::new("env_key")
                        .long("env-key")
                        .action(ArgAction::Append)
                        .help("A database environment key to set in the container (repeatable)"),
                )
                .arg(
                    Arg::new("port")
                        .long("port")
                        .action(ArgAction::Append)
                        .value_parser(parse_port)
                        .help("A port the container serves, as `name=port` (repeatable)"),
                )
                .arg(
                    Arg::new("cpu_millicores")
                        .long("cpu-millicores")
                        .help("CPU limit, in thousandths of a CPU")
                        .value_parser(value_parser!(u32))
                        .default_value(defaults.cpu_millicores.to_string()),
                )
                .arg(
                    Arg::new("memory_bytes")
                        .long("memory-bytes")
                        .help("Memory limit, in bytes; swap is disabled")
                        .value_parser(value_parser!(u64))
                        .default_value(defaults.memory_bytes.to_string()),
                )
                .arg(
                    Arg::new("scratch_bytes")
                        .long("scratch-bytes")
                        .help("Limit on the writable layer, including /tmp, in bytes")
                        .value_parser(value_parser!(u64))
                        .default_value(defaults.scratch_bytes.to_string()),
                )
                .arg(
                    Arg::new("pids_max")
                        .long("pids-max")
                        .help("Limit on Linux tasks, including threads")
                        .value_parser(value_parser!(u32))
                        .default_value(defaults.pids_max.to_string()),
                )
                .arg(
                    Arg::new("restart")
                        .long("restart")
                        .value_parser(["on-failure", "always", "never"])
                        .default_value("on-failure")
                        .help("When to restart the main command after it exits"),
                )
                .arg(
                    Arg::new("command")
                        .index(2)
                        .last(true)
                        .num_args(1..)
                        .help("Replaces the image's entrypoint and command"),
                ),
        ))
        .subcommand(target(Command::new("status").about("Show the database's container")))
        .subcommand(target(Command::new("start").about("Start the database's container")))
        .subcommand(target(Command::new("stop").about("Stop the database's container")))
        .subcommand(target(
            Command::new("remove").about("Detach the container from the database, stopping it"),
        ))
}

fn parse_port(s: &str) -> Result<ContainerPort, String> {
    let (name, port) = s.split_once('=').ok_or("expected `name=port`")?;
    let port = port.parse().map_err(|_| format!("invalid port `{port}`"))?;
    Ok(ContainerPort {
        name: name.to_string(),
        port,
    })
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> anyhow::Result<()> {
    let (subcommand, args) = args.subcommand().context("missing subcommand")?;
    let server_from_cli = args.get_one::<String>("server").map(|s| s.as_ref());
    let config_targets = load_config_db_targets(args.get_flag("no_config"))?;
    let resolved = resolve_database_arg(
        args.get_one::<String>("database").map(|s| s.as_str()),
        config_targets.as_deref(),
        &format!("spacetime container {subcommand} [database] [--no-config]"),
    )?;
    let server = server_from_cli.or(resolved.server.as_deref());

    let identity = database_identity(&config, &resolved.database, server).await?;
    let host_url = config.get_host_url(server)?;
    let auth_header = get_auth_header(&mut config, false, server, true).await?;
    let client = reqwest::Client::new();
    let url = container_url(&host_url, identity);

    let request = match subcommand {
        "set" => {
            let spec = ContainerSpec {
                image: args.get_one::<String>("image").unwrap().clone(),
                command: args
                    .get_many::<String>("command")
                    .map(|command| command.cloned().collect()),
                env_keys: args
                    .get_many::<String>("env_key")
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect(),
                resources: ContainerResources {
                    cpu_millicores: *args.get_one("cpu_millicores").unwrap(),
                    memory_bytes: *args.get_one("memory_bytes").unwrap(),
                    scratch_bytes: *args.get_one("scratch_bytes").unwrap(),
                    pids_max: *args.get_one("pids_max").unwrap(),
                },
                ports: args
                    .get_many::<ContainerPort>("port")
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect(),

                restart: match args.get_one::<String>("restart").map(String::as_str) {
                    Some("always") => RestartPolicy::Always,
                    Some("never") => RestartPolicy::Never,
                    _ => RestartPolicy::OnFailure,
                },
            };
            // The server decides whether it accepts local image IDs.
            spec.validate_with(true).map_err(anyhow::Error::msg)?;
            client.put(&url).json(&spec)
        }
        "status" => client.get(&url),
        "start" => client.post(format!("{url}/start")),
        "stop" => client.post(format!("{url}/stop")),
        "remove" => client.delete(&url),
        _ => unreachable!("unknown container subcommand `{subcommand}`"),
    };
    let response = check_response(add_auth_header_opt(request, &auth_header).send().await?).await?;

    match subcommand {
        "status" => print_info(&response.json::<ContainerInfo>().await?),
        "set" => println!("Container of database {identity} set; it starts once the database's node claims it."),
        "start" => println!("Container of database {identity} will start."),
        "stop" => println!("Container of database {identity} will stop."),
        "remove" => println!("Container removed from database {identity}."),
        _ => {}
    }
    Ok(())
}

/// The URL of the container API of a database.
pub(crate) fn container_url(host_url: &str, database_identity: impl std::fmt::Display) -> String {
    format!("{host_url}/v1/database/{database_identity}/container")
}

/// Pass a successful container API response through, and turn a failed one into an error
/// that says what to do about it.
pub(crate) async fn check_response(response: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    match status {
        StatusCode::NOT_IMPLEMENTED => bail!(
            "{status}: this server does not run containers. If it is a local `spacetime start`, restart it \
             while Docker is running, listening only on loopback (`--listen-addr 127.0.0.1:3000`), \
             or with `--enable-containers`."
        ),
        StatusCode::NOT_FOUND if body.is_empty() => bail!("{status}: this server does not support containers."),
        StatusCode::FORBIDDEN => bail!("{status}: your identity may not manage this database's container here. {body}"),
        _ => bail!("{status}: {body}"),
    }
}

/// A short description of a container's last reported state.
pub(crate) fn describe_state(state: &Option<ContainerState>) -> String {
    match state {
        None => "pending".to_string(),
        Some(ContainerState::Starting) => "starting".to_string(),
        Some(ContainerState::Running) => "running".to_string(),
        Some(ContainerState::Exited(code)) => format!("exited with code {code}"),
        Some(ContainerState::OutOfMemory) => "killed for exceeding its memory limit".to_string(),
        Some(ContainerState::Failed(error)) => format!("failed: {error}"),
    }
}

fn print_info(info: &ContainerInfo) {
    let state = describe_state(&info.state);
    println!("image:      {}", info.spec.image);
    println!("desired:    {}", if info.running { "running" } else { "stopped" });
    println!("generation: {}", info.generation);
    println!("restart:    {:?}", info.spec.restart);
    println!("state:      {state}");
    if let Some(command) = &info.spec.command {
        println!("command:    {}", command.join(" "));
    }
    if !info.spec.env_keys.is_empty() {
        println!("env keys:   {}", info.spec.env_keys.join(", "));
    }
    for port in &info.spec.ports {
        println!("port:       {}={}", port.name, port.port);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_is_valid() {
        // The root `spacetime` command requires help for every argument.
        clap::Command::new("spacetime")
            .help_expected(true)
            .subcommand(super::cli())
            .debug_assert();
    }

    #[test]
    fn set_accepts_a_database_and_a_trailing_command() {
        let image = format!("x@sha256:{}", "a".repeat(64));
        let matches = super::cli()
            .try_get_matches_from(["container", "set", "db", "--image", &image, "--", "/bin/sh", "-c", "x"])
            .unwrap();
        let (_, set) = matches.subcommand().unwrap();
        assert_eq!(set.get_one::<String>("database").unwrap(), "db");
        let command: Vec<_> = set.get_many::<String>("command").unwrap().collect();
        assert_eq!(command, ["/bin/sh", "-c", "x"]);
    }
}
