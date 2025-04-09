use std::fmt;
use std::fmt::Write;
use std::time::{Duration, Instant};

use crate::api::{from_json_seed, ClientApi, Connection, StmtResultJson, StmtStats};
use crate::common_args;
use crate::config::Config;
use crate::util::{database_identity, get_auth_header, ResponseExt, UNSTABLE_WARNING};
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches};
use reqwest::RequestBuilder;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::{satn, ProductType, ProductValue, Typespace};

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
            if stats.rows_inserted != 0 {
                info.push(format!("inserted: {}", stats.rows_inserted));
            }
            if stats.rows_deleted != 0 {
                info.push(format!("deleted: {}", stats.rows_deleted));
            }
            if stats.rows_updated != 0 {
                info.push(format!("updated: {}", stats.rows_updated));
            }
            info.push(format!(
                "server: {:.2?}",
                std::time::Duration::from_micros(stats.total_duration_micros)
            ));

            if !info.is_empty() {
                write!(f, "{result} [{info}]", info = info.join(", "))?;
            } else {
                write!(f, "{result}")?;
            };
        };
        Ok(())
    }
}

fn print_stmt_result(
    stmt_results: &[StmtResultJson],
    with_stats: Option<Duration>,
    f: &mut String,
) -> anyhow::Result<()> {
    let if_empty: Option<anyhow::Result<StmtResult>> = stmt_results.is_empty().then_some(anyhow::Ok(StmtResult {
        stats: with_stats.is_some().then_some(StmtStats::default()),
        table: tabled::Table::new([""]),
    }));
    let total = stmt_results.len();
    for (pos, result) in if_empty
        .into_iter()
        .chain(stmt_results.iter().map(|stmt_result| {
            let (stats, table) = stmt_result_to_table(stmt_result)?;

            anyhow::Ok(StmtResult {
                stats: with_stats.is_some().then_some(stats),
                table,
            })
        }))
        .enumerate()
    {
        let result = result?;
        f.write_str(&format!("{result}"))?;
        if pos + 1 < total {
            f.write_char('\n')?;
            f.write_char('\n')?;
        }
    }

    if let Some(with_stats) = with_stats {
        f.write_char('\n')?;
        f.write_str(&format!("Roundtrip time: {:.2?}", with_stats))?;
        f.write_char('\n')?;
    }
    Ok(())
}

pub(crate) async fn run_sql(builder: RequestBuilder, sql: &str, with_stats: bool) -> Result<(), anyhow::Error> {
    let now = Instant::now();

    let json = builder
        .body(sql.to_owned())
        .send()
        .await?
        .ensure_content_type("application/json")
        .await?
        .text()
        .await?;

    let stmt_result_json: Vec<StmtResultJson> = serde_json::from_str(&json).context("malformed sql response")?;

    let mut out = String::new();
    print_stmt_result(&stmt_result_json, with_stats.then_some(now.elapsed()), &mut out)?;
    println!("{}", out);

    Ok(())
}

