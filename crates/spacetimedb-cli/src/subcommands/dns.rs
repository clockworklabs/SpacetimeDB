use crate::config::Config;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("dns")
        .about("Resolves the address of a SpacetimeDB database.")
        .arg(Arg::new("name").required(true))
        .after_help("Run `stdb help call for more detailed information.\n`")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let name = args.value_of("name").unwrap();
    if let Ok(address) = spacetime_dns(&config, name).await {
        println!("{}", address);
    } else {
        println!("No such database.");
    };
    Ok(())
}
