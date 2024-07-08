use crate::common_args;
use crate::config::Config;
use crate::util::database_address;
use clap::{Arg, ArgMatches};
use reqwest::StatusCode;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub fn cli() -> clap::Command {
    clap::Command::new("tracelog")
        .about("Invokes commands related to tracelogs.")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_energy_subcommands())
}

fn get_energy_subcommands() -> Vec<clap::Command> {
    vec![
        clap::Command::new("get")
            .about("Retrieve a copy of the trace log for a database, if tracing is turned on")
            .arg(Arg::new("database").required(true))
            .arg(Arg::new("outputfile").required(true).help("path to write tracelog to"))
            .arg(common_args::server().help("The nickname, host name or URL of the server running tracing")),
        clap::Command::new("stop")
            .about("Stop tracing on a given database")
            .arg(Arg::new("database").required(true))
            .arg(common_args::server().help("The nickname, host name or URL of the server running tracing")),
        clap::Command::new("replay")
            .about("Replay a tracelog on a temporary fresh DB instance on the server")
            .arg(Arg::new("tracefile").required(true).help("path to read tracelog from"))
            .arg(common_args::server().help("The nickname, host name or URL of the server running tracing")),
    ]
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "get" => exec_get(config, args).await,
        "stop" => exec_stop(config, args).await,
        "replay" => exec_replay(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

pub async fn exec_replay(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let tracefile = args.get_one::<String>("tracefile").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    match std::fs::read(tracefile) {
        Ok(o) => {
            let client = reqwest::Client::new();
            let res = client
                .post(format!("{}/tracelog/replay", config.get_host_url(server)?))
                .body(o)
                .send()
                .await?;
            if res.status() != StatusCode::OK {
                println!("Unable to replay log: {}", res.status())
            } else {
                let bytes = res.bytes().await?;
                let json = String::from_utf8(bytes.to_vec()).unwrap();
                println!("{}", json);
            }
        }
        Err(e) => {
            println!("Could not read tracefile: {}", e);
        }
    }
    Ok(())
}

pub async fn exec_stop(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let address = database_address(&config, database, server).await?;

    let client = reqwest::Client::new();
    let res = client
        .post(format!(
            "{}/tracelog/database/{}/stop",
            config.get_host_url(server)?,
            address
        ))
        .send()
        .await?
        .error_for_status()?;
    if res.status() == StatusCode::NOT_FOUND {
        println!("Could not find database {}", address);
        return Ok(());
    }
    if res.status() != StatusCode::OK {
        println!("Error while stopping tracelog for database {}", address);
        return Ok(());
    }
    println!("Stopped tracing on: {}", address);

    Ok(())
}

pub async fn exec_get(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let address = database_address(&config, database, server).await?;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{}/tracelog/database/{}",
            config.get_host_url(server)?,
            address
        ))
        .send()
        .await?;

    let res = res.error_for_status()?;
    if res.status() == StatusCode::NOT_FOUND {
        println!("Could not find tracelog for database {}", address);
        return Ok(());
    }
    if res.status() != StatusCode::OK {
        println!("Error while retrieving tracelog for database {}", address);
        return Ok(());
    }
    let output_filename = args.get_one::<String>("outputfile").unwrap();
    let content = res.bytes().await?;
    {
        let mut output_file = File::create(Path::new(output_filename)).await?;
        output_file.write_all(content.to_vec().as_slice()).await?;
        output_file.flush().await?;
    }
    println!("Wrote {} bytes to {}", content.len(), output_filename);
    Ok(())
}
