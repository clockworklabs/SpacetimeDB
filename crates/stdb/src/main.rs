use std::vec;
use clap::Command;
use clap::ArgMatches;
use clap::error::ContextKind;
use clap::error::ContextValue;

mod signup;

fn main() {
    match main_app().try_get_matches() {
        Ok(args) => {
            match args.subcommand() {
                Some((cmd, subcommand_args)) => {
                    let func = builtin_exec(cmd).unwrap();
                    func(subcommand_args);
                }

                None => {
                    panic!("No subcommand found!")
                }
            }

        }

        Err(e) => {
            if e.kind() == clap::ErrorKind::UnrecognizedSubcommand {
                let cmd = e.context()
                    .find_map(|c| match c {
                        (ContextKind::InvalidSubcommand, &ContextValue::String(ref cmd)) => {
                            Some(cmd)
                        }
                        _ => None
                    })
                    .expect("UnrecognizedSubcommand implies the presence of InvalidSubcommand");

                    println!("invalid command: {}", cmd)
            }

        }
    }

}

fn main_app() -> Command<'static> {
    Command::new("stdb")
        .allow_external_subcommands(true)
        .subcommands(builtin())
        .override_usage("stdb [OPTIONS] [SUBCOMMAND]")
        .help_template("\
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
    metrics     Prints metrics")
        }

fn builtin() -> Vec<Command<'static>> {
    vec![
        signup::cli(),
    ]
}

fn builtin_exec(cmd: &str) -> Option<fn(&ArgMatches)> {
    let f = match cmd {
        "signup" => signup::exec,
        _ => return None,
    };
    Some(f)
}
