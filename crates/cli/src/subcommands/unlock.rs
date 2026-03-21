use crate::common_args;
use crate::config::Config;
use crate::subcommands::db_arg_resolution::{load_config_db_targets, resolve_database_arg};
use crate::util::{add_auth_header_opt, database_identity, get_auth_header};
use clap::{Arg, ArgMatches};

pub fn cli() -> clap::Command {
    clap::Command::new("unlock")
        .about("Unlock a database to allow deletion")
        .long_about(
            "Unlock a database that was previously locked with `spacetime lock`.\n\n\
             After unlocking, the database can be deleted normally with `spacetime delete`.",
        )
        .arg(
            Arg::new("database")
                .required(false)
                .help("The name or identity of the database to unlock"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(clap::ArgAction::SetTrue)
                .help("Ignore spacetime.json configuration"),
        )
        .after_help("Run `spacetime help unlock` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server_from_cli = args.get_one::<String>("server").map(|s| s.as_ref());
    let no_config = args.get_flag("no_config");
    let database_arg = args.get_one::<String>("database").map(|s| s.as_str());
    let config_targets = load_config_db_targets(no_config)?;
    let resolved = resolve_database_arg(
        database_arg,
        config_targets.as_deref(),
        "spacetime unlock [database] [--no-config]",
    )?;
    let server = server_from_cli.or(resolved.server.as_deref());

    let identity = database_identity(&config, &resolved.database, server).await?;
    let host_url = config.get_host_url(server)?;
    let auth_header = get_auth_header(&mut config, false, server, true).await?;
    let client = reqwest::Client::new();

    let mut builder = client.post(format!("{host_url}/v1/database/{identity}/unlock"));
    builder = add_auth_header_opt(builder, &auth_header);

    let response = builder.send().await?;
    response.error_for_status()?;

    println!("Database {} is now unlocked.", identity);
    Ok(())
}
