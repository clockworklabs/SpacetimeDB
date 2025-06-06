use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, get_auth_header, ResponseExt};
use clap::ArgMatches;
use clap::{Arg, Command};

use spacetimedb_client_api_messages::name::{DomainName, SetDomainsResult};

pub fn cli() -> Command {
    Command::new("rename")
        .about("Rename a database")
        .arg(
            Arg::new("new-name")
                .long("to")
                .required(true)
                .help("The new name you would like to assign"),
        )
        .arg(
            Arg::new("database-identity")
                .required(true)
                .help("The database identity to rename"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server on which to set the name"))
        .arg(common_args::yes())
        .after_help("Run `spacetime rename --help` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let domain = args.get_one::<String>("new-name").unwrap();
    let database_identity = args.get_one::<String>("database-identity").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let force = args.get_flag("force");
    let auth_header = get_auth_header(&mut config, false, server, !force).await?;

    let domain: DomainName = domain.parse()?;

    let builder = reqwest::Client::new()
        .put(format!(
            "{}/v1/database/{database_identity}/names",
            config.get_host_url(server)?
        ))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_string(&[&domain])?);
    let builder = add_auth_header_opt(builder, &auth_header);

    let response = builder.send().await?;
    let status = &response.status();
    let result: SetDomainsResult = response.json_or_error().await?;

    if !status.is_success() {
        anyhow::bail!(match result {
            SetDomainsResult::Success => "".to_string(),
            SetDomainsResult::PermissionDenied { domain } => format!("Permission denied for domain: {}", domain),
            SetDomainsResult::PermissionDeniedOnAny { domains } =>
                format!("Permission denied for domains: {:?}", domains),
            SetDomainsResult::DatabaseNotFound => format!("Database {} not found", database_identity),
            SetDomainsResult::NotYourDatabase { .. } => format!(
                "You cannot rename {} because it is owned by another identity.",
                database_identity
            ),
            SetDomainsResult::OtherError(err) => err,
        });
    }

    println!("Name set to {} for identity {}.", domain, database_identity);

    Ok(())
}
