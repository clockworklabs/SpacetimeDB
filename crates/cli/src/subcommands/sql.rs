use std::fmt;
use std::time::Instant;

use crate::api::{from_json_seed, ClientApi, Connection, StmtResultJson, StmtStats};
use crate::common_args;
use crate::config::Config;
use crate::util::{database_identity, get_auth_header, ResponseExt, UNSTABLE_WARNING};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches};
use itertools::Itertools;
use reqwest::RequestBuilder;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::{satn, Typespace};
use tabled::settings::Style;

pub fn cli() -> clap::Command {
    clap::Command::new("sql")
        .about(format!("Runs a SQL query on the database. {}", UNSTABLE_WARNING))
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

struct StmtResult {
    table: tabled::Table,
    stats: Option<StmtStats>,
    time_client: Instant,
}

impl fmt::Display for StmtResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let has_table = !self.table.is_empty();
        if has_table {
            write!(f, "{}", self.table)?;
        }

        if let Some(stats) = &self.stats {
            if has_table {
                writeln!(f)?;
            }
            let txt = if stats.total_rows == 1 { "row" } else { "rows" };

            let result = format!("({} {txt})", stats.total_rows);
            let mut info = Vec::new();
            if stats.rows_scanned != 0 {
                info.push(format!("scan: {}", stats.rows_scanned));
            }
            if stats.rows_inserted != 0 {
                info.push(format!("ins: {}", stats.rows_inserted));
            }
            if stats.rows_deleted != 0 {
                info.push(format!("del: {}", stats.rows_deleted));
            }
            if stats.rows_updated != 0 {
                info.push(format!("upd: {}", stats.rows_updated));
            }
            info.push(format!(
                "server: {:.2?}",
                std::time::Duration::from_micros(stats.total_duration_micros)
            ));
            info.push(format!("client: {:.2?}", self.time_client.elapsed()));

            if !info.is_empty() {
                write!(f, "{result} [{info}]", info = info.join(", "))?;
            } else {
                write!(f, "{result}")?;
            };
        };
        Ok(())
    }
}

pub(crate) async fn run_sql(builder: RequestBuilder, sql: &str, with_stats: bool) -> Result<(), anyhow::Error> {
    let mut now = Instant::now();

    let json = builder
        .body(sql.to_owned())
        .send()
        .await?
        .ensure_content_type("application/json")
        .await?
        .text()
        .await?;

    let stmt_result_json: Vec<StmtResultJson> = serde_json::from_str(&json).context("malformed sql response")?;
    let stats = stmt_result_json.iter().map(StmtStats::from).sum::<StmtStats>();

    // Print only `OK for empty tables as it's likely a command like `INSERT`.
    if stmt_result_json.is_empty() {
        println!(
            "{}",
            StmtResult {
                stats: if with_stats { Some(stats) } else { None },
                table: tabled::Table::new([""]),
                time_client: now,
            }
        );

        println!("OK");
        return Ok(());
    };

    stmt_result_json
        .iter()
        .map(|stmt_result| {
            let (stats, table) = stmt_result_to_table(stmt_result)?;

            let time_client = now;
            now = Instant::now();
            anyhow::Ok(StmtResult {
                stats: if with_stats { Some(stats) } else { None },
                table,
                time_client,
            })
        })
        .process_results(|it| println!("{}", it.format("\n\n")))?;

    Ok(())
}

fn stmt_result_to_table(stmt_result: &StmtResultJson) -> anyhow::Result<(StmtStats, tabled::Table)> {
    let stats = StmtStats::from(stmt_result);
    let StmtResultJson { schema, rows, .. } = stmt_result;

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

    Ok((stats, table))
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    eprintln!("{}\n", UNSTABLE_WARNING);
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
