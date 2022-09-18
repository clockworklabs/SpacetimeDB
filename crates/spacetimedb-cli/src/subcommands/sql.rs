use clap::arg;
use clap::Arg;
use clap::ArgMatches;
use serde::Deserialize;
use serde::Serialize;
use spacetimedb_bindings::TupleDef;
use spacetimedb_bindings::TypeValue;
use tabled::builder::Builder;
use tabled::Style;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StmtResultJson {
    pub schema: TupleDef,
    pub rows: Vec<Vec<TypeValue>>,
}

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("sql")
        .about("Runs a SQL query on the database.")
        .arg(Arg::new("identity").required(true))
        .arg(Arg::new("name").required(true))
        .arg(Arg::new("query").conflicts_with("filename").required(true))
        .arg(
            arg!(-f --filename <FILENAME> "filename")
                .conflicts_with("query")
                .required(true),
        )
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let hex_identity = args.value_of("identity").unwrap();
    let name = args.value_of("name").unwrap();
    let query = args.value_of("query").unwrap();

    let client = reqwest::Client::new();

    let mut builder = client.post(format!("http://{}/database/{}/{}/sql", config.host, hex_identity, name));

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

    let stmt_result = stmt_result_json.first().unwrap();
    let rows = &stmt_result.rows;
    let schema = &stmt_result.schema;

    let mut builder = Builder::default();
    builder.set_columns(
        schema
            .elements
            .iter()
            .map(|e| e.name.as_ref().map(|s| s.clone()).unwrap_or(format!("{}", e.tag))),
    );

    for row in rows {
        builder.add_record(row);
    }

    let table = builder.build().with(Style::psql());

    println!("{}", table);

    Ok(())
}
