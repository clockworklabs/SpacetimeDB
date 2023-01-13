use crate::{config::Config, util::get_auth_header};
use clap::{Arg, ArgAction, ArgMatches};

pub fn cli() -> clap::Command {
    clap::Command::new("setname")
        .about("Sets the name of the database.")
        .arg(Arg::new("name").required(true))
        .arg(Arg::new("address").required(true))
        .arg(
            Arg::new("as_identity")
                .long("as-identity")
                .short('i')
                .required(false)
                .conflicts_with("anon_identity"),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .required(false)
                .conflicts_with("as_identity")
                .action(ArgAction::SetTrue),
        )
        .after_help("Run `spacetime help setname` for more detailed information.\n")
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let name = args.get_one::<String>("name").unwrap();
    let address = args.get_one::<String>("address").unwrap();

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let auth_header = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str())).await;

    let client = reqwest::Client::new();

    let mut builder = client.post(format!(
        "{}/database/set_name?name={}&address={}",
        config.get_host_url(),
        name,
        address
    ));

    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }

    let res = builder.send().await?;

    res.error_for_status()?;

    println!("Name set to {} for address {}.", name, address);

    Ok(())
}