fn stmt_result_to_table(stmt_result: &StmtResultJson) -> anyhow::Result<(StmtStats, tabled::Table)> {
    let stats = StmtStats::from(stmt_result);
    let StmtResultJson { schema, rows, .. } = stmt_result;
    let ty = Typespace::EMPTY.with_type(schema);

    let table = build_table(
        schema,
        rows.iter().map(|row| from_json_seed(row.get(), SeedWrapper(ty))),
    )?;

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

/// Generates a [`tabled::Table`] from a schema and rows, using the style of a psql table.
fn build_table<E>(
    schema: &ProductType,
    rows: impl Iterator<Item = Result<ProductValue, E>>,
) -> Result<tabled::Table, E> {
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
        let row = row?;
        builder.push_record(ty.with_values(&row).enumerate().map(|(idx, value)| {
            let ty = satn::PsqlType {
                tuple: ty.ty(),
                field: &ty.ty().elements[idx],
                idx,
            };

            satn::PsqlWrapper { ty, value }.to_string()
        }));
    }

    let mut table = builder.build();
    table.with(tabled::settings::Style::psql());

    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::StmtStatsJson;
    use itertools::Itertools;
    use serde_json::value::RawValue;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::sats::time_duration::TimeDuration;
    use spacetimedb_lib::sats::timestamp::Timestamp;
    use spacetimedb_lib::sats::{product, GroundSpacetimeType, ProductType};
    use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ConnectionId, Identity};

    fn make_row(row: &[AlgebraicValue]) -> Result<Box<RawValue>, serde_json::Error> {
        let json = serde_json::json!(row);
        RawValue::from_string(json.to_string())
    }

    fn check_outputs(
        result: &[StmtResultJson],
        duration: Option<Duration>,
        expect: &str,
    ) -> Result<String, anyhow::Error> {
        let mut out = String::new();
        print_stmt_result(result, duration, &mut out)?;

        // Need to trim the output to because rustfmt remove the `expect` spaces
        let out = out.lines().map(|line| line.trim_end()).join("\n");
        assert_eq!(out, expect,);

        Ok(out)
    }

    fn check_output(
        schema: ProductType,
        rows: Vec<&RawValue>,
        stats: StmtStatsJson,
        duration: Option<Duration>,
        expect: &str,
    ) -> Result<String, anyhow::Error> {
        let table = StmtResultJson {
            schema: schema.clone(),
            rows,
            total_duration_micros: 1000,
            stats: stats.clone(),
        };

        let mut out = String::new();
        print_stmt_result(&[table], duration, &mut out)?;

        // Need to trim the output to because rustfmt remove the `expect` spaces
        let out = out.lines().map(|line| line.trim_end()).join("\n");
        assert_eq!(out, expect,);

        Ok(out)
    }

    #[test]
    fn test_output() -> Result<(), anyhow::Error> {
        let duration = Duration::from_micros(1000);
        let schema = ProductType::from([("a", AlgebraicType::I32), ("b", AlgebraicType::I64)]);
        let row = make_row(&[AlgebraicValue::I32(1), AlgebraicValue::I64(2)])?;
        // Verify with and without stats
        check_output(
            schema.clone(),
            vec![&row],
            StmtStatsJson {
                rows_inserted: 1,
                rows_deleted: 1,
                rows_updated: 1,
            },
            None,
            r#" a | b
---+---
 1 | 2"#,
        )?;

        check_output(
            schema.clone(),
            vec![&row],
            StmtStatsJson {
                rows_inserted: 1,
                rows_deleted: 1,
                rows_updated: 1,
            },
            Some(duration),
            r#" a | b
---+---
 1 | 2
(1 row) [inserted: 1, deleted: 1, updated: 1, server: 1.00ms]
Roundtrip time: 1.00ms"#,
        )?;

        // Only a query result
        check_output(
            schema.clone(),
            vec![&row],
            StmtStatsJson {
                rows_inserted: 0,
                rows_deleted: 0,
                rows_updated: 0,
            },
            Some(duration),
            r#" a | b
---+---
 1 | 2
(1 row) [server: 1.00ms]
Roundtrip time: 1.00ms"#,
        )?;

        // Empty table
        check_output(
            schema.clone(),
            vec![],
            StmtStatsJson {
                rows_inserted: 0,
                rows_deleted: 0,
                rows_updated: 0,
            },
            Some(duration),
            r#" a | b
---+---
(0 rows) [server: 1.00ms]
Roundtrip time: 1.00ms"#,
        )?;

        // DML
        check_output(
            schema.clone(),
            vec![],
            StmtStatsJson {
                rows_inserted: 1,
                rows_deleted: 0,
                rows_updated: 0,
            },
            Some(duration),
            r#" a | b
---+---
(0 rows) [inserted: 1, server: 1.00ms]
Roundtrip time: 1.00ms"#,
        )?;

        check_output(
            schema.clone(),
            vec![],
            StmtStatsJson {
                rows_inserted: 0,
                rows_deleted: 1,
                rows_updated: 0,
            },
            Some(duration),
            r#" a | b
---+---
(0 rows) [deleted: 1, server: 1.00ms]
Roundtrip time: 1.00ms"#,
        )?;

        check_output(
            schema.clone(),
            vec![],
            StmtStatsJson {
                rows_inserted: 0,
                rows_deleted: 0,
                rows_updated: 1,
            },
            Some(duration),
            r#" a | b
---+---
(0 rows) [updated: 1, server: 1.00ms]
Roundtrip time: 1.00ms"#,
        )?;

        Ok(())
    }

    #[test]
    fn test_multiple_output() -> Result<(), anyhow::Error> {
        let duration = Duration::from_micros(1000);
        let schema = ProductType::from([("a", AlgebraicType::I32), ("b", AlgebraicType::I64)]);
        let row = make_row(&[AlgebraicValue::I32(1), AlgebraicValue::I64(2)])?;

        // Verify with and without stats
        check_outputs(
            &[
                StmtResultJson {
                    schema: schema.clone(),
                    rows: vec![&row],
                    total_duration_micros: 1000,
                    stats: StmtStatsJson {
                        rows_inserted: 1,
                        rows_deleted: 1,
                        rows_updated: 1,
                    },
                },
                StmtResultJson {
                    schema: schema.clone(),
                    rows: vec![&row],
                    total_duration_micros: 1000,
                    stats: StmtStatsJson {
                        rows_inserted: 1,
                        rows_deleted: 1,
                        rows_updated: 1,
                    },
                },
            ],
            Some(duration),
            r#" a | b
---+---
 1 | 2
(1 row) [inserted: 1, deleted: 1, updated: 1, server: 1.00ms]

 a | b
---+---
 1 | 2
(1 row) [inserted: 1, deleted: 1, updated: 1, server: 1.00ms]
Roundtrip time: 1.00ms"#,
        )?;

        Ok(())
    }

    fn expect_psql_table(ty: &ProductType, rows: Vec<ProductValue>, expected: &str) {
        let table = build_table(ty, rows.into_iter().map(Ok::<_, ()>)).unwrap().to_string();
        let mut table = table.split('\n').map(|x| x.trim_end()).join("\n");
        table.insert(0, '\n');
        assert_eq!(expected, table);
    }

    // Verify the output of `sql` matches the inputs that return true for [`AlgebraicType::is_special()`]
    #[test]
    fn output_special_types() -> ResultTest<()> {
        // Check tuples
        let kind: ProductType = [
            AlgebraicType::String,
            AlgebraicType::U256,
            Identity::get_type(),
            ConnectionId::get_type(),
            Timestamp::get_type(),
            TimeDuration::get_type(),
        ]
        .into();
        let value = product![
            "a",
            Identity::ZERO.to_u256(),
            Identity::ZERO,
            ConnectionId::ZERO,
            Timestamp::UNIX_EPOCH,
            TimeDuration::ZERO
        ];

        expect_psql_table(
            &kind,
            vec![value],
            r#"
 column 0 | column 1 | column 2                                                           | column 3                           | column 4                  | column 5
----------+----------+--------------------------------------------------------------------+------------------------------------+---------------------------+-----------
 "a"      | 0        | 0x0000000000000000000000000000000000000000000000000000000000000000 | 0x00000000000000000000000000000000 | 1970-01-01T00:00:00+00:00 | +0.000000"#,
        );

        // Check struct
        let kind: ProductType = [
            ("bool", AlgebraicType::Bool),
            ("str", AlgebraicType::String),
            ("bytes", AlgebraicType::bytes()),
            ("identity", Identity::get_type()),
            ("connection_id", ConnectionId::get_type()),
            ("timestamp", Timestamp::get_type()),
            ("duration", TimeDuration::get_type()),
        ]
        .into();

        let value = product![
            true,
            "This is spacetimedb".to_string(),
            AlgebraicValue::Bytes([1, 2, 3, 4, 5, 6, 7].into()),
            Identity::ZERO,
            ConnectionId::ZERO,
            Timestamp::UNIX_EPOCH,
            TimeDuration::ZERO
        ];

        expect_psql_table(
            &kind,
            vec![value.clone()],
            r#"
 bool | str                   | bytes            | identity                                                           | connection_id                      | timestamp                 | duration
------+-----------------------+------------------+--------------------------------------------------------------------+------------------------------------+---------------------------+-----------
 true | "This is spacetimedb" | 0x01020304050607 | 0x0000000000000000000000000000000000000000000000000000000000000000 | 0x00000000000000000000000000000000 | 1970-01-01T00:00:00+00:00 | +0.000000"#,
        );

        // Check nested struct, tuple...
        let kind: ProductType = [(None, AlgebraicType::product(kind))].into();

        let value = product![value.clone()];

        expect_psql_table(
            &kind,
            vec![value.clone()],
            r#"
 column 0
----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
 (bool = true, str = "This is spacetimedb", bytes = 0x01020304050607, identity = 0x0000000000000000000000000000000000000000000000000000000000000000, connection_id = 0x00000000000000000000000000000000, timestamp = 1970-01-01T00:00:00+00:00, duration = +0.000000)"#,
        );

        let kind: ProductType = [("tuple", AlgebraicType::product(kind))].into();

        let value = product![value];

        expect_psql_table(
            &kind,
            vec![value],
            r#"
 tuple
----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
 (0 = (bool = true, str = "This is spacetimedb", bytes = 0x01020304050607, identity = 0x0000000000000000000000000000000000000000000000000000000000000000, connection_id = 0x00000000000000000000000000000000, timestamp = 1970-01-01T00:00:00+00:00, duration = +0.000000))"#,
        );

        Ok(())
    }
}
