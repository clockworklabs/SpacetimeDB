use crate::config::Config;
use crate::util::spacetime_dns;
use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("describe")
        .about("Describe the structure of a database or entities within it")
        .arg(Arg::new("database").required(true))
        .arg(
            Arg::new("entity_type")
                .required(false)
                .value_parser(["reducer", "table", "repeater"]),
        )
        .arg(Arg::new("entity_name").required(false))
        .arg(Arg::new("brief").long("brief").short('b'))
        .after_help("Run `spacetime help describe` for more detailed information.\n")
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.value_of("database").unwrap();
    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };
    let expand = !args.is_present("brief");

    let res = match args.value_of("entity_name") {
        None => {
            let client = reqwest::Client::new();
            client
                .get(format!("http://{}/database/schema/{}", config.host, address))
                .query(&[("expand", expand)])
                .send()
                .await?
        }
        Some(entity_name) => {
            let entity_type = format!("{}s", args.value_of("entity_type").unwrap());

            let client = reqwest::Client::new();
            client
                .get(format!(
                    "http://{}/database/schema/{}/{}/{}",
                    config.host, address, entity_type, entity_name
                ))
                .query(&[("expand", expand)])
                .send()
                .await?
        }
    };

    let res = res.error_for_status()?;
    let body = res.bytes().await?;
    let str = String::from_utf8(body.to_vec())?;
    println!("{}", str);
    Ok(())
}
