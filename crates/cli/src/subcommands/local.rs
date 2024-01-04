use crate::config::Config;
use clap::ArgAction::SetTrue;
use clap::ArgMatches;
use clap::{Arg, Command};
use spacetimedb::stdb_path;
use std::io::Write;
use std::path::PathBuf;

pub fn cli() -> Command {
    Command::new("local")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_subcommands())
        .about("Manage local SpacetimeDB database")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    exec_subcommand(config, cmd, subcommand_args).await
}

fn get_subcommands() -> Vec<Command> {
    vec![Command::new("clear")
        .about("Deletes all data from the local database")
        .arg(
            Arg::new("force")
                .long("force")
                .action(SetTrue)
                .help("Clear the database without prompting for confirmation"),
        )]
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "clear" => exec_clear(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

async fn exec_clear(_config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let force = args.get_flag("force");
    if std::env::var_os("STDB_PATH").map(PathBuf::from).is_none() {
        let mut path = dirs::home_dir().unwrap_or_default();
        path.push(".spacetime");
        std::env::set_var("STDB_PATH", path.to_str().unwrap());
    }

    let data_dir = stdb_path("data");
    if data_dir.exists() {
        if force {
            std::fs::remove_dir_all(&data_dir)?;
        } else {
            println!("Database path: {}", data_dir.to_str().unwrap());
            print!("Are you sure you want to delete all data from the local database? (y/n) ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes" {
                std::fs::remove_dir_all(&data_dir)?;
            } else {
                println!("Aborting");
                return Ok(());
            }
        }
    } else {
        println!("No local database found in: {}", data_dir.to_str().unwrap());
    }
    Ok(())
}
