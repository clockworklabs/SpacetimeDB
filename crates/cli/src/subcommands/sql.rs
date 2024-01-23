use std::time::Instant;

use crate::api::{from_json_seed, ClientApi, Connection, StmtResultJson};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches};
use itertools::Itertools;
use reqwest::RequestBuilder;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::{satn, Typespace};
use tabled::settings::Style;

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
        auth_header: get_auth_header_only(&mut config, anon_identity, as_identity, server).await?,
        address: database_address(&config, database, server).await?,
        database: database.to_string(),
    })
}

// Need to report back timings from each query from the backend instead of infer here...
fn print_row_count(rows: usize) -> String {
    let txt = if rows == 1 { "row" } else { "rows" };
    format!("({rows} {txt})")
}

fn print_timings(now: Instant) {
    println!("Time: {:.2?}", now.elapsed());
}

pub(crate) async fn run_sql(builder: RequestBuilder, sql: &str, with_stats: bool) -> Result<(), anyhow::Error> {
    let now = Instant::now();

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
        if with_stats {
            print_timings(now);
        }
        println!("OK");
        return Ok(());
    };

    stmt_result_json
        .iter()
        .map(|stmt_result| {
            let mut table = stmt_result_to_table(stmt_result)?;
            if with_stats {
                // The `tabled::count_rows` add the header as a row, so subtract it.
                let row_count = print_row_count(table.count_rows().wrapping_sub(1));
                table.with(tabled::settings::panel::Footer::new(row_count));
            }
            anyhow::Ok(table)
        })
        .process_results(|it| println!("{}", it.format("\n\n")))?;
    if with_stats {
        print_timings(now);
    }

    Ok(())
}

fn stmt_result_to_table(stmt_result: &StmtResultJson) -> anyhow::Result<tabled::Table> {
    let StmtResultJson { schema, rows } = stmt_result;

    let mut builder = tabled::builder::Builder::default();
    builder.set_header(
        schema
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| e.name.clone().unwrap_or_else(|| format!("column {i}"))),
    );

    let ty = Typespace::EMPTY.with_type(schema);
    for row in rows {
        let row = from_json_seed(row.get(), SeedWrapper(ty))?;
        builder.push_record(
            ty.with_values(&row)
                .map(|col_val| satn::PsqlWrapper(col_val).to_string()),
        );
    }

    let mut table = builder.build();
    table.with(Style::psql());

    Ok(table)
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

        run_sql(api.sql(), query, false).await?;
    }
    Ok(())
}
