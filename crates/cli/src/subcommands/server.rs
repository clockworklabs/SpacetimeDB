use crate::{util::VALID_PROTOCOLS, Config};
use clap::{Arg, ArgMatches};

pub fn cli() -> clap::Command {
    clap::Command::new("server")
        .about("Changes the host and protocol values for future interactions with spacetimedb")
        .arg(
            Arg::new("url")
                .help("The URL of the SpacetimeDB server to connect to. Example: https://spacetimedb.com")
                .required(true),
        )
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let url = args.get_one::<String>("url").unwrap();

    let protocol: &str;
    let host: &str;

    if url.contains("://") {
        protocol = url.split("://").next().unwrap();
        host = url.split("://").last().unwrap();

        if !VALID_PROTOCOLS.contains(&protocol) {
            return Err(anyhow::anyhow!("Invalid protocol: {}", protocol));
        }
    } else {
        return Err(anyhow::anyhow!("Invalid url: {}", url));
    }

    config.set_host(host);
    config.set_protocol(protocol);

    println!("Host: {}", host);
    println!("Protocol: {}", protocol);

    config.save();

    Ok(())
}
