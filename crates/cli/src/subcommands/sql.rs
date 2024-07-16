use std::iter;
use std::time::{Duration, Instant};

use crate::api::{from_json_seed, ClientApi, Connection, StmtResultJson};
use crate::common_args;
use crate::format::{self, arg_output_format, fmt_row_psql, get_arg_output_format, OutputFormat, Render};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches};
use indexmap::IndexMap;
use reqwest::RequestBuilder;
use serde_json::json;
use spacetimedb::json::client_api;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::sats::Typespace;
use tabled::settings::panel::Footer;
use tabled::settings::Style;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

use crate::config::Config;
use crate::errors::error_for_status;
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
            arg_output_format("table")
                .long_help("How to render the output of queries. Not available in interactive mode.")
        )
        .arg(
            common_args::identity()
                .conflicts_with("anon_identity")
                .help("The identity to use for querying the database")
                .long_help("The identity to use for querying the database. If no identity is provided, the default one will be used."),
        )
        .arg(
            Arg::new("anon_identity")
                .long("anon-identity")
                .short('a')
                .conflicts_with("identity")
                .action(ArgAction::SetTrue)
                .help("If this flag is present, no identity will be provided when querying the database")
        )
        .arg(common_args::server()
                .help("The nickname, host name or URL of the server hosting the database"),
        )
}

pub(crate) async fn parse_req(mut config: Config, args: &ArgMatches) -> Result<Connection, anyhow::Error> {
    let server = args.get_one::<String>("server").map(|s| s.as_ref());
    let database = args.get_one::<String>("database").unwrap();
    let identity = args.get_one::<String>("identity");
    let anon_identity = args.get_flag("anon_identity");

    Ok(Connection {
        host: config.get_host_url(server)?,
        auth_header: get_auth_header_only(&mut config, anon_identity, identity, server).await?,
        address: database_address(&config, database, server).await?,
        database: database.to_string(),
    })
}

#[derive(Clone, Copy)]
pub struct RenderOpts {
    pub with_timing: bool,
    pub with_row_count: bool,
}

struct Output<'a> {
    stmt_results: Vec<StmtResultJson<'a>>,
    elapsed: Duration,
    opts: RenderOpts,
}

impl Output<'_> {
    pub async fn render(self, fmt: OutputFormat, out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        format::render(self, fmt, out).await
    }

    fn fmt_timing(&self) -> String {
        format!("Time: {:.2?}\n", self.elapsed)
    }

    fn sep_by<'a>(&self, s: &'a [u8]) -> impl Iterator<Item = Option<&'a [u8]>> {
        iter::repeat(Some(s))
            .take(self.stmt_results.len().saturating_sub(1))
            .chain([None])
    }
}

