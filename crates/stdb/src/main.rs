use clap::error::ContextKind;
use clap::error::ContextValue;
use clap::ArgMatches;
use clap::Command;
use std::vec;

mod address;
mod call;
mod energy;
mod init;
mod login;
mod logs;
mod metrics;
mod query;
mod revert;
mod signup;
mod update;

fn main() {
    match main_app().try_get_matches() {
        Ok(args) => match args.subcommand() {
            Some((cmd, subcommand_args)) => {
                let func = builtin_exec(cmd).unwrap();
                func(subcommand_args);
            }

            None => {
                panic!("No subcommand found!")
            }
        },

        Err(e) => {
            if e.kind() == clap::ErrorKind::UnrecognizedSubcommand {
                let cmd = e
                    .context()
                    .find_map(|c| match c {
                        (ContextKind::InvalidSubcommand, &ContextValue::String(ref cmd)) => {
                            Some(cmd)
                        }
                        _ => None,
                    })
                    .expect("UnrecognizedSubcommand implies the presence of InvalidSubcommand");

                println!("invalid command: {}", cmd);
            } else {
                let _ = e.print();
			}
        }
    }
}

fn main_app() -> Command<'static> {
    Command::new("stdb")
        .allow_external_subcommands(true)
        .subcommands(builtin())
        .override_usage("stdb [OPTIONS] [SUBCOMMAND]")
        .help_template(
            "\
Client program for SpacetimeDB

Usage: {usage}

Options:
{options}

Some common SpacetimeDB commands are
    signup      Creates a new SpacetimeDB identity using your email
    login       Login using an existing identity
    init        Initializes a new project
    update      ???
    logs        Prints logs from a SpacetimeDB database
    energy      Invokes commands related to energy
    revert      Reverts the database to a given point in time
    query       Run a SQL query on the database
    call        Invokes a SpacetimeDB function
    address     ???
    metrics     Prints metrics",
        )
}

fn builtin() -> Vec<Command<'static>> {
    vec![
        address::cli(),
        call::cli(),
        energy::cli(),
        init::cli(),
        login::cli(),
        logs::cli(),
        metrics::cli(),
        query::cli(),
        revert::cli(),
        signup::cli(),
        update::cli(),
    ]
}

fn builtin_exec(cmd: &str) -> Option<fn(&ArgMatches)> {
    let f = match cmd {
        "address" => address::exec,
        "call" => call::exec,
        "energy" => energy::exec,
        "init" => init::exec,
        "login" => login::exec,
        "logs" => logs::exec,
        "metrics" => metrics::exec,
        "query" => query::exec,
        "revert" => revert::exec,
        "signup" => signup::exec,
        "update" => update::exec,
        _ => return None,
    };
    Some(f)
}
