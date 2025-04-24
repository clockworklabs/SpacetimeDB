use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, database_identity, get_auth_header, build_client, map_request_error};
use clap::{Arg, ArgMatches};
use std::path::{Path, PathBuf};

pub fn cli() -> clap::Command {
    clap::Command::new("delete")
        .about("Deletes a SpacetimeDB database")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database to delete"),
        )
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(common_args::yes())

        .arg(common_args::trust_server_cert())
        .arg(common_args::client_cert())
        .arg(common_args::client_key())
        .arg(common_args::trust_system_root_store())
        .arg(common_args::no_trust_system_root_store())
        .after_help("Run `spacetime help delete` for more detailed information.\n")
}



pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let database = args.get_one::<String>("database").unwrap();
    let force = args.get_flag("force");

    // TLS arguments
    let trust_server_cert_path: Option<&Path> = args.get_one::<PathBuf>("trust-server-cert").map(|p| p.as_path());
    let client_cert_path: Option<&Path> = args.get_one::<PathBuf>("client-cert").map(|p| p.as_path());
    let client_key_path: Option<&Path> = args.get_one::<PathBuf>("client-key").map(|p| p.as_path());

    // for clients, default to true unless --no-trust-system-root-store
    // because this is used to verify the received server cert which can be signed by public CA
    // thus using system's trust/root store, by default, makes sense.
    let trust_system = !args.get_flag("no-trust-system-root-store");

    let host = config.get_host_url(server)?;
    let client = map_request_error!(
        build_client(
            trust_server_cert_path,
            client_cert_path,
            client_key_path,
            trust_system,
        ).await
        ,host, client_cert_path, client_key_path)
        ?;
    let identity = map_request_error!(
        database_identity(&config, database, server, &client).await
        ,host, client_cert_path, client_key_path)
        ?;

    let builder = client.delete(format!("{}/v1/database/{}", host, identity));
    let auth_header = get_auth_header(&mut config, false, server, !force).await?;
    let builder = add_auth_header_opt(builder, &auth_header);
    builder.send().await?.error_for_status()?;

    Ok(())
}
