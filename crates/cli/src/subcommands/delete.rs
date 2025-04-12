use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, database_identity, get_auth_header};
use clap::{Arg, ArgMatches};

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
        .arg(
            Arg::new("cert")
                .long("cert")
                .value_name("FILE")
                .action(clap::ArgAction::Set)
                .value_parser(clap::value_parser!(std::path::PathBuf))
                .help("Path to the server’s self-signed certificate or CA certificate (PEM format) to trust"),
        )
        .after_help("Run `spacetime help delete` for more detailed information.\n")
}



pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let database = args.get_one::<String>("database").unwrap();
    let force = args.get_flag("force");

    let cert: Option<&std::path::Path> = args.get_one::<std::path::PathBuf>("cert").map(|p| p.as_path());

    pub async fn build_client(cert_path: Option<&std::path::Path>) -> anyhow::Result<reqwest::Client> {
        let mut client_builder = reqwest::Client::builder();

        if let Some(path) = cert_path {
            let cert_pem = tokio::fs::read_to_string(path).await
                .map_err(|e| anyhow::anyhow!("Failed to read certificate file {} err: {}", path.display(), e))?;
            let cert = reqwest::Certificate::from_pem(cert_pem.as_bytes())
                .map_err(|e| anyhow::anyhow!("Failed to parse certificate file {} err: {}", path.display(), e))?;
            client_builder = client_builder.add_root_certificate(cert);
        }

        client_builder.build()
            .map_err(|e| anyhow::anyhow!("Failed to build client with cert {:?} err: {}", cert_path, e))
    }

    let client = build_client(cert).await?;
    let identity = database_identity(&config, database, server, &client).await?;

    let builder = client.delete(format!("{}/v1/database/{}", config.get_host_url(server)?, identity));
    let auth_header = get_auth_header(&mut config, false, server, !force).await?;
    let builder = add_auth_header_opt(builder, &auth_header);
    builder.send().await?.error_for_status()?;

    Ok(())
}
