use crate::{util::VALID_PROTOCOLS, Config};
use clap::{Arg, ArgMatches, Command};

pub fn cli() -> Command {
    Command::new("server")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about("Manage the connection to the SpacetimeDB server")
}

fn get_subcommands() -> Vec<Command> {
    vec![
        Command::new("set")
            .about("Changes the host and protocol values for future interactions with SpacetimeDB")
            .arg(
                Arg::new("url")
                    .help(
                        "The URL of the SpacetimeDB server to connect to. Example: https://spacetimedb.com/spacetimedb",
                    )
                    .required(true),
            ),
        Command::new("show").about("Shows the server that is currently configured"),
        Command::new("ping")
            .about("Checks to see if a SpacetimeDB host is online")
            .arg(
                Arg::new("url")
                    .help(
                        "The URL of the SpacetimeDB server to connect to. Example: https://spacetimedb.com/spacetimedb. If no URL is provided, then the configured server is used."
                    )
                    .required(false),
            ),
    ]
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "set" => exec_set(config, args).await,
        "show" => exec_show(config, args).await,
        "ping" => exec_ping(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub async fn exec_set(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let url = args.get_one::<String>("url").unwrap();

    let protocol: &str;
    let host: &str;

    if url.contains("://") {
        protocol = url.split("://").next().unwrap();
        host = url.split("://").last().unwrap();

        if !VALID_PROTOCOLS.contains(&protocol) {
            return Err(anyhow::anyhow!("Invalid protocol: {}", protocol));
        }
    } else {
        return Err(anyhow::anyhow!("Invalid url: {}", url));
    }

    config.set_host(host);
    config.set_protocol(protocol);

    println!("Host: {}", host);
    println!("Protocol: {}", protocol);

    config.save();

    Ok(())
}

pub async fn exec_show(config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    println!("{}", config.get_host_url());
    Ok(())
}

pub async fn exec_ping(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = match args.get_one::<String>("url") {
        Some(s) => s.clone(),
        None => config.get_host_url(),
    };

    let builder = reqwest::Client::new().get(format!("{}/database/ping", server).as_str());
    builder.send().await?.error_for_status()?;
    println!("Server is online: {}", server);
    Ok(())
}
