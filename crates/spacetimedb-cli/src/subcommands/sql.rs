use std::path::PathBuf;

use anyhow::anyhow;
use clap::arg;
use clap::Arg;
use clap::ArgMatches;
use serde::Deserialize;
use serde::Serialize;
use spacetimedb_lib::{TupleDef, TypeValue};
use tabled::builder::Builder;
use tabled::Style;

use crate::config::Config;
use crate::util::spacetime_dns;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StmtResultJson {
    pub schema: TupleDef,
    pub rows: Vec<Vec<TypeValue>>,
}

pub fn cli() -> clap::Command {
    clap::Command::new("sql")
        .about("Runs a SQL query on the database.")
        .arg(Arg::new("database").required(true))
        .arg(Arg::new("query").conflicts_with("filename").required(true))
        .arg(
            arg!(-f --filename <FILENAME> "filename")
                .value_parser(clap::value_parser!(PathBuf))
                .conflicts_with("query")
                .required(true),
        )
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let database = args.get_one::<String>("database").unwrap();
    let query = args.get_one::<String>("query").unwrap();
    // let filename = args.get_one::<PathBuf>("filename");

    let address = if let Ok(address) = spacetime_dns(&config, database).await {
        address
    } else {
        database.to_string()
    };

    let client = reqwest::Client::new();

    let mut builder = client.post(format!("http://{}/database/sql/{}", config.host, address));

    if let Some(identity_token) = config.get_default_identity_config() {
        builder = builder.basic_auth("token", Some(identity_token.token.clone()));
    } else {
        println!("Missing identity credentials for identity.");
        std::process::exit(0);
    }

    let builder = builder.body(query.to_owned());

    let res = builder.send().await?;
    let res = res.error_for_status()?;

    let body = res.bytes().await.unwrap();
    let json = String::from_utf8(body.to_vec()).unwrap();

    let stmt_result_json: Vec<StmtResultJson> = serde_json::from_str(&json).unwrap();

    let stmt_result = stmt_result_json.first();
    if stmt_result.is_none() {
        return Err(anyhow!("Invalid sql query."));
    }
    let stmt_result = stmt_result.unwrap();
    let rows = &stmt_result.rows;
    let schema = &stmt_result.schema;

    let mut builder = Builder::default();
    builder.set_columns(
        schema
            .elements
            .iter()
            .map(|e| e.name.as_ref().cloned().unwrap_or(format!("{}", e.tag))),
    );

    for row in rows {
        builder.add_record(row.iter().map(ToString::to_string));
    }

    let table = builder.build().with(Style::psql());

    println!("{}", table);

    Ok(())
}
