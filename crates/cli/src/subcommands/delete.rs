use crate::config::Config;
use crate::util::{database_address, get_auth_header_only};
use clap::{Arg, ArgMatches};

pub fn cli() -> clap::Command {
    clap::Command::new("delete")
        .about("Deletes a SpacetimeDB database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The domain or address of the database to delete"),
        )
        .arg(
            Arg::new("identity")
                .long("identity")
                .short('i')
                .help("The identity to use for deleting this database")
                .long_help("The identity to use for deleting this database. If no identity is provided, the default one will be used."),
        )
        .after_help("Run `spacetime help delete` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let identity_or_name = args.get_one::<String>("identity");

    let auth_header = get_auth_header_only(&mut config, false, identity_or_name).await;

    let address = database_address(&config, database).await?;

    let client = reqwest::Client::new();
    let mut builder = client.post(format!("{}/database/delete/{}", config.get_host_url(), address));
    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }
    let res = builder.send().await?;

    res.error_for_status()?;

    Ok(())
}
