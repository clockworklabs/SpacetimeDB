use crate::config::Config;
use crate::util::spacetime_reverse_dns;
use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command {
    clap::Command::new("reversedns")
        .about("Returns the name provided the database address.")
        .arg(Arg::new("address").required(true))
        .after_help("Run `spacetime help reversedns` for more detailed information.\n")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let addr = args.get_one::<String>("address").unwrap();

    let name = if let Ok(name) = spacetime_reverse_dns(&config, addr).await {
        name
    } else {
        return Err(anyhow::anyhow!("Could not find a name for the address: {}", addr));
    };

    println!("{}", name);
    Ok(())
}
