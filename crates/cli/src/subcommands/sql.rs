use anyhow::Context;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use serde::Deserialize;
use serde_json::value::RawValue;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::name::{is_address, DnsLookupResponse};
use spacetimedb_lib::sats::satn;
use spacetimedb_lib::sats::Typespace;
use spacetimedb_lib::TupleDef;
use tabled::builder::Builder;
use tabled::Style;

use crate::config::Config;
use crate::util::get_auth_header;
use crate::util::spacetime_dns;

#[derive(Debug, Clone, Deserialize)]
pub struct StmtResultJson<'a> {
    pub schema: TupleDef,
    #[serde(borrow)]
    pub rows: Vec<&'a RawValue>,
}

pub fn cli() -> clap::Command {
    clap::Command::new("sql")
        .about("Runs a SQL query on the database.")
        .arg(
            Arg::new("database")
                .required(true)
                .help("The domain or address of the database you would like to query"),
        )
        .arg(
            Arg::new("query")
                .required(true)
                .help("The SQL query to execute"),
        )
        .arg(
            Arg::new("as_identity")
                .long("as-identity")
                .short('i')
                .conflicts_with("anon_identity")
                .help("The identity to use for querying the database")
                .long_help("The identity to use for querying the database. If no identity is provided, the default one will be used."),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .conflicts_with("as_identity")
                .action(ArgAction::SetTrue)
                .help("If this flag is present, no identity will be provided when querying the database")
        )
}

pub async fn exec(mut config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let query = args.get_one::<String>("query").unwrap();

    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    let auth_header = get_auth_header(&mut config, anon_identity, as_identity.map(|x| x.as_str()))
        .await
        .map(|x| x.0);

    let address = if is_address(database.as_str()) {
        database.clone()
    } else {
        match spacetime_dns(&config, database).await? {
            DnsLookupResponse::Success { domain: _, address } => address,
            DnsLookupResponse::Failure { domain } => {
                return Err(anyhow::anyhow!("The dns resolution of {} failed.", domain));
            }
        }
    };

    let client = reqwest::Client::new();

    let mut builder = client
        .post(format!("{}/database/sql/{}", config.get_host_url(), address))
        .body(query.to_owned());

    if let Some(auth_header) = auth_header {
        builder = builder.header("Authorization", auth_header);
    }

    let res = builder.send().await?;
    let res = res.error_for_status()?;

    let body = res.bytes().await.unwrap();
    let json = String::from_utf8(body.to_vec()).unwrap();

    let stmt_result_json: Vec<StmtResultJson> = serde_json::from_str(&json).unwrap();

    let stmt_result = stmt_result_json.first().context("Invalid sql query.")?;
    let StmtResultJson { schema, rows } = &stmt_result;

    let mut builder = Builder::default();
    builder.set_columns(
        schema
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| e.name.clone().unwrap_or_else(|| format!("column {i}"))),
    );

    let typespace = Typespace::default();
    let ty = typespace.with_type(schema);
    for row in rows {
        let row = from_json_seed(row.get(), SeedWrapper(ty))?;
        builder.add_record(
            row.elements
                .iter()
                .zip(&schema.elements)
                .map(|(v, e)| satn::Wrapper(ty.with(&e.algebraic_type).with_value(v))),
        );
    }

    let table = builder.build().with(Style::psql());

    println!("{}", table);

    Ok(())
}

fn from_json_seed<'de, T: serde::de::DeserializeSeed<'de>>(
    s: &'de str,
    seed: T,
) -> Result<T::Value, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_str(s);
    let out = seed.deserialize(&mut de)?;
    de.end()?;
    Ok(out)
}