impl Render for Output<'_> {
    /// Renders each query result as a JSON object, delimited as [`json-seq`].
    ///
    /// The objects have a single field `rows`, which is an array of the result
    /// values keyed by their column names.
    ///
    /// `json-seq` can be decoded by `jq --seq` in a streaming fashion.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM Message; SELECT * FROM Users;` against the
    /// `quickstart-chat` application may return the following:
    ///
    /// ```json
    /// {"rows":[{"sender":["aba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993"],"sent":1718611618273455,"text":"hello"}]}
    /// {"rows":[{"identity":["aba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993"],"name":{"1":[]},"online":true}]}
    /// ```
    ///
    /// [json-seq]: https://datatracker.ietf.org/doc/html/rfc7464
    async fn render_json(self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        for stmt_result in &self.stmt_results {
            let client_api::StmtResultJson { schema, rows } = stmt_result.try_into()?;

            let col_names = schema.names().collect::<Vec<_>>();
            let rows = rows
                .into_iter()
                .map(|row| {
                    // NOTE: Column names may actually be column positions.
                    // Render all columns in position order, to make it less
                    // confusing when inspecting visually.
                    // TODO: Use a weaker hash algorithm?
                    col_names.iter().zip(row).collect::<IndexMap<_, _>>()
                })
                .collect::<Vec<_>>();

            format::write_json_seq(&mut out, &json!({"rows": rows})).await?;
        }
        out.flush().await?;

        Ok(())
    }

    /// Renders each query result as an ASCII table similar in style to `psql`.
    ///
    /// If `with_timing` is given, timing information about the whole request
    /// is printed after all tables have been rendered.
    ///
    /// If `with_row_count` is given, the number of rows is printed in the
    /// footer of each table.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM Message; SELECT * FROM Users;` against the
    /// `quickstart-chat` application may return the following:
    ///
    /// ```text
    ///  sender                                                             | sent             | text
    /// --------------------------------------------------------------------+------------------+----------------------
    ///  0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | 1718611618273455 | "hello"
    ///
    ///  identity                                                           | name        | online
    /// --------------------------------------------------------------------+-------------+--------
    ///  0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993 | (none = ()) | true
    /// ```
    async fn render_tabled(self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        // Print only `OK for empty tables as it's likely a command like `INSERT`.
        if self.stmt_results.is_empty() {
            if self.opts.with_timing {
                out.write_all(self.fmt_timing().as_bytes()).await?;
            }
            out.write_all(b"OK").await?;
            return Ok(());
        }

        for (stmt_result, sep) in self.stmt_results.iter().zip(self.sep_by(b"\n\n")) {
            let mut table = stmt_result_to_table(stmt_result)?;
            if self.opts.with_row_count {
                let rows = stmt_result.rows.len();
                table.with(Footer::new(format!(
                    "({} {})",
                    rows,
                    if rows == 1 { "row" } else { "rows" }
                )));
            }
            out.write_all(table.to_string().as_bytes()).await?;
            if let Some(sep) = sep {
                out.write_all(sep).await?;
            }
        }

        if self.opts.with_timing {
            out.write_all(self.fmt_timing().as_bytes()).await?;
        }
        out.flush().await?;
        Ok(())
    }

    /// Renders each query result as a sequence of CSV-formatted lines.
    ///
    /// The column names are printed as a comment (a line starting with '#') at
    /// the beginning of each row.
    ///
    /// # Example
    ///
    /// A query `SELECT * FROM Message; SELECT * FROM Users;` against the
    /// `quickstart-chat` application may return the following:
    ///
    /// ```text
    /// # sender,sent,text
    /// 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993,1718611618273455,hello
    /// # identity,name,online
    /// 0xaba52919637a60eb336e76eed40843653bfb3d9d94881d78ab13dda56363b993,(none = ()),true
    /// ```
    async fn render_csv(self, mut out: impl AsyncWrite + Unpin) -> anyhow::Result<()> {
        for stmt_result in &self.stmt_results {
            let mut csv = csv_async::AsyncWriter::from_writer(&mut out);
            let StmtResultJson { schema, rows } = stmt_result;
            let ts = Typespace::EMPTY.with_type(schema);

            // Render column names as a comment, e.g.
            // `# sender,sent,text`
            for (i, name) in schema.names().enumerate() {
                if i == 0 {
                    csv.write_field(format!("# {}", name)).await?;
                } else {
                    csv.write_field(&*name).await?;
                }
            }
            // Terminate record.
            csv.write_record(None::<&[u8]>).await?;

            for row in rows {
                let row = from_json_seed(row.get(), SeedWrapper(ts))?;
                for field in fmt_row_psql(&row, ts) {
                    // Remove quotes around string values to prevent quoting.
                    csv.write_field(field.trim_matches('"')).await?;
                }
                // Terminate record.
                csv.write_record(None::<&[u8]>).await?;
            }
            csv.flush().await?;
        }

        out.flush().await?;
        Ok(())
    }
}

pub(crate) async fn run_sql(
    builder: RequestBuilder,
    sql: &str,
    opts: RenderOpts,
    fmt: OutputFormat,
) -> Result<(), anyhow::Error> {
    let now = Instant::now();

    let json = error_for_status(builder.body(sql.to_owned()).send().await?)
        .await?
        .text()
        .await?;
    let stmt_results: Vec<StmtResultJson> = serde_json::from_str(&json)?;

    Output {
        stmt_results,
        elapsed: now.elapsed(),
        opts,
    }
    .render(fmt, tokio::io::stdout())
    .await?;

    Ok(())
}

fn stmt_result_to_table(stmt_result: &StmtResultJson) -> anyhow::Result<tabled::Table> {
    let StmtResultJson { schema, rows } = stmt_result;

    let mut builder = tabled::builder::Builder::default();
    builder.push_record(schema.names());

    let ty = Typespace::EMPTY.with_type(schema);
    for row in rows {
        let row = from_json_seed(row.get(), SeedWrapper(ty))?;
        builder.push_record(fmt_row_psql(&row, ty));
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
        let fmt = get_arg_output_format(args);
        let con = parse_req(config, args).await?;
        let api = ClientApi::new(con);
        let opts = RenderOpts {
            with_timing: false,
            with_row_count: false,
        };

        run_sql(api.sql(), query, opts, fmt).await?;
    }
    Ok(())
}
