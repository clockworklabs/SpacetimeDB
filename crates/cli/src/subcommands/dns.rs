use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, get_auth_header, prepend_root_database_namespace, ResponseExt};
use anyhow::ensure;
use clap::ArgMatches;
use clap::{Arg, Command};
use reqwest::StatusCode;

use spacetimedb_client_api_messages::name::{parse_database_name, parse_domain_name, DomainName, SetDomainsResult};

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
    let requested_name = args.get_one::<String>("new-name").unwrap();
    let database_identity = args.get_one::<String>("database-identity").unwrap();
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let force = args.get_flag("force");
    let auth_header = get_auth_header(&mut config, false, server, !force).await?;
    let root_database_namespace = config.root_database_namespace();
    let host_url = config.get_host_url(server)?;
    let client = reqwest::Client::new();

    let current_default_name = get_default_name(&client, &host_url, database_identity, &auth_header).await?;
    let name = resolve_name_for_rename(requested_name, current_default_name.as_ref(), root_database_namespace)?;

    let builder = client
        .post(format!("{host_url}/v1/database/default-name/{database_identity}"))
        .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(name.to_string());
    let builder = add_auth_header_opt(builder, &auth_header);

    let response = builder.send().await?;
    let status = &response.status();
    let result: SetDomainsResult = response.json_or_error().await?;

    if !status.is_success() {
        anyhow::bail!(match result {
            SetDomainsResult::Success => "".to_string(),
            SetDomainsResult::PermissionDenied { domain } => format!("Permission denied for database name: {domain}"),
            SetDomainsResult::PermissionDeniedOnAny { domains } =>
                format!("Permission denied for database names: {domains:?}"),
            SetDomainsResult::DatabaseNotFound => format!("Database {database_identity} not found"),
            SetDomainsResult::NotYourDatabase { .. } =>
                format!("You cannot rename {database_identity} because it is owned by another identity."),
            SetDomainsResult::OtherError(err) => err,
        });
    }

    println!("Name set to {name} for identity {database_identity}.");

    Ok(())
}

async fn get_default_name(
    client: &reqwest::Client,
    host_url: &str,
    database_identity: &str,
    auth_header: &crate::util::AuthHeader,
) -> Result<Option<DomainName>, anyhow::Error> {
    let builder = client.get(format!("{host_url}/v1/database/default-name/{database_identity}"));
    let builder = add_auth_header_opt(builder, auth_header);
    let response = builder.send().await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    response.json_or_error().await.map(Some)
}

fn resolve_name_for_rename(
    requested_name: &str,
    current_default_name: Option<&DomainName>,
    root_database_namespace: Option<&str>,
) -> Result<DomainName, anyhow::Error> {
    if let Some(current_default_name) = current_default_name {
        if current_default_name
            .sub_domain()
            .is_some_and(|sub_domain| sub_domain.contains('/'))
        {
            ensure!(
                !requested_name.contains('/'),
                "Child database rename target cannot contain `/`"
            );
            parse_database_name(requested_name)?;
            let parent = current_default_name
                .as_ref()
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .ok_or_else(|| anyhow::anyhow!("Failed to determine parent database path"))?;
            return parse_domain_name(format!("{parent}/{requested_name}")).map_err(Into::into);
        }
    }

    let qualified = prepend_root_database_namespace(requested_name, root_database_namespace);
    parse_domain_name(qualified).map_err(Into::into)
}
