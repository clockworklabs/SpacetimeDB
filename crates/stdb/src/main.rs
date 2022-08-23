use clap::error::ContextKind;
use clap::error::ContextValue;
use clap::ArgMatches;
use clap::Command;
use std::process::exit;
use std::vec;

mod call;
mod energy;
mod identity;
mod init;
mod login;
mod logs;
mod metrics;
mod query;
mod revert;
mod rm;
mod signup;
mod update;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let host = "localhost:3000";
    let args = match get_command().try_get_matches() {
        Ok(args) => args,
        Err(e) => {
            if e.kind() == clap::ErrorKind::UnrecognizedSubcommand {
                let cmd = e
                    .context()
                    .find_map(|c| match c {
                        (ContextKind::InvalidSubcommand, &ContextValue::String(ref cmd)) => Some(cmd),
                        _ => None,
                    })
                    .expect("UnrecognizedSubcommand implies the presence of InvalidSubcommand");

                println!("invalid command: {}", cmd);
                exit(0);
            } else {
                e.exit();
            }
        }
    };
    match args.subcommand() {
        Some((cmd, subcommand_args)) => exec_subcommand(host, cmd, subcommand_args).await?,
        None => {
            get_command().print_help().unwrap();
            exit(0);
        }
    }
    Ok(())
}

fn get_command() -> Command<'static> {
    Command::new("stdb")
        .allow_external_subcommands(true)
        .subcommands(get_subcommands())
        .override_usage("stdb [OPTIONS] [SUBCOMMAND]")
        .help_template(
            "\
Client program for SpacetimeDB

Usage: {usage}

Options:
{options}

Some common SpacetimeDB commands are
    init        Initializes a new Spacetime database
    update      Updates the Wasm module of an existing Spacetime database
    rm          Removes the Wasm module of an existing Spacetime database
    logs        Prints logs from a Spacetime database
    call        Invokes a Spacetime function
    identity    Requests a new Spacetime Identity and token
",
        )
    //signup      Creates a new SpacetimeDB identity using your email
    //login       Login using an existing identity
    //energy      Invokes commands related to energy
    //query       Run a SQL query on the database
    //revert      Reverts the database to a given point in time
    //metrics     Prints metrics
}

fn get_subcommands() -> Vec<Command<'static>> {
    vec![
        init::cli(),
        update::cli(),
        rm::cli(),
        logs::cli(),
        call::cli(),
        identity::cli(),
        // TODO
        energy::cli(),
        login::cli(),
        metrics::cli(),
        query::cli(),
        revert::cli(),
        signup::cli(),
    ]
}

async fn exec_subcommand(host: &str, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "identity" => identity::exec(host, args).await,
        "call" => call::exec(host, args).await,
        "energy" => energy::exec(host, args).await,
        "init" => init::exec(host, args).await,
        "rm" => rm::exec(host, args).await,
        "login" => login::exec(host, args).await,
        "logs" => logs::exec(host, args).await,
        "metrics" => metrics::exec(host, args).await,
        "query" => query::exec(host, args).await,
        "revert" => revert::exec(host, args).await,
        "signup" => signup::exec(host, args).await,
        "update" => update::exec(host, args).await,
        _ => Err(anyhow::anyhow!("invalid subcommand")),
    }
}
