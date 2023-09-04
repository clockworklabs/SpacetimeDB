use crate::api::{from_json_seed, ClientApi, Connection, StmtResultJson};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches};
use reqwest::RequestBuilder;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::{satn, SatsString, Typespace};
use tabled::builder::Builder;
use tabled::Style;

use crate::config::Config;
use crate::util::{database_address, get_auth_header_only};

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
                .action(ArgAction::Set)
                .required(true)
                .conflicts_with("interactive")
                .help("The SQL query to execute"),
        )
        .arg(Arg::new("interactive")
                 .long("interactive")
                 .action(ArgAction::SetTrue)
                 .conflicts_with("query")
                 .help("Runs an interactive command prompt for `SQL` expressions"),)
        .group(
            ArgGroup::new("mode")
                .args(["interactive","query"])
                .multiple(false)
                .required(true)
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
        .arg(
            Arg::new("server")
                .long("server")
                .short('s')
                .help("The nickname, host name or URL of the server hosting the database"),
        )
}

pub(crate) async fn parse_req(mut config: Config, args: &ArgMatches) -> Result<Connection, anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let database = args.get_one::<String>("database").unwrap();
    let as_identity = args.get_one::<String>("as_identity");
    let anon_identity = args.get_flag("anon_identity");

    Ok(Connection {
        host: config.get_host_url(server)?,
        auth_header: get_auth_header_only(&mut config, anon_identity, as_identity, server).await,
        address: database_address(&config, database, server).await?,
        database: database.to_string(),
    })
}

pub(crate) async fn run_sql(builder: RequestBuilder, sql: &str) -> Result<(), anyhow::Error> {
    let json = builder
        .body(sql.to_owned())
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let stmt_result_json: Vec<StmtResultJson> = serde_json::from_str(&json)?;

    // Print only `OK for empty tables as it's likely a command like `INSERT`.
    if stmt_result_json.is_empty() {
        println!("OK");
        return Ok(());
    };

    for (i, stmt_result) in stmt_result_json.iter().enumerate() {
        let StmtResultJson { schema, rows } = &stmt_result;

        let mut builder = Builder::default();
        builder.set_columns(schema.elements.iter().enumerate().map(|(i, e)| {
            e.name
                .clone()
                .unwrap_or_else(|| SatsString::from_string(format!("column {i}")))
        }));

        let typespace = Typespace::default();
        let ty = typespace.with_type(schema);
        for row in rows {
            let row = from_json_seed(row.get(), SeedWrapper(ty))?;
            builder.add_record(
                row.elements
                    .iter()
                    .zip(&*schema.elements)
                    .map(|(v, e)| satn::PsqlWrapper(ty.with(&e.algebraic_type).with_value(v))),
            );
        }

        let table = builder.build().with(Style::psql());

        if i > 0 {
            println!("\n{}", table);
        } else {
            println!("{}", table);
        }
    }

    Ok(())
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let interactive = args.get_one::<bool>("interactive").unwrap_or(&false);
    if *interactive {
        let con = parse_req(config, args).await?;

        crate::repl::exec(con).await?;
    } else {
        let query = args.get_one::<String>("query").unwrap();

        let con = parse_req(config, args).await?;
        let api = ClientApi::new(con);

        run_sql(api.sql(), query).await?;
    }
    Ok(())
}
