use std::time::Instant;

use crate::api::{from_json_seed, ClientApi, Connection, StmtResultJson};
use crate::common_args;
use clap::{Arg, ArgAction, ArgMatches};
use itertools::Itertools;
use reqwest::RequestBuilder;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::{satn, Typespace};
use tabled::settings::Style;

use crate::config::Config;
use crate::errors::error_for_status;
use crate::util::{database_identity, get_auth_header, UNSTABLE_HELPTEXT};

pub fn cli() -> clap::Command {
    clap::Command::new("sql")
        .about(format!("Runs a SQL query on the database.\n\n{}", UNSTABLE_HELPTEXT))
        .arg(
            Arg::new("database")
                .required(true)
                .help("The name or identity of the database you would like to query"),
        )
        .arg(
            Arg::new("query")
                .action(ArgAction::Set)
                .required(true)
                .conflicts_with("interactive")
                .help("The SQL query to execute"),
        )
        .arg(
            Arg::new("interactive")
                .long("interactive")
                .action(ArgAction::SetTrue)
                .conflicts_with("query")
                .help("Instead of using a query, run an interactive command prompt for `SQL` expressions"),
        )
        .arg(common_args::anonymous())
        .arg(common_args::server().help("The nickname, host name or URL of the server hosting the database"))
        .arg(common_args::yes())
}

pub(crate) async fn parse_req(mut config: Config, args: &ArgMatches) -> Result<Connection, anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let force = args.get_flag("force");
    let database_name_or_identity = args.get_one::<String>("database").unwrap();
    let anon_identity = args.get_flag("anon_identity");

    Ok(Connection {
        host: config.get_host_url(server)?,
        auth_header: get_auth_header(&mut config, anon_identity, server, !force).await?,
        database_identity: database_identity(&config, database_name_or_identity, server).await?,
        database: database_name_or_identity.to_string(),
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

    let json = error_for_status(builder.body(sql.to_owned()).send().await?)
        .await?
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
            .map(|(i, e)| e.name.clone().unwrap_or_else(|| format!("column {i}").into())),
    );

    let ty = Typespace::EMPTY.with_type(schema);
    for row in rows {
        let row = from_json_seed(row.get(), SeedWrapper(ty))?;
        builder.push_record(
            ty.with_values(&row)
                .map(|value| satn::PsqlWrapper { ty: ty.ty(), value }.to_string()),
        );
    }

    let mut table = builder.build();
    table.with(Style::psql());

    Ok(table)
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    println!("{}", UNSTABLE_HELPTEXT);
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
