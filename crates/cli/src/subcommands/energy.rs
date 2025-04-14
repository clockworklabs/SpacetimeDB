// use clap::Arg;
use crate::common_args;
use clap::ArgMatches;

use crate::config::Config;
use crate::util::{self, get_login_token_or_log_in, UNSTABLE_WARNING};

pub fn cli() -> clap::Command {
    clap::Command::new("energy")
        .about(format!(
            "Invokes commands related to database budgets. {}",
            UNSTABLE_WARNING
        ))
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
        .subcommands(get_energy_subcommands())
}

fn get_energy_subcommands() -> Vec<clap::Command> {
    vec![clap::Command::new("balance")
        .about("Show current energy balance for an identity")
        .arg(
            common_args::identity()
                .help("The identity to check the balance for")
                .long_help(
                    "The identity to check the balance for. If no identity is provided, the default one will be used.",
                ),
        )
        .arg(
            common_args::server()
                .help("The nickname, host name or URL of the server from which to request balance information"),
        )
        .arg(
            clap::Arg::new("cert")
            .long("cert")
            .value_name("FILE")
            .action(clap::ArgAction::Set)
            .value_parser(clap::value_parser!(std::path::PathBuf))
            .help("Path to the server’s self-signed certificate or CA certificate (PEM format) to trust"),
        )
        .arg(common_args::yes())]
}

async fn exec_subcommand(config: Config, cmd: &str, args: &ArgMatches) -> Result<(), anyhow::Error> {
    match cmd {
        "balance" => exec_status(config, args).await,
        unknown => Err(anyhow::anyhow!("Invalid subcommand: {}", unknown)),
    }
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let (cmd, subcommand_args) = args.subcommand().expect("Subcommand required");
    eprintln!("{}\n", UNSTABLE_WARNING);
    exec_subcommand(config, cmd, subcommand_args).await
}

async fn exec_status(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // let project_name = args.value_of("project name").unwrap();
    let identity = args.get_one::<String>("identity");
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let force = args.get_flag("force");
    // TODO: We should remove the ability to call this for arbitrary users. At *least* remove it from the CLI.
    let identity = if let Some(identity) = identity {
        identity.clone()
    } else {
        let token = get_login_token_or_log_in(&mut config, server, !force).await?;
        util::decode_identity(&token)?
    };
    let cert: Option<&std::path::Path> = args.get_one::<std::path::PathBuf>("cert").map(|p| p.as_path());
    let client = crate::util::build_client(cert).await?;

    let status = client
        .get(format!("{}/v1/energy/{}", config.get_host_url(server)?, identity))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    println!("{}", status);

    Ok(())
}
