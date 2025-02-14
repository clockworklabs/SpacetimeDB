use crate::common_args;
use crate::config::Config;
use crate::util::{add_auth_header_opt, decode_identity, get_auth_header, get_login_token_or_log_in, ResponseExt};
use clap::ArgMatches;
use clap::{Arg, Command};

use spacetimedb_client_api_messages::name::{DomainName, InsertDomainResult};

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
    let token = get_login_token_or_log_in(&mut config, server, !force).await?;
    let identity = decode_identity(&token)?;
    let auth_header = get_auth_header(&mut config, false, server, !force).await?;

    let domain: DomainName = domain.parse()?;

    let builder = reqwest::Client::new()
        .post(format!(
            "{}/v1/database/{database_identity}/names",
            config.get_host_url(server)?
        ))
        .body(String::from(domain));
    let builder = add_auth_header_opt(builder, &auth_header);

    let result = builder.send().await?.json_or_error().await?;
    match result {
        InsertDomainResult::Success {
            domain,
            database_identity,
        } => {
            println!("Domain set to {} for identity {}.", domain, database_identity);
        }
        InsertDomainResult::TldNotRegistered { domain } => {
            return Err(anyhow::anyhow!(
                "The top level domain that you provided is not registered.\n\
            This tld is not yet registered to any identity. You can register this domain with the following command:\n\
            \n\
            \tspacetime dns register-tld {}\n",
                domain.tld()
            ));
        }
        InsertDomainResult::PermissionDenied { domain } => {
            //TODO(jdetter): Have a nice name generator here, instead of using some abstract characters
            // we should perhaps generate fun names like 'green-fire-dragon' instead
            let suggested_tld: String = identity.chars().take(12).collect();
            if let Some(sub_domain) = domain.sub_domain() {
                return Err(anyhow::anyhow!(
                    "The top level domain {} is not registered to the identity you provided.\n\
                We suggest you register a new tld:\n\
                \tspacetime dns register-tld {}\n\
                \n\
                And then push to the domain that uses that tld:\n\
                \tspacetime publish {}/{}\n",
                    domain.tld(),
                    suggested_tld,
                    suggested_tld,
                    sub_domain
                ));
            } else {
                return Err(anyhow::anyhow!(
                    "The top level domain {} is not registered to the identity you provided.\n\
                We suggest you register a new tld:\n\
                \tspacetime dns register-tld {}\n\
                \n\
                And then push to the domain that uses that tld:\n\
                \tspacetime publish {}\n",
                    domain.tld(),
                    suggested_tld,
                    suggested_tld
                ));
            }
        }
        InsertDomainResult::OtherError(e) => return Err(anyhow::anyhow!(e)),
    }

    Ok(())
}
